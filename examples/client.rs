extern crate failure;
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate kubernetes_api;
extern crate kubernetes_holding;
extern crate log;
extern crate pretty_env_logger;
extern crate serde_json;
extern crate tokio;

use failure::Error;
use futures::prelude::*;
use std::result::Result;
use tokio::runtime::current_thread;

use kubernetes_api::core::v1::Pods;
use kubernetes_api::meta::v1::ListOptions;
use kubernetes_holding::client::Client;

fn main_() -> Result<(), Error> {
    let client = Client::new()?;

    let namespace = "kube-system";

    let nsclient = client.namespace(namespace);

    let names_future = nsclient
        .iter(Pods {})
        .map(|pod| pod.metadata.name.unwrap())
        .collect();

    // Artificially low `limit`, to demonstrate pagination
    let opts = ListOptions {
        limit: 2,
        ..Default::default()
    };
    let names_future2 = nsclient
        .iter_opt(Pods {}, opts)
        .map(|pod| pod.metadata.name.unwrap())
        .collect();

    // Resolve future synchronously, for simpler demo
    let mut rt = current_thread::Runtime::new()?;
    let names = rt.block_on(names_future)?;
    let names2 = rt.block_on(names_future2)?;

    assert_eq!(names, names2);

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
