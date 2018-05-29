//! # Kubernetes client

extern crate url;
extern crate serde;
#[cfg_attr(test,macro_use)] extern crate serde_json;
#[macro_use] extern crate serde_derive;
extern crate serde_urlencoded;
extern crate serde_yaml;
#[macro_use] extern crate failure;
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate native_tls;
extern crate tokio_core;
extern crate base64;
extern crate openssl;
#[macro_use] extern crate log;

pub mod api;
pub mod client;
mod groupversion;
mod serde_base64;
mod unstructured;

use std::slice;
use std::borrow::Cow;

use api::meta::v1::{ObjectMeta,ListMeta};

pub use groupversion::*;
pub use unstructured::*;

pub trait Metadata {
    fn api_version(&self) -> &str;
    fn kind(&self) -> &str;
    fn metadata(&self) -> Cow<ObjectMeta>;
}

pub trait List<T> {
    fn listmeta(&self) -> Cow<ListMeta>;
    fn items(&self) -> &[T];
    fn items_mut(&mut self) -> &mut [T];
    fn into_items(self) -> Vec<T>;
}

/*
impl<T> IntoIterator for List<T>
    where List<T>: Sized, T: Sized
{
    type Item = T;
    type IntoIter = vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_items().into_iter()
    }
}
 */

impl<'a, T> IntoIterator for &'a List<T> {
    type Item = &'a T;
    type IntoIter = slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items().into_iter()
    }
}

impl<'a, T> IntoIterator for &'a mut List<T> {
    type Item = &'a mut T;
    type IntoIter = slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items_mut().iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{self,Value};
    use super::Metadata;

    fn pod_json() -> Value {
        json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "name": "pod-example",
            },
            "spec": {
                "containers": [
                    {
                        "image": "busybox",
                        "command": ["echo"],
                        "args": ["Hello world"],
                    },
                ],
            },
        })
    }

    #[test]
    fn untyped() {
        let j = pod_json();
        assert_eq!(j.kind(), "Pod");
        assert_eq!(j.api_version(), "v1");
        assert_eq!(j.metadata().name.as_ref().unwrap(), "pod-example");
    }

    #[test]
    fn typed() {
        use api::core::v1::Pod;
        let pod: Pod = serde_json::from_value(pod_json()).unwrap();
        assert_eq!(pod.spec.containers[0].image, Some("busybox".into()));
    }
}
