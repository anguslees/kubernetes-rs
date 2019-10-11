use crate::meta::v1::{
    DeleteOptions, GetOptions, List, ListOptions, Metadata, Status, UpdateOptions, WatchEvent,
};
use crate::meta::{Resource, ResourceScope};
use crate::request::{Patch, Request, APPLICATION_JSON};
use crate::response::{DecodeError, Response};
use crate::{ApiService, HttpService};
use async_stream::try_stream;
use async_trait::async_trait;
use failure::{Error, ResultExt};
use futures::future::TryFutureExt;
use futures::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use futures::pin_mut;
use futures::stream::{BoxStream, Stream, TryStream, TryStreamExt};
use http::header::{HeaderMap, HeaderValue, ValueIter, CONTENT_LENGTH};
use http::{self, Method};
use log::debug;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::convert::TryFrom;

#[derive(Debug, Clone)]
pub struct ApiClient<C> {
    http_client: C,
    base_url: String,
}

impl<C> ApiClient<C> {
    pub fn new(client: C, base_url: String) -> Self {
        ApiClient {
            http_client: client,
            base_url: base_url,
        }
    }
}

impl<C> ApiClient<C>
where
    C: HttpService + Send + Sync,
{
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn http_client(&self) -> &C {
        &self.http_client
    }

    pub fn resource<'a, R>(&'a self, rsrc: R) -> ResourceClient<&'a Self, R>
    where
        R: Resource,
    {
        ResourceClient {
            api_client: self,
            resource: rsrc,
        }
    }
}

#[async_trait]
impl<C: ?Sized + ApiService + Send + Sync> ApiService for &C {
    async fn request<B, O, B2>(&self, req: Request<B, O>) -> Result<Response<B2>, Error>
    where
        B: Serialize + Send + 'async_trait,
        O: Serialize + Send + 'async_trait,
        B2: DeserializeOwned + Send + 'static,
    {
        (**self).request(req).await
    }

    fn watch<'a, B, O, B2>(&'a self, req: Request<B, O>) -> BoxStream<'a, Result<B2, Error>>
    where
        B: Serialize + Send + 'a,
        O: Serialize + Send + 'a,
        B2: DeserializeOwned + Send + Unpin + 'static,
    {
        (**self).watch(req)
    }
}

#[async_trait]
impl<C> ApiService for ApiClient<C>
where
    C: HttpService + Send + Sync,
{
    async fn request<B, O, B2>(&self, req: Request<B, O>) -> Result<Response<B2>, Error>
    where
        B: Serialize + Send + 'async_trait,
        O: Serialize + Send + 'async_trait,
        B2: DeserializeOwned + Send + 'static,
    {
        let r = req.into_http_request(self.base_url())?;
        let http_resp = self.http_client.request(r).await?;
        let resp = Response::from_http_response(http_resp)?;
        Ok(resp)
    }

    fn watch<'a, B, O, B2>(&'a self, req: Request<B, O>) -> BoxStream<'a, Result<B2, Error>>
    where
        B: Serialize + Send + 'a,
        O: Serialize + Send + 'a,
        B2: DeserializeOwned + Send + Unpin + 'static,
    {
        let s = try_stream! {
            let base_url = self.base_url();
            let r = req.into_http_request(base_url)?;
            let resp = self.http_client.watch(r).await?;

            if ! resp.status().is_success() {
                // HTTP error (ie: not 2xx)

                // Pre-allocate buffer based on content-length, if
                // provided.
                let con_len =
                    content_length_parse_all(resp.headers()).and_then(|n| usize::try_from(n).ok());
                let mut buf = Vec::with_capacity(con_len.unwrap_or(0));
                let body = resp.into_body();
                pin_mut!(body);
                body.read_to_end(&mut buf).map_err(Error::from).await?;

                let status = Status::from_vec(buf)?;
                Err(Error::from(status))?;
                unreachable!();
            }

            // HTTP 2xx response
            let stream = BufReader::new(resp.into_body())
                .lines();
            pin_mut!(stream);
            while let Some(line) = stream.try_next().map_err(Error::from).await? {
                debug!("Watch response: {:#?}", line);
                let parsed: B2 = serde_json::from_str(&line)
                    .with_context(|e| DecodeError::new(e, line.into())).map_err(Error::from)?;
                yield parsed;
            }
        };

        Box::pin(s)
    }
}

