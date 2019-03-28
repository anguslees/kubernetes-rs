#![warn(unused_extern_crates)]
#![allow(bare_trait_objects)] // TODO as part of 2018 update

extern crate failure;
extern crate futures;
extern crate http;
#[macro_use]
extern crate serde_derive;
extern crate kubernetes_apimachinery as apimachinery;
extern crate serde_json;
#[cfg(test)]
extern crate serde_yaml;

pub mod apps;
pub mod core;
pub mod policy;
