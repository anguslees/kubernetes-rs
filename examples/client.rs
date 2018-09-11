extern crate failure;
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate kubernetes;
extern crate log;
extern crate pretty_env_logger;
extern crate serde_json;
extern crate tokio;

use failure::Error;
use futures::prelude::*;
use std::result::Result;
use tokio::runtime::current_thread;

use kubernetes::api;
use kubernetes::api::core::v1::{Pod, PodList};
use kubernetes::client::{Client, ListOptions};

fn main_() -> Result<(), Error> {
    let client = Client::new()?;

    let pods = api::core::v1::GROUP_VERSION.with_resource("pods");
    let namespace = Some("kube-system");

    // Artificially low `limit`, to demonstrate pagination
    let opts = ListOptions {
        limit: 2,
        ..Default::default()
    };

    let names_future = client
        .iter::<PodList, Pod>(&pods, namespace, opts)
        .map(|pod| pod.metadata.name.unwrap_or("(no name)".into()))
        .collect();

    // Resolve future synchronously, for simpler demo
    let mut rt = current_thread::Runtime::new()?;
    let names = rt.block_on(names_future)?;

    for n in names {
        println!("Found name: {}", n);
    }

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
            eprintln!("{}", e.backtrace());
            1
        }
    };
    ::std::process::exit(status);
}
