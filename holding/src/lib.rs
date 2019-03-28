//! # Kubernetes client

#![warn(unused_extern_crates)]

extern crate serde;
#[cfg_attr(test, macro_use)]
extern crate serde_json;
extern crate url;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate failure;
extern crate base64;
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate native_tls;
extern crate openssl;
#[macro_use]
extern crate log;

extern crate kubernetes_api as api;
extern crate kubernetes_apimachinery as apimachinery;
extern crate kubernetes_client as k8sclient;

pub mod client;
mod serde_base64;
