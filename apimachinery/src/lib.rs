#![warn(unused_extern_crates)]
// async_stream needs all the recursions
#![recursion_limit = "1000"]

use crate::request::Request;
use crate::response::Response;
use async_trait::async_trait;
use failure::Error;
use futures::io::{AsyncRead, AsyncWrite};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use std::ops::Deref;
use std::sync::Arc;

pub mod client;
pub mod meta;
pub mod request;
pub mod response;
pub mod serde_base64;
pub mod unstructured;

#[async_trait]
pub trait ApiService {
    // TODO: add other methods like version, schema introspection, etc.

    async fn request<B, O, B2>(&self, req: Request<B, O>) -> Result<Response<B2>, Error>
    where
        B: Serialize + Send + 'async_trait,
        O: Serialize + Send + 'async_trait,
        B2: DeserializeOwned + Send + 'static;

    fn watch<'a, B, O, B2>(&'a self, req: Request<B, O>) -> BoxStream<'a, Result<B2, Error>>
    where
        B: Serialize + Send + 'a,
        O: Serialize + Send + 'a,
        B2: DeserializeOwned + Send + Unpin + 'static;
}

#[async_trait]
pub trait HttpService {
    type Body: AsRef<[u8]> + IntoIterator<Item = u8>;
    type Read: AsyncRead + Send;

    async fn request(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Body>, Error>;

    // FIXME: this can probably be unified with request(), given the
    // right type constraints
    async fn watch(&self, req: http::Request<Vec<u8>>)
        -> Result<http::Response<Self::Read>, Error>;
}

#[async_trait]
impl<T: ?Sized + HttpService + Send + Sync> HttpService for Box<T> {
    type Body = T::Body;
    type Read = T::Read;

    async fn request(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Body>, Error> {
        (**self).request(req).await
    }

    async fn watch(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Read>, Error> {
        (**self).watch(req).await
    }
}

#[async_trait]
impl<T: ?Sized + HttpService + Send + Sync> HttpService for Arc<T> {
    type Body = T::Body;
    type Read = T::Read;

    async fn request(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Body>, Error> {
        (**self).request(req).await
    }

    async fn watch(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Read>, Error> {
        (**self).watch(req).await
    }
}

#[async_trait]
impl<'a, T: ?Sized + HttpService + Send + Sync> HttpService for &'a T {
    type Body = T::Body;
    type Read = T::Read;

    async fn request(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Body>, Error> {
        (**self).request(req).await
    }

    async fn watch(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Read>, Error> {
        (**self).watch(req).await
    }
}

#[async_trait]
pub trait HttpUpgradeService {
    type Upgraded: AsyncRead + AsyncWrite;

    async fn upgrade(&self, req: http::Request<()>) -> Result<Self::Upgraded, Error>;
}

#[async_trait]
impl<T> HttpUpgradeService for T
where
    T: Deref + Sync,
    T::Target: HttpUpgradeService + Send + Sync,
{
    type Upgraded = <T::Target as HttpUpgradeService>::Upgraded;

    async fn upgrade(
        &self,
        req: http::Request<()>,
    ) -> Result<<Self as HttpUpgradeService>::Upgraded, Error> {
        self.deref().upgrade(req).await
    }
}
