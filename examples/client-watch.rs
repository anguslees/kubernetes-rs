#![warn(unused_extern_crates)]

#[macro_use]
extern crate log;
use failure::Error;
use futures::future::Future;
use futures::stream::Stream;
use hyper::rt;
use kubernetes_api::core::v1::Pods;
use kubernetes_api::core::v1::{ContainerState, Pod, PodList, PodPhase};
use kubernetes_apimachinery::meta::v1::{ListOptions, WatchEvent};
use kubernetes_apimachinery::meta::NamespaceScope;
use kubernetes_client::Client;
use pretty_env_logger;
use std::result::Result;

fn print_pod_state(p: &Pod) {
    println!(
        "pod {} - {:?}",
        p.metadata
            .name
            .as_ref()
            .map(String::as_str)
            .unwrap_or("(no name)"),
        p.status.phase.unwrap_or(PodPhase::Unknown)
    );
    let c_statuses = p
        .status
        .init_container_statuses
        .iter()
        .chain(p.status.container_statuses.iter());
    for c in c_statuses {
        print!("  -> {}: ", c.name);
        match c.state {
            None => println!("state unknown"),
            Some(ContainerState::Waiting(ref s)) => {
                println!(
                    "waiting: {}",
                    s.message
                        .as_ref()
                        .or(s.reason.as_ref())
                        .map(String::as_str)
                        .unwrap_or("waiting")
                );
            }
            Some(ContainerState::Running(ref s)) => {
                print!("running");
                if let Some(ref t) = s.started_at {
                    print!(" since {}", t);
                }
                println!("");
            }
            Some(ContainerState::Terminated(ref s)) => {
                if let Some(ref msg) = s.message {
                    println!("terminated: {}", msg);
                } else {
                    print!("exited with code {}", s.exit_code);
                    if let Some(ref t) = s.finished_at {
                        print!(" at {}", t);
                    }
                    println!("");
                }
            }
        }
    }
}

fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let client = Client::new()?;

    let name = NamespaceScope::Namespace("kube-system".to_string());

    let list = client
        .resource(Pods)
        .list(
            &name,
            ListOptions {
                limit: 500,
                ..Default::default()
            },
        )
        .inspect(|podlist: &PodList| {
            podlist.items.iter().for_each(print_pod_state);
        });

    let watch = list.and_then(move |podlist: PodList| {
        debug!(
            "Starting at resource version {}",
            podlist.metadata.resource_version
        );

        let listopts = ListOptions {
            resource_version: podlist.metadata.resource_version,
            ..Default::default()
        };
        client
            .resource(Pods)
            .watch(&name, listopts)
            .for_each(|event| {
                match event {
                    WatchEvent::Added(p) | WatchEvent::Modified(p) => {
                        print_pod_state(&p);
                    }
                    WatchEvent::Deleted(p) => {
                        println!("deleted {}", p.metadata.name.unwrap_or("(no name)".into()));
                    }
                    WatchEvent::Error(status) => debug!("Ignoring error event {:#?}", status),
                }
                Ok(())
            })
    });

    rt::run(watch.map_err(|err| panic!("Error: {}", err)));

    Ok(())
}
