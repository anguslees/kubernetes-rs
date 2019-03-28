# A Kubernetes API client library for Rust

[crates.io]: https://crates.io/crates/kubernetes

## Structure

* `/apimachinery` Version-agnostic Kubernetes API library.  Includes
  a client implemented using abstract traits, and generic runtime
  `unstructured` objects.
* `/api` Kubernetes release-specific API domain objects modeled as
  static Rust types.
* `/client` a concrete client implementation using hyper.
* `/proxy` to become an (explicit-where-known + unstructured passthrough where
   not) k8s proxy.

## Status

*Experimental.*

- Get, put, list, and watch are implemented, using tokio
  futures/streams.
- Client obeys `~/.kube/config` (or `$KUBECONFIG`) by default, as per
  golang client.  TLS is supported.  Client certificates are the only
  currently supported method of client authentication.
- API objects are currently manually defined and incomplete.
  Additional 3rd-party object types can be defined via traits.
- API error handling is very naive.

---

Example of listing all the pods in `kube-system` namespace.
Results are streamed, limited to 20 results per page.

```rust
use std::default::Default;
use hyper::rt;

use kubernetes_api::core::v1::Pods;
use kubernetes_apimachinery::meta::NamespaceScope;
use kubernetes_apimachinery::meta::v1::ListOptions;
use kubernetes_client::{Client,ListOptions};

fn main() {
    let client = Client::new().unwrap();

    let name = NamespacedScope::Namespace(String::new("kube-system"));

    let work = client
        .resource(Pods)
        .iter(&name, ListOptions{ limit: 20, ..Default::default() })
        .for_each(|pod| {
           println!("pod is {}", pod.metadata.name.unwrap_or_default());
           Ok(())
        });

    rt::run(work.unwrap());
}
```
