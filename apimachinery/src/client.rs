use crate::meta::v1::{
    DeleteOptions, GetOptions, List, ListOptions, Metadata, Status, UpdateOptions, WatchEvent,
};
use crate::meta::{Resource, ResourceScope};
use crate::request::{Patch, Request, APPLICATION_JSON};
use crate::resplit;
use crate::response::{DecodeError, Response};
use crate::{ApiService, HttpService};
use failure::Error;
use failure::ResultExt;
use futures::{self, future, stream, Future, Stream};
use http::header::{HeaderMap, HeaderValue, ValueIter, CONTENT_LENGTH};
use http::{self, Method};
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
    C::Future: 'static,
    C::Stream: 'static,
    C::StreamFuture: 'static,
{
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn http_client(&self) -> &C {
        &self.http_client
    }

    pub fn resource<R>(&self, rsrc: R) -> ResourceClient<Self, R>
    where
        R: Resource,
        Self: Clone,
    {
        ResourceClient {
            api_client: self.clone(),
            resource: rsrc,
        }
    }
}

impl<'a, C: ?Sized + ApiService> ApiService for &'a C {
    fn request<B, O, B2>(
        &self,
        req: Request<B, O>,
    ) -> Box<Future<Item = Response<B2>, Error = Error> + Send>
    where
        B: Serialize + Send + 'static,
        O: Serialize + Send + 'static,
        B2: DeserializeOwned + Send + 'static,
    {
        (**self).request(req)
    }

    fn watch<B, O, B2>(&self, req: Request<B, O>) -> Box<Stream<Item = B2, Error = Error> + Send>
    where
        B: Serialize + Send + 'static,
        O: Serialize + Send + 'static,
        B2: DeserializeOwned + Send + 'static,
    {
        (**self).watch(req)
    }
}