pub struct ResourceClient<C, R>
where
    C: ApiService + Send + Sync + Clone,
    R: Resource,
{
    api_client: C,
    resource: R,
}

impl<C, R> ResourceClient<C, R>
where
    C: ApiService + Send + Sync + Clone,
    R: Resource,
{
    pub fn api_client(&self) -> &C {
        &self.api_client
    }
}

impl<C, R> ResourceClient<C, R>
where
    C: ApiService + Send + Sync + Clone,
    R: Resource,
{
    pub async fn get(&self, name: &R::Scope, opts: GetOptions) -> Result<R::Item, Error> {
        let req = Request::builder(self.resource.gvr())
            .scope(name)
            .method(Method::GET)
            .opts(opts)
            .build();

        self.api_client()
            .request(req)
            .await
            .map(Response::into_body)
    }

    pub async fn create(&self, value: R::Item, opts: GetOptions) -> Result<R::Item, Error> {
        let ns = {
            let metadata = value.metadata();
            metadata.namespace.clone()
        };

        let req = Request::builder(self.resource.gvr())
            .namespace_maybe(ns)
            .method(Method::POST)
            .opts(opts)
            .body(APPLICATION_JSON, value)
            .build();

        self.api_client()
            .request(req)
            .await
            .map(Response::into_body)
    }

    pub async fn update(&self, value: R::Item, opts: UpdateOptions) -> Result<R::Item, Error> {
        let (ns, name) = {
            let md = value.metadata();
            (md.namespace.clone(), md.name.clone())
        };

        let req = Request::builder(self.resource.gvr())
            .name_maybe(name)
            .namespace_maybe(ns)
            .method(Method::PUT)
            .opts(opts)
            .body(APPLICATION_JSON, value)
            .build();

        self.api_client()
            .request(req)
            .await
            .map(Response::into_body)
    }

    pub async fn patch(
        &self,
        name: R::Scope,
        patch: Patch,
        opts: UpdateOptions,
    ) -> Result<R::Item, Error> {
        let req = Request::builder(self.resource.gvr())
            .scope(name)
            .method(Method::PATCH)
            .opts(opts)
            .body(patch.content_type(), patch)
            .build();

        self.api_client()
            .request(req)
            .await
            .map(Response::into_body)
    }

    pub async fn delete(&self, name: R::Scope, opts: DeleteOptions) -> Result<R::Item, Error> {
        let req = Request::builder(self.resource.gvr())
            .scope(name)
            .method(Method::DELETE)
            .opts(opts)
            .build();

        self.api_client()
            .request(req)
            .await
            .map(Response::into_body)
    }

    pub async fn list(&self, name: &R::Scope, opts: ListOptions) -> Result<R::List, Error> {
        // R::Scope is ~wrong - should be only namespace or empty.
        // FIXME: Convert list(name) into list(collection) with
        // metadata.name filter to handle this single-item list (or
        // watch) case.

        let req = Request::builder(self.resource.gvr())
            .scope(name) // NB: see note above
            .method(Method::GET)
            .opts(opts)
            .build();

        self.api_client()
            .request(req)
            .await
            .map(Response::into_body)
    }

    pub fn iter<'a>(
        &'a self,
        name: &'a R::Scope, // FIXME: see note on list()
        mut opts: ListOptions,
    ) -> impl TryStream<Ok = R::Item, Error = Error> + 'a
    where
        R::List: Unpin,
        R::Item: Unpin,
    {
        let pages = try_stream! {
            let ns = name.namespace();
            let client = self.api_client();

            loop {
                let req = Request::builder(self.resource.gvr())
                    .namespace_maybe(ns)
                    .method(Method::GET)
                    .opts(opts.clone())
                    .build();

                let resp: Response<R::List> = client.request(req).await?;

                let page = resp.into_body();
                opts.continu = page.listmeta().continu.clone().unwrap_or_default();

                yield page;

                if opts.continu.is_empty() {
                    break;
                }
            }
        };

        let s = try_stream! {
            pin_mut!(pages);
            while let Some(page) = pages.try_next().await? {
                for item in page.into_items() {
                    yield item;
                }
            }
        };

        Box::pin(s)
    }

    pub fn watch<'a>(
        &'a self,
        name: &'a R::Scope, // FIXME: wrong!
        mut opts: ListOptions,
    ) -> impl Stream<Item = Result<WatchEvent<R::Item>, Error>> + Send + 'a
    where
        R::Item: Unpin + 'static,
    {
        opts.watch = true;
        let req = Request::builder(self.resource.gvr())
            .scope(name)
            .method(Method::GET)
            .opts(opts)
            .build();

        self.api_client().watch(req)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::meta::v1::{ItemList, Metadata, ObjectMeta};
    use crate::meta::{GroupVersionResource, NamespaceScope, TypeMeta, TypeMetaImpl};
    use serde::{Deserialize, Serialize};
    use std::borrow::Cow;

    #[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Foo {
        #[serde(flatten)]
        typemeta: TypeMetaImpl<Foo>,
        #[serde(default)]
        pub metadata: ObjectMeta,
        pub foo: String,
    }

    impl TypeMeta for Foo {
        fn api_version() -> &'static str {
            "test/v1"
        }

        fn kind() -> &'static str {
            "Foo"
        }
    }

    impl Metadata for Foo {
        fn api_version(&self) -> &str {
            <Self as TypeMeta>::api_version()
        }
        fn kind(&self) -> &str {
            <Self as TypeMeta>::kind()
        }
        fn metadata(&self) -> Cow<ObjectMeta> {
            Cow::Borrowed(&self.metadata)
        }
    }

    struct Foos;
    impl Resource for Foos {
        type Item = Foo;
        type Scope = NamespaceScope;
        type List = ItemList<Foo>;
        fn gvr(&self) -> GroupVersionResource {
            GroupVersionResource {
                group: "test",
                version: "v1",
                resource: "foos",
            }
        }
        fn singular(&self) -> String {
            "foo".to_string()
        }
    }

    fn try_convert<T, U>(input: T) -> Result<U, Error>
    where
        T: Serialize,
        U: DeserializeOwned,
    {
        let value = serde_json::to_value(input)?;
        let result = serde_json::from_value(value)?;
        Ok(result)
    }

    #[derive(Debug, Clone)]
    struct TestClient;

    impl TestClient {
        pub fn resource<'a, R>(&'a self, rsrc: R) -> ResourceClient<&'a Self, R>
        where
            R: Resource,
        {
            ResourceClient {
                api_client: self,
                resource: rsrc,
            }
        }
    }

    #[async_trait]
    impl ApiService for TestClient {
        async fn request<B, O, B2>(&self, req: Request<B, O>) -> Result<Response<B2>, Error>
        where
            B: Serialize + Send + 'async_trait,
            O: Serialize + Send + 'async_trait,
            B2: DeserializeOwned + Send + 'static,
        {
            let resp_body = match req.method {
                Method::GET => match (
                    req.group.as_str(),
                    req.version.as_str(),
                    req.resource.as_str(),
                    req.namespace.as_ref().map(|s| s.as_str()),
                    req.name.as_ref().map(|s| s.as_str()),
                ) {
                    ("test", "v1", "foos", Some("default"), Some("myfoo")) => {
                        let obj = Foo {
                            metadata: ObjectMeta {
                                namespace: Some("default".to_string()),
                                name: Some("myfoo".to_string()),
                                ..Default::default()
                            },
                            foo: "bar".to_string(),
                            ..Default::default()
                        };
                        try_convert(obj)?
                    }
                    _ => Err(failure::err_msg("Unknown resource"))?,
                },
                _ => Err(failure::err_msg("Unimplemented method"))?,
            };
            Ok(Response::ok(resp_body))
        }

        fn watch<'a, B, O, B2>(&'a self, req: Request<B, O>) -> BoxStream<'a, Result<B2, Error>>
        where
            B: Serialize + Send + 'a,
            O: Serialize + Send + 'a,
            B2: DeserializeOwned + Send + Unpin + 'static,
        {
            let s = try_stream! {
                match req.method {
                    Method::GET => match (
                        req.group.as_str(),
                        req.version.as_str(),
                        req.resource.as_str(),
                        req.namespace.as_ref().map(|s| s.as_str()),
                        req.name.as_ref().map(|s| s.as_str()),
                    ) {
                        ("test", "v1", "foos", Some("default"), None) => {
                            let value = try_convert(WatchEvent::Added(Foo {
                                metadata: ObjectMeta {
                                    namespace: Some("default".to_string()),
                                    name: Some("myfoo".to_string()),
                                    ..Default::default()
                                },
                                foo: "bar".to_string(),
                                ..Default::default()
                            }))?;
                            yield value;
                            let value = try_convert(WatchEvent::Modified(Foo {
                                metadata: ObjectMeta {
                                    namespace: Some("default".to_string()),
                                    name: Some("myfoo".to_string()),
                                    ..Default::default()
                                },
                                foo: "baz".to_string(),
                                ..Default::default()
                            }))?;
                            yield value;
                        },
                        _ => Err(failure::err_msg("Unknown resource"))?,
                    },
                    _ => Err(failure::err_msg("Bad method"))?,
                };
            };
            Box::pin(s)
        }
    }

    #[test]
    fn test_client_get() {
        let c = TestClient {};
        let name = NamespaceScope::Name {
            namespace: "default".to_string(),
            name: "myfoo".to_string(),
        };
        let f = async { c.resource(Foos).get(&name, Default::default()).await };
        let result = futures::executor::block_on(f);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dynamic_get() {
        use crate::unstructured::{DynamicResource, DynamicScope};

        let c = TestClient {};

        // These might be calcuated at runtime
        let rsrc = DynamicResource {
            group: "test".to_string(),
            version: "v1".to_string(),
            singular: "foo".to_string(),
            plural: "foos".to_string(),
        };

        let f = async {
            c.resource(&rsrc)
                .get(
                    &DynamicScope::Namespace(NamespaceScope::Name {
                        namespace: "default".to_string(),
                        name: "myfoo".to_string(),
                    }),
                    Default::default(),
                )
                .await
        };
        let result = futures::executor::block_on(f);
        assert!(result.is_ok());
    }
}

// Borrowed from hyper
fn content_length_parse_all(headers: &HeaderMap) -> Option<u64> {
    content_length_parse_all_values(headers.get_all(CONTENT_LENGTH).into_iter())
}

fn content_length_parse_all_values(values: ValueIter<HeaderValue>) -> Option<u64> {
    // If multiple Content-Length headers were sent, everything can still
    // be alright if they all contain the same value, and all parse
    // correctly. If not, then it's an error.

    let folded = values.fold(None, |prev, line| match prev {
        Some(Ok(prev)) => Some(
            line.to_str()
                .map_err(|_| ())
                .and_then(|s| s.parse().map_err(|_| ()))
                .and_then(|n| if prev == n { Ok(n) } else { Err(()) }),
        ),
        None => Some(
            line.to_str()
                .map_err(|_| ())
                .and_then(|s| s.parse().map_err(|_| ())),
        ),
        Some(Err(())) => Some(Err(())),
    });

    if let Some(Ok(n)) = folded {
        Some(n)
    } else {
        None
    }
}
