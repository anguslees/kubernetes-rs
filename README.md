# A Kubernetes API client library for Rust

[![Latest Version]][crates.io]

[crates.io]: https://crates.io/crates/kubernetes

## Status

*Experimental.*

- Get, put, list, and watch are implemented, using tokio
  futures/streams.
- Client obeys `~/.kube/config` (or `$KUBECONFIG`) by default, as per
  golang client.  TLS is supported.  Client certificates are the only
  currently supported method of client authentication.
- API objects are currently manually defined and incomplete.
  Additional 3rd-party object types can be defined via traits.
- API resources do not yet have a representation in Rust.
- API error handling is very naive.

---

Example of listing all the pods in `kube-system` namespace.
Results are streamed, limited to 20 results per page.

```rust
extern crate kubernetes;
extern crate tokio_core;

use std::default::Default;
use tokio_core::reactor::Core;

use kubernetes::api;
use kubernetes::client::{Client,ListOptions};
use kubernetes::api::core::v1::{Pod,PodList};

fn main() {
    let mut core = Core::new().unwrap();
    let client = Client::new(2, &core.handle()).unwrap();

    let pods = api::core::v1::GROUP_VERSION.with_resource("pods");
    let namespace = Some("kube-system");

    let opts = ListOptions{ limit: 20, ..Default::default() };
    let work = client.iter::<PodList,Pod>(&pods, namespace, opts)
        .for_each(|pod| {
           println!("pod is {}", pod.metadata.name.unwrap_or_default());
           Ok(())
        });

    core.run(work).unwrap()
}
```
