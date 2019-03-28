#![warn(unused_extern_crates)]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
extern crate hyper;
extern crate serde_json;
#[cfg(test)]
#[macro_use]
extern crate serde_derive;

pub mod error;