impl<C> ApiService for ApiClient<C>
where
    C: HttpService + Send + Sync,
    C::Future: 'static,
    C::Stream: 'static,
    C::StreamFuture: 'static,
{
    fn request<B, O, B2>(
        &self,
        req: Request<B, O>,
    ) -> Box<Future<Item = Response<B2>, Error = Error> + Send>
    where
        B: Serialize + Send + 'static,
        O: Serialize + Send + 'static,
        B2: DeserializeOwned + Send + 'static,
    {
        let r = match req.into_http_request(self.base_url()) {
            Ok(r) => r,
            Err(e) => return Box::new(futures::future::err(e)),
        };

        let f = self
            .http_client
            .request(r)
            .and_then(|resp: http::Response<C::Body>| {
                // Deserialize body
                Response::from_http_response(resp).map_err(|e| e.into())
            });
        Box::new(f)
    }

    fn watch<B, O, B2>(&self, req: Request<B, O>) -> Box<Stream<Item = B2, Error = Error> + Send>
    where
        B: Serialize + Send + 'static,
        O: Serialize + Send + 'static,
        B2: DeserializeOwned + Send + 'static,
    {
        let r = match req.into_http_request(self.base_url()) {
            Ok(r) => r,
            Err(e) => return Box::new(stream::once(Err(e))),
        };

        let s = self
            .http_client
            .watch(r)
            .and_then(|resp: http::Response<C::Stream>| {
                let httpstatus = resp.status();
                let r = if httpstatus.is_success() {
                    Ok(resp)
                } else {
                    Err(resp)
                };
                future::result(r).or_else(|res| {
                    let con_len = content_length_parse_all(res.headers())
                        .and_then(|n| usize::try_from(n).ok());
                    res.into_body()
                        .map(move |b| {
                            // Pre-allocate buffer based on
                            // content-length, if provided
                            if let Some(n) = con_len {
                                let mut v = Vec::with_capacity(n);
                                v.extend(b.as_ref());
                                v
                            } else {
                                b.as_ref().to_vec()
                            }
                        })
                        .concat2()
                        .from_err::<Error>()
                        .and_then(|body| {
                            let status = Status::from_vec(body.to_vec())?;
                            Err(status.into())
                        })
                })
            })
            .map(|resp: http::Response<C::Stream>| {
                resplit::new(resp.into_body(), |&c| c == b'\n')
                    .from_err()
                    .inspect(|line| {
                        debug!("Watch response: {:#?}", line);
                    })
                    .and_then(|line| {
                        let parsed = serde_json::from_slice(&line)
                            .with_context(|e| DecodeError::new(e, line))?;
                        Ok(parsed)
                    })
            })
            .flatten_stream();

        Box::new(s)
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
    pub fn get(
        &self,
        name: &R::Scope,
        opts: GetOptions,
    ) -> impl Future<Item = R::Item, Error = Error> + Send {
        let gvr = self.resource.gvr();
        let req = Request {
            group: gvr.group.to_string(),
            version: gvr.version.to_string(),
            resource: gvr.resource.to_string(),
            namespace: name.namespace().map(|s| s.to_string()),
            name: name.name().map(|s| s.to_string()),
            subresource: None,
            method: Method::GET,
            opts: opts,
            content_type: None,
            body: (),
        };
        self.api_client()
            .request(req)
            .map(|resp: Response<_>| resp.into_body())
    }

    pub fn create(
        &self,
        value: R::Item,
        opts: GetOptions,
    ) -> impl Future<Item = R::Item, Error = Error> + Send {
        let ns = {
            let metadata = value.metadata();
            metadata.namespace.clone()
        };
        let gvr = self.resource.gvr();

        let req = Request {
            group: gvr.group.to_string(),
            version: gvr.version.to_string(),
            resource: gvr.resource.to_string(),
            namespace: ns,
            name: None,
            subresource: None,
            method: Method::POST,
            opts: opts,
            content_type: Some(APPLICATION_JSON),
            body: value,
        };
        self.api_client()
            .request(req)
            .map(|resp: Response<_>| resp.into_body())
    }

    pub fn update(
        &self,
        value: R::Item,
        opts: UpdateOptions,
    ) -> impl Future<Item = R::Item, Error = Error> + Send {
        let gvr = self.resource.gvr();
        let (ns, name) = {
            let md = value.metadata();
            (md.namespace.clone(), md.name.clone())
        };

        let req = Request {
            group: gvr.group.to_string(),
            version: gvr.version.to_string(),
            resource: gvr.resource.to_string(),
            namespace: ns,
            name: name,
            subresource: None,
            method: Method::PUT,
            opts: opts,
            content_type: Some(APPLICATION_JSON),
            body: value,
        };
        self.api_client()
            .request(req)
            .map(|resp: Response<_>| resp.into_body())
    }

    pub fn patch(
        &self,
        name: R::Scope,
        patch: Patch,
        opts: UpdateOptions,
    ) -> impl Future<Item = R::Item, Error = Error> + Send {
        let gvr = self.resource.gvr();

        let req = Request {
            group: gvr.group.to_string(),
            version: gvr.version.to_string(),
            resource: gvr.resource.to_string(),
            namespace: name.namespace().map(|s| s.to_string()),
            name: name.name().map(|s| s.to_string()),
            subresource: None,
            method: Method::PATCH,
            opts: opts,
            content_type: Some(patch.content_type()),
            body: patch,
        };
        self.api_client()
            .request(req)
            .map(|resp: Response<_>| resp.into_body())
    }

    pub fn delete(
        &self,
        name: R::Scope,
        opts: DeleteOptions,
    ) -> impl Future<Item = (), Error = Error> + Send {
        let gvr = self.resource.gvr();

        let req = Request {
            group: gvr.group.to_string(),
            version: gvr.version.to_string(),
            resource: gvr.resource.to_string(),
            namespace: name.namespace().map(|s| s.to_string()),
            name: name.name().map(|s| s.to_string()),
            subresource: None,
            method: Method::DELETE,
            opts: opts,
            content_type: None,
            body: (),
        };
        self.api_client()
            .request(req)
            .map(|resp: Response<_>| resp.into_body())
    }

    pub fn list(
        &self,
        name: &R::Scope,
        opts: ListOptions,
    ) -> impl Future<Item = R::List, Error = Error> + Send {
        // R::Scope is ~wrong - should be only namespace or empty.
        // FIXME: Convert list(name) into list(collection) with
        // metadata.name filter to handle this single-item list (or
        // watch) case.

        let gvr = self.resource.gvr();
        let req = Request {
            group: gvr.group.to_string(),
            version: gvr.version.to_string(),
            resource: gvr.resource.to_string(),
            namespace: name.namespace().map(|s| s.to_string()),
            name: name.name().map(|s| s.to_string()), // NB: see note above
            subresource: None,
            method: Method::GET,
            opts: opts,
            content_type: None,
            body: (),
        };
        self.api_client()
            .request(req)
            .map(move |resp: Response<_>| resp.into_body())
    }

    pub fn iter(
        &self,
        name: &R::Scope, // FIXME: see note on list()
        opts: ListOptions,
    ) -> impl Stream<Item = R::Item, Error = Error> + Send {
        let gvr = {
            // TODO: add an owned version of GroupVersionResource
            let gvr = self.resource.gvr();
            (
                gvr.group.to_string(),
                gvr.version.to_string(),
                gvr.resource.to_string(),
            )
        };
        let ns = name.namespace().map(|s| s.to_string());
        let client = self.api_client().clone();

        let fetch_pages = stream::unfold(Some((client, gvr, ns, opts)), |maybe_args| {
            maybe_args.map(|(client, gvr, ns, mut opts)| {
                let req = Request {
                    group: gvr.0.clone(),
                    version: gvr.1.clone(),
                    resource: gvr.2.clone(),
                    namespace: ns.clone(),
                    name: None,
                    subresource: None,
                    method: Method::GET,
                    opts: opts.clone(),
                    content_type: None,
                    body: (),
                };
                client.request(req).map(move |resp: Response<R::List>| {
                    let page = resp.into_body();
                    let maybe_next = page
                        .listmeta()
                        .continu
                        .as_ref()
                        .filter(|c| !c.is_empty())
                        .map(|c| {
                            opts.continu = c.to_string();
                            (client, gvr, ns, opts)
                        });
                    (page, maybe_next)
                })
            })
        });

        fetch_pages
            .map(|page| stream::iter_ok(page.into_items().into_iter()))
            .flatten()
    }

    pub fn watch(
        &self,
        name: &R::Scope, // FIXME: wrong!
        mut opts: ListOptions,
    ) -> impl Stream<Item = WatchEvent<R::Item>, Error = Error> + Send {
        let gvr = self.resource.gvr();
        opts.watch = true;
        let req = Request {
            group: gvr.group.to_string(),
            version: gvr.version.to_string(),
            resource: gvr.resource.to_string(),
            namespace: name.namespace().map(|s| s.to_string()),
            name: None,
            subresource: None,
            method: Method::GET,
            opts: opts,
            content_type: None,
            body: (),
        };
        self.api_client().watch(req)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::meta::v1::{ItemList, Metadata, ObjectMeta};
    use crate::meta::{GroupVersionResource, NamespaceScope, TypeMeta, TypeMetaImpl};
    use futures::IntoFuture;
    use serde::Serialize;
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
        pub fn resource<R>(&self, rsrc: R) -> ResourceClient<&Self, R>
        where
            R: Resource,
        {
            ResourceClient {
                api_client: self,
                resource: rsrc,
            }
        }
    }

    impl ApiService for TestClient {
        fn request<B, O, B2>(
            &self,
            req: Request<B, O>,
        ) -> Box<Future<Item = Response<B2>, Error = Error> + Send>
        where
            B: Serialize + Send + 'static,
            O: Serialize + Send + 'static,
            B2: DeserializeOwned + Send + 'static,
        {
            let result = match req.method {
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
                        try_convert(obj)
                    }
                    _ => Err(failure::err_msg("Unknown resource")),
                },
                _ => Err(failure::err_msg("Unimplemented method")),
            };
            Box::new(result.map(Response::ok).into_future())
        }

        fn watch<B, O, B2>(
            &self,
            req: Request<B, O>,
        ) -> Box<Stream<Item = B2, Error = Error> + Send>
        where
            B: Serialize + Send + 'static,
            O: Serialize + Send + 'static,
            B2: DeserializeOwned + Send + 'static,
        {
            let results = match req.method {
                Method::GET => match (
                    req.group.as_str(),
                    req.version.as_str(),
                    req.resource.as_str(),
                    req.namespace.as_ref().map(|s| s.as_str()),
                    req.name.as_ref().map(|s| s.as_str()),
                ) {
                    ("test", "v1", "foos", Some("default"), None) => vec![
                        try_convert(WatchEvent::Added(Foo {
                            metadata: ObjectMeta {
                                namespace: Some("default".to_string()),
                                name: Some("myfoo".to_string()),
                                ..Default::default()
                            },
                            foo: "bar".to_string(),
                            ..Default::default()
                        })),
                        try_convert(WatchEvent::Modified(Foo {
                            metadata: ObjectMeta {
                                namespace: Some("default".to_string()),
                                name: Some("myfoo".to_string()),
                                ..Default::default()
                            },
                            foo: "baz".to_string(),
                            ..Default::default()
                        })),
                    ],
                    _ => vec![Err(failure::err_msg("Unknown resource"))],
                },
                _ => vec![Err(failure::err_msg("Bad method"))],
            };
            Box::new(stream::iter_result(results))
        }
    }

    #[test]
    fn test_client_get() {
        let c = TestClient {};
        let name = NamespaceScope::Name {
            namespace: "default".to_string(),
            name: "myfoo".to_string(),
        };
        let f = c.resource(Foos).get(&name, Default::default());
        let result = f.wait();
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

        let f = c.resource(&rsrc).get(
            &DynamicScope::Namespace(NamespaceScope::Name {
                namespace: "default".to_string(),
                name: "myfoo".to_string(),
            }),
            Default::default(),
        );
        let result = f.wait();
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
