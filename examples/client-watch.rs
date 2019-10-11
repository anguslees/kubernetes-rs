#![warn(unused_extern_crates)]

use failure::Error;
use futures::pin_mut;
use futures::stream::TryStreamExt;
use kubernetes_api::core::v1::Pods;
use kubernetes_api::core::v1::{ContainerState, Pod, PodPhase};
use kubernetes_apimachinery::meta::v1::{ListOptions, WatchEvent};
use kubernetes_apimachinery::meta::NamespaceScope;
use kubernetes_client::Client;
use log::debug;
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

#[runtime::main(runtime_tokio::Tokio)]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let client = Client::new()?;

    let name = NamespaceScope::Namespace("kube-system".to_string());

    let podlist = client
        .resource(Pods)
        .list(&name, Default::default())
        .await?;

    let resource_version = podlist.metadata.resource_version;

    for pod in podlist.items {
        print_pod_state(&pod);
    }

    debug!("Starting watch at resource version {}", resource_version);

    let listopts = ListOptions {
        resource_version: resource_version,
        ..Default::default()
    };
    let rc = client.resource(Pods); // FIXME: ugly workaround for "creates a temporary which is freed while still in use"
    let watch = rc.watch(&name, listopts);

    pin_mut!(watch);
    while let Some(event) = watch.try_next().await? {
        match event {
            WatchEvent::Added(p) | WatchEvent::Modified(p) => {
                print_pod_state(&p);
            }
            WatchEvent::Deleted(p) => {
                let name = p.metadata.name.unwrap_or("(no name)".into());
                println!("deleted {}", name);
            }
            WatchEvent::Error(status) => debug!("Ignoring error event {:#?}", status),
        }
    }

    Ok(())
}
