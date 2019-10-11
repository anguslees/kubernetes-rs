#![warn(unused_extern_crates)]

use failure::Error;
use futures::pin_mut;
use futures::stream::TryStreamExt;
use kubernetes_api::core::v1::Pods;
use kubernetes_apimachinery::meta::v1::ListOptions;
use kubernetes_apimachinery::meta::NamespaceScope;
use kubernetes_client::Client;
use pretty_env_logger;
use std::default::Default;
use std::result::Result;

#[runtime::main(runtime_tokio::Tokio)]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let client = Client::new()?;

    let name = NamespaceScope::Namespace("kube-system".to_string());

    // Artificially low `limit`, to demonstrate pagination
    let opts = ListOptions {
        limit: 2,
        ..Default::default()
    };

    let rc = client.resource(Pods);
    let pods = rc.iter(&name, opts);

    pin_mut!(pods);
    while let Some(pod) = pods.try_next().await? {
        let name = pod.metadata.name.unwrap();
        println!("Found name: {}", name);
    }

    Ok(())
}
