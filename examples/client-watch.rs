#![warn(unused_extern_crates)]

extern crate failure;
extern crate futures;
extern crate hyper;
extern crate kubernetes_api;
extern crate kubernetes_apimachinery;
extern crate kubernetes_holding;
#[macro_use]
extern crate log;
extern crate pretty_env_logger;

use failure::Error;
use futures::prelude::*;
use hyper::rt;
use std::result::Result;

use kubernetes_api::core::v1::{ContainerState, Pod, PodList, PodPhase};
use kubernetes_apimachinery::meta::v1::{ListOptions, WatchEvent};
use kubernetes_holding::client::Client;

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

fn main_() -> Result<(), Error> {
    let client = Client::new()?;

    let pods = kubernetes_api::core::v1::GROUP_VERSION.with_resource("pods");
    let namespace = Some("kube-system");

    let work = client
        .list(&pods, namespace, Default::default())
        .inspect(|podlist: &PodList| {
            podlist.items.iter().for_each(print_pod_state);
        })
        .and_then(move |podlist: PodList| {
            debug!(
                "Starting at resource version {}",
                podlist.metadata.resource_version
            );

            let listopts = ListOptions {
                resource_version: podlist.metadata.resource_version,
                ..Default::default()
            };
            client
                .watch_list(&pods, namespace, listopts)
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

    rt::run(work.map_err(|err| panic!("Error: {}", err)));

    Ok(())
}

fn main() {
    pretty_env_logger::init();
    let status = match main_() {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("Error: {}", e);
            for c in e.iter_chain().skip(1) {
                eprintln!(" Caused by {}", c);
            }
            debug!("Backtrace: {}", e.backtrace());
            1
        }
    };
    ::std::process::exit(status);
}
