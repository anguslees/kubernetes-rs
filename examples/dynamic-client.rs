#![warn(unused_extern_crates)]

use failure::Error;
use futures::stream::Stream;
use kubernetes_apimachinery::meta::v1::{ListOptions, Metadata};
use kubernetes_apimachinery::meta::NamespaceScope;
use kubernetes_apimachinery::unstructured::{DynamicResource, DynamicScope};
use kubernetes_client::Client;
use pretty_env_logger;
use std::default::Default;
use std::result::Result;
use tokio::runtime::current_thread;

fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let client = Client::new()?;

    // In client-go-speak, a "dynamic client" is one which uses
    // entirely runtime k8s type information (eg: read from JSON files
    // and schema introspection).  In Rust, this uses
    // `apimachinery::unstructured::*`.

    // Some values discovered at runtime, perhaps from schema.
    let resource = DynamicResource {
        group: "".to_string(),
        version: "v1".to_string(),
        singular: "pod".to_string(),
        plural: "pods".to_string(),
    };

    let name = DynamicScope::Namespace(NamespaceScope::Namespace("kube-system".to_string()));

    // Artificially low `limit`, to demonstrate pagination
    let opts = ListOptions {
        limit: 2,
        ..Default::default()
    };

    let names_future = client
        .resource(&resource)
        .iter(&name, opts)
        .map(|item| item.metadata().into_owned().name.unwrap())
        .collect();

    // Resolve future synchronously, for simpler demo
    let mut rt = current_thread::Runtime::new()?;
    let names = rt.block_on(names_future)?;

    for n in names {
        println!("Found name: {}", n);
    }

    Ok(())
}
