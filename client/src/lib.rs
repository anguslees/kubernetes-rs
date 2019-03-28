#![warn(unused_extern_crates)]
#![allow(bare_trait_objects)] // TODO as part of 2018 update

extern crate bytes;
#[macro_use]
extern crate failure;
extern crate http;
extern crate kubernetes_apimachinery as apimachinery;
#[macro_use]
extern crate log;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate native_tls;
extern crate openssl;
extern crate serde_yaml;
extern crate tokio_codec;

pub mod config;
pub mod error;

mod client;

pub use client::Client;
