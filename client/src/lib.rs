#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
extern crate serde_json;

extern crate hyper;
#[cfg(test)]
extern crate serde;
#[cfg(test)]
#[macro_use]
extern crate serde_derive;

pub mod error;
