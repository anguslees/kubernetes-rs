#![warn(unused_extern_crates)]

use failure::Error;
use futures::stream::Stream;
use kubernetes_api::core::v1::Pods;
use kubernetes_apimachinery::meta::v1::ListOptions;
use kubernetes_apimachinery::meta::NamespaceScope;
use kubernetes_client::Client;
use pretty_env_logger;
use std::default::Default;
use std::result::Result;
use tokio::runtime::current_thread;

fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let client = Client::new()?;

    let name = NamespaceScope::Namespace("kube-system".to_string());

    // Artificially low `limit`, to demonstrate pagination
    let opts = ListOptions {
        limit: 2,
        ..Default::default()
    };

    let names_future = client
        .resource(Pods)
        .iter(&name, opts)
        .map(|pod| pod.metadata.name.unwrap())
        .collect();

    // Resolve future synchronously, for simpler demo
    let mut rt = current_thread::Runtime::new()?;
    let names = rt.block_on(names_future)?;

    for n in names {
        println!("Found name: {}", n);
    }

    Ok(())
}
