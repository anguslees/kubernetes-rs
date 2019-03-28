#![warn(unused_extern_crates)]

#[macro_use]
extern crate serde_derive;
extern crate kubernetes_apimachinery as apimachinery;
extern crate serde_json;
#[cfg(test)]
extern crate serde_yaml;

pub mod apps;
pub mod core;
