#![warn(unused_extern_crates)]
#![allow(bare_trait_objects)] // TODO as part of 2018 update

extern crate base64;
#[macro_use]
extern crate failure;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate futures;
extern crate http;
#[cfg(test)]
extern crate kubernetes_api as api;
#[macro_use]
extern crate log;
#[cfg_attr(test, macro_use)]
extern crate serde_json;
extern crate serde_urlencoded;

use crate::request::Request;
use crate::response::Response;
use failure::Error;
use futures::{Future, Sink, Stream};
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use std::sync::Arc;

pub mod client;
pub mod meta;
pub mod request;
mod resplit;
pub mod response;
pub mod serde_base64;
pub mod unstructured;

pub trait ApiService {
    // TODO: add other methods like version, schema introspection, etc.

    fn request<B, O, B2>(
        &self,
        req: Request<B, O>,
    ) -> Box<Future<Item = Response<B2>, Error = Error> + Send>
    where
        B: Serialize + Send + 'static,
        O: Serialize + Send + 'static,
        B2: DeserializeOwned + Send + 'static;

    fn watch<B, O, B2>(&self, req: Request<B, O>) -> Box<Stream<Item = B2, Error = Error> + Send>
    where
        B: Serialize + Send + 'static,
        O: Serialize + Send + 'static,
        B2: DeserializeOwned + Send + 'static;
}

pub trait HttpService {
    type Body: AsRef<[u8]> + IntoIterator<Item = u8>;
    type Future: Future<Item = http::Response<Self::Body>, Error = Error> + Send;
    type StreamFuture: Future<Item = http::Response<Self::Stream>, Error = Error> + Send;
    type Stream: Stream<Item = Self::Body, Error = Error> + Send;

    fn request(&self, req: http::Request<Vec<u8>>) -> Self::Future;

    fn watch(&self, req: http::Request<Vec<u8>>) -> Self::StreamFuture;
}

impl<T: ?Sized + HttpService> HttpService for Box<T> {
    type Body = T::Body;
    type Future = T::Future;
    type StreamFuture = T::StreamFuture;
    type Stream = T::Stream;

    fn request(&self, req: http::Request<Vec<u8>>) -> Self::Future {
        (**self).request(req)
    }

    fn watch(&self, req: http::Request<Vec<u8>>) -> Self::StreamFuture {
        (**self).watch(req)
    }
}

impl<T: ?Sized + HttpService> HttpService for Arc<T> {
    type Body = T::Body;
    type Future = T::Future;
    type StreamFuture = T::StreamFuture;
    type Stream = T::Stream;

    fn request(&self, req: http::Request<Vec<u8>>) -> Self::Future {
        (**self).request(req)
    }

    fn watch(&self, req: http::Request<Vec<u8>>) -> Self::StreamFuture {
        (**self).watch(req)
    }
}

impl<'a, T: ?Sized + HttpService> HttpService for &'a T {
    type Body = T::Body;
    type Future = T::Future;
    type StreamFuture = T::StreamFuture;
    type Stream = T::Stream;

    fn request(&self, req: http::Request<Vec<u8>>) -> Self::Future {
        (**self).request(req)
    }

    fn watch(&self, req: http::Request<Vec<u8>>) -> Self::StreamFuture {
        (**self).watch(req)
    }
}

pub trait HttpUpgradeService {
    // Ideally this trait would use AsyncRead+AsyncWrite, but that requires
    // a dependency on tokio_io (just for those trait declarations).
    // Take the easy/slow path for now and use Stream+Sink instead.
    type Sink: Sink<SinkItem = Self::SinkItem, SinkError = Error> + Send;
    type SinkItem: AsRef<[u8]>;
    type Stream: Stream<Item = Self::StreamItem, Error = Error> + Send;
    type StreamItem: AsRef<[u8]>;
    type Future: Future<Item = (Self::Stream, Self::Sink), Error = Error> + Send;

    fn upgrade(&self, req: http::Request<()>) -> Self::Future;
}

impl<T: HttpUpgradeService> HttpUpgradeService for Arc<T> {
    type Sink = T::Sink;
    type SinkItem = T::SinkItem;
    type Stream = T::Stream;
    type StreamItem = T::StreamItem;
    type Future = T::Future;

    fn upgrade(&self, req: http::Request<()>) -> Self::Future {
        (**self).upgrade(req)
    }
}

impl<T: HttpUpgradeService> HttpUpgradeService for Box<T> {
    type Sink = T::Sink;
    type SinkItem = T::SinkItem;
    type Stream = T::Stream;
    type StreamItem = T::StreamItem;
    type Future = T::Future;

    fn upgrade(&self, req: http::Request<()>) -> Self::Future {
        (**self).upgrade(req)
    }
}

impl<'a, T: HttpUpgradeService> HttpUpgradeService for &'a T {
    type Sink = T::Sink;
    type SinkItem = T::SinkItem;
    type Stream = T::Stream;
    type StreamItem = T::StreamItem;
    type Future = T::Future;

    fn upgrade(&self, req: http::Request<()>) -> Self::Future {
        (**self).upgrade(req)
    }
}
