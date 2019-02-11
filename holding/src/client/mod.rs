use api::core::v1::{NamespacedResource, Resource};
use api::meta::v1::{DeleteOptions, GetOptions, List, ListOptions, Metadata, Status, WatchEvent};
use api::meta::GroupVersionResource;
use api::TypeMeta;
use failure::{Error, ResultExt};
use futures::{future, stream, Future, Stream};
use hyper::header::{HeaderValue, CONTENT_TYPE};
use hyper::{self, Body, Method, Request};
use hyper_tls::HttpsConnector;
use native_tls::{Certificate, Identity, TlsConnector};
use openssl;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json;
use serde_urlencoded;
use std::default::Default;
use std::env;
use std::fmt;
use std::path::PathBuf;
use std::str;
use std::sync::Arc;
use url::Url;

pub mod config;
mod resplit;

use self::config::ConfigContext;

#[derive(Fail, Debug)]
#[fail(display = "HTTP client error: {}", err)]
pub struct ClientError {
    err: hyper::Error,
}

#[derive(Fail, Debug)]
#[fail(display = "Unexpected HTTP response status: {}", status)]
pub struct HttpStatusError {
    status: hyper::StatusCode,
}

#[derive(Fail, Debug)]
#[fail(display = "Attribute {} required but not provided", attr)]
pub struct RequiredAttributeError {
    attr: &'static str,
}
pub fn required_attr(attr: &'static str) -> RequiredAttributeError {
    RequiredAttributeError { attr: attr }
}

#[derive(Debug, Clone)]
pub struct Client<C> {
    client: Arc<hyper::Client<C>>,
    config: ConfigContext,
}

#[derive(Debug, Clone)]
pub struct NamespacedClient<'a, C> {
    namespace: &'a str,
    client: &'a Client<C>,
}

impl<'a, C> Client<C> {
    pub fn namespace(&'a self, ns: &'a str) -> NamespacedClient<'a, C> {
        NamespacedClient {
            namespace: ns,
            client: self,
        }
    }
}

impl Client<HttpsConnector<hyper::client::HttpConnector>> {
    pub fn new() -> Result<Self, Error> {
        let dns_threads = 1; // Only need a single DNS lookup
        let http = hyper::client::HttpConnector::new(dns_threads);
        Client::new_from_http(http)
    }

    pub fn new_from_http(http: hyper::client::HttpConnector) -> Result<Self, Error> {
        let config_path = env::var_os(config::CONFIG_ENV)
            .map(PathBuf::from)
            .or_else(config::default_path)
            .ok_or(format_err!("Unable to find config"))?;
        debug!("Reading config from {}", config_path.display());
        let config = config::load_from_file(&config_path)
            .with_context(|e| format!("Unable to read {}: {}", config_path.display(), e))?;
        let context = config.config_context(&config.current_context)?;
        Client::new_from_context(http, context)
    }

    pub fn new_from_context(
        mut http: hyper::client::HttpConnector,
        config: ConfigContext,
    ) -> Result<Self, Error> {
        http.enforce_http(false);
        let mut tls = TlsConnector::builder();
        if let (Some(certdata), Some(keydata)) = (
            config.user.client_certificate_read(),
            config.user.client_key_read(),
        ) {
            debug!("Setting user client cert");
            let cert = openssl::x509::X509::from_pem(&certdata?)?;
            let pkey = openssl::pkey::PKey::private_key_from_pem(&keydata?)?;
            // openssl pkcs12 -export -clcerts -inkey kubecfg.key -in kubecfg.crt -out kubecfg.p12 -name "kubecfg"
            let password = "";
            let p12 =
                openssl::pkcs12::Pkcs12::builder().build(password, "kubeconfig", &pkey, &cert)?;
            tls.identity(Identity::from_pkcs12(&p12.to_der()?, password)?);
        }

        if let Some(data) = config.cluster.certificate_authority_read() {
            debug!("Setting cluster CA cert");
            let cert = Certificate::from_pem(&data?)?;
            // FIXME: want to validate against _only_ this cert ..
            tls.add_root_certificate(cert);
        }

        if config.cluster.insecure_skip_tls_verify {
            debug!("Disabling CA verification");
            // TODO: do this only for the endpoint in question, not globally.
            tls.danger_accept_invalid_certs(true);
        }

        let hyper_client =
            hyper::Client::builder().build(HttpsConnector::from((http, tls.build()?)));

        Self::new_with_client(hyper_client, config)
    }
}

impl<C> Client<C> {
    pub fn new_with_client(client: hyper::Client<C>, config: ConfigContext) -> Result<Self, Error> {
        Ok(Client {
            client: Arc::new(client),
            config: config,
        })
    }

    pub fn client(&self) -> &hyper::Client<C> {
        &self.client
    }
}

fn hyper_uri(u: Url) -> hyper::Uri {
    u.to_string()
        .parse()
        .expect("attempted to convert invalid uri")
}

fn do_request<C, T>(
    client: Arc<hyper::Client<C>>,
    req: Result<Request<hyper::Body>, Error>,
) -> impl Future<Item = T, Error = Error> + Send
where
    C: hyper::client::connect::Connect + 'static,
    T: DeserializeOwned + Send + 'static,
{
    future::result(req)
        .inspect(|req|
                 // Avoid body, since it may not be Debug
                 debug!("Request: {} {}", req.method(), req.uri()))
        .and_then(move |req|
                  // TODO: add method/uri context to error
                  client.request(req).from_err::<Error>())
        .inspect(|res| debug!("Response: {} {:?}", res.status(), res.headers()))
        // Verbose!
        //.inspect(|res| debug!("Response: {:#?}", res))
        .and_then(|res| {
            let status = res.status();
            res.into_body()
                .concat2()
                .map(move |body| (status, body))
                .from_err()
        })
        // Verbose!
        //.inspect(|(_, body)| debug!("Response body: {:?}", ::std::str::from_utf8(body.as_ref())))
        .and_then(move |(httpstatus, body)| -> Result<T, Error> {
            if !httpstatus.is_success() {
                // I think we can drop this debug! it is redundant with enrich_error.
                debug!("failure body: {:#?}", ::std::str::from_utf8(body.as_ref()));
                let status: Status = serde_json::from_slice(body.as_ref()).map_err(|e| {
                    debug!(
                        "Failed to parse error Status ({}), falling back to HTTP status",
                        enrich_error("error Status", &e, body.as_ref())
                    );
                    HttpStatusError { status: httpstatus }
                })?;
                Err(status.into())
            } else {
                let o = serde_json::from_slice(body.as_ref())
                    .with_context(|e| enrich_error("response body", e, body.as_ref()))?;
                Ok(o)
            }
        })
}

/// Grabs the 1K preceeding text from the failed document and describes the error.
///
/// TODO: handle multi-line JSON, just in case some API server decides to start emitting that.
fn enrich_error(desc: &str, e: &serde_json::Error, body_ref: &[u8]) -> String {
    // debug! so that operators running with debug logs get *everything*
    debug!("Parse failure: {:#?}", body_ref);
    // Provide a short snippet for errors that may be handled, logged at higher verbosity etc.
    match e.classify() {
        serde_json::error::Category::Io | serde_json::error::Category::Eof => {
            format!("Unable to parse {}: {}", desc, e)
        }
        _ => {
            // Either bad structure/values in the JSON (so show it) or bad contents (so show it)
            // TODO: discard leading content? e.g. smaller but still debuggable?
            let mut lines = str::from_utf8(body_ref).unwrap().lines();
            let mut line_n = 1;
            let mut line = lines.next().unwrap();
            while line_n < e.line() {
                line_n += 1;
                line = lines.next().unwrap();
            }
            let start_n = if e.column() < 1024 {
                0
            } else {
                e.column() - 1024
            };
            let body_snippet = &line[start_n..e.column()];
            format!("Unable to parse {}: {} {}", desc, body_snippet, e)
        }
    }
}

#[cfg(test)]
mod tests {
    #[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct SampleObject {
        pub required_field: String,
    }

    #[test]
    fn test_enrich_error_short() {
        let doc = "{\"doc\": 1}";
        let formatted = serde_json::from_slice::<SampleObject>(doc.as_bytes())
            .err()
            .map(|e| super::enrich_error("error Status", &e, doc.as_bytes()));
        assert_eq!(Some(String::from("Unable to parse error Status: {\"doc\": 1} missing field `requiredField` at line 1 column 10")), formatted);
    }
}

fn do_watch<C, T>(
    client: &Arc<hyper::Client<C>>,
    req: Result<hyper::Request<hyper::Body>, Error>,
) -> impl Stream<Item = T, Error = Error> + Send
where
    C: hyper::client::connect::Connect + 'static,
    T: DeserializeOwned + Send + 'static,
{
    let client = Arc::clone(client);
    future::result(req)
        .inspect(|req| debug!("Watch request: {} {}", req.method(), req.uri()))
        .and_then(move |req|
                  // TODO: add method/uri context to error
                  client.request(req).from_err::<Error>())
        .inspect(|res| debug!("Response: {:#?}", res))
        .and_then(|res| {
            let httpstatus = res.status();
            let r = if httpstatus.is_success() {
                Ok(res)
            } else {
                Err(res)
            };
            future::result(r)
                .or_else(move |res| {
                    res.into_body()
                        .concat2()
                        .from_err::<Error>()
                        .and_then(move |body| {
                            // Redundant debug with enrich_error
                            debug!("failure body: {:#?}", ::std::str::from_utf8(body.as_ref()));
                            let status: Status = serde_json::from_slice(body.as_ref()).map_err(
                                |e| {
                                    debug!("Failed to parse error Status ({}), falling back to HTTP status", 
                                        enrich_error("error Status", &e, body.as_ref()));
                                    HttpStatusError { status: httpstatus }
                                },
                            )?;

                            Err(status.into())
                        })
                })
                .map(|res| {
                    resplit::new(res.into_body(), |&c| c == b'\n')
                        .from_err()
                        .inspect(|line| {
                            debug!(
                                "Got line: {:#?}",
                                ::std::str::from_utf8(line).unwrap_or("<invalid utf8>")
                            )
                        })
                        .and_then(move |line| {
                            let o: T = serde_json::from_slice(line.as_ref())
                                .with_context(|e| enrich_error("watch line", e, line.as_ref()))?;
                            Ok(o)
                        })
                })
        })
        .flatten_stream()
}

impl<'a, C: hyper::client::connect::Connect + 'static> NamespacedClient<'a, C> {
    pub fn iter<T>(
        &self,
        rsrc: T,
    ) -> impl Stream<Item = <T::List as List>::Item, Error = Error> + Send
    where
        T: NamespacedResource,
        T::List: List + DeserializeOwned + Send + 'static,
        <T::List as List>::Item: TypeMeta + DeserializeOwned + Default + Send + 'static,
    {
        self.iter_opt(rsrc, Default::default())
    }

    pub fn iter_opt<T>(
        &self,
        rsrc: T,
        opts: ListOptions,
    ) -> impl Stream<Item = <T::List as List>::Item, Error = Error> + Send
    where
        T: NamespacedResource,
        T::List: List + DeserializeOwned + Send + 'static,
        <T::List as List>::Item: TypeMeta + DeserializeOwned + Default + Send + 'static,
    {
        let ns = if rsrc.namespaced() {
            Some(self.namespace)
        } else {
            None
        };
        self.client._do_iter::<T::List>(rsrc.gvr(), ns, opts)
    }
}

impl<C: hyper::client::connect::Connect + 'static> Client<C> {
    fn url<O>(
        &self,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        name: Option<&str>,
        opts: O,
    ) -> Result<Url, Error>
    where
        O: Serialize + fmt::Debug,
    {
        let mut url: Url = self.config.cluster.server.parse()?;

        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| format_err!("URL scheme does not support paths"))?;
            /* XXX: This looks like a k8s API rooted at (say) /kube on a
             *      reverse proxy will break.
             */
            path.clear();
            /* This knowledge should perhaps be pushed into the group itself */
            path.push(if gvr.group == "" && gvr.version == "v1" {
                "api"
            } else {
                "apis"
            });
            if gvr.group != "" {
                path.push(&gvr.group);
            }
            path.push(&gvr.version);
            namespace.map(|ns| path.extend(&["namespaces", ns]));
            path.push(&gvr.resource);
            name.map(|n| path.push(n));
        }

        serde_urlencoded::to_string(&opts)
            .map(|query| {
                let q = if query != "" {
                    Some(query.as_str())
                } else {
                    None
                };
                url.set_query(q)
            })
            .with_context(|e| format!("Unable to encode URL parameters {}", e))?;
        Ok(url)
    }

    pub fn get<T>(
        &self,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        name: &str,
        opts: GetOptions,
    ) -> impl Future<Item = T, Error = Error> + Send
    where
        T: DeserializeOwned + Send + 'static,
    {
        let req = self.url(gvr, namespace, Some(name), opts).and_then(|url| {
            Request::builder()
                .method(Method::GET)
                .uri(hyper_uri(url))
                .body(Body::empty())
                .map_err(|e| e.into())
        });
        do_request(Arc::clone(&self.client), req)
    }

    pub fn create<T>(
        &self,
        gvr: &GroupVersionResource,
        value: &T,
        opts: GetOptions,
    ) -> impl Future<Item = T, Error = Error> + Send
    where
        T: Metadata + Serialize + DeserializeOwned + Send + 'static,
    {
        let req = || -> Result<_, Error> {
            let metadata = value.metadata();
            let namespace = &metadata.namespace; // NB: assumes input object is correctly qualified
            let name = metadata.name.as_ref().ok_or(required_attr("name"))?;

            let json = serde_json::to_vec(value)?;

            Request::builder()
                .method(Method::POST)
                .uri(hyper_uri(self.url(
                    gvr,
                    namespace.as_ref().map(|v| v.as_str()),
                    Some(&name),
                    opts,
                )?))
                .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                .body(Body::from(json))
                .map_err(|e| e.into())
        }();
        do_request(Arc::clone(&self.client), req)
    }

    pub fn update<T>(
        &self,
        gvr: &GroupVersionResource,
        value: &T,
    ) -> impl Future<Item = T, Error = Error> + Send
    where
        T: Metadata + Serialize + DeserializeOwned + Send + 'static,
    {
        let req = || -> Result<_, Error> {
            let metadata = value.metadata();
            let namespace = &metadata.namespace; // NB: assumes input object is correctly qualified
            let name = metadata.name.as_ref().ok_or(required_attr("name"))?;

            let json = serde_json::to_vec(value)?;

            Request::builder()
                .method(Method::PUT)
                .uri(hyper_uri(self.url(
                    gvr,
                    namespace.as_ref().map(|v| v.as_str()),
                    Some(&name),
                    (),
                )?))
                .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                .body(Body::from(json))
                .map_err(|e| e.into())
        }();
        do_request(Arc::clone(&self.client), req)
    }

    pub fn patch<T, U>(
        &self,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        name: &str,
        patch_type: &str,
        value: &T,
    ) -> impl Future<Item = U, Error = Error> + Send
    where
        T: Serialize,
        U: DeserializeOwned + Send + 'static,
    {
        let req = || -> Result<_, Error> {
            let json = serde_json::to_vec(value)?;

            Request::builder()
                .method(Method::PATCH)
                .uri(hyper_uri(self.url(gvr, namespace, Some(name), ())?))
                .header(CONTENT_TYPE, patch_type)
                .body(Body::from(json))
                .map_err(|e| e.into())
        }();
        do_request(Arc::clone(&self.client), req)
    }

    pub fn delete(
        &self,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        name: &str,
        opts: DeleteOptions,
    ) -> impl Future<Item = (), Error = Error> + Send {
        let req = self.url(gvr, namespace, Some(name), opts).and_then(|url| {
            Request::builder()
                .method(Method::DELETE)
                .uri(hyper_uri(url))
                .body(Body::empty())
                .map_err(|e| e.into())
        });
        do_request(Arc::clone(&self.client), req)
    }

    pub fn delete_collection(
        &self,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        opts: ListOptions,
    ) -> impl Future<Item = (), Error = Error> + Send {
        let req = self.url(gvr, namespace, None, opts).and_then(|url| {
            Request::builder()
                .method(Method::DELETE)
                .uri(hyper_uri(url))
                .body(Body::empty())
                .map_err(|e| e.into())
        });
        do_request(Arc::clone(&self.client), req)
    }

    pub fn watch(
        &self,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        name: &str,
        mut opts: ListOptions,
    ) -> impl Stream<Item = WatchEvent, Error = Error> + Send {
        opts.watch = true;
        let req = self.url(gvr, namespace, Some(name), opts).and_then(|url| {
            Request::builder()
                .method(Method::GET)
                .uri(hyper_uri(url))
                .body(Body::empty())
                .map_err(|e| e.into())
        });
        do_watch(&self.client, req)
    }

    pub fn watch_list(
        &self,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        mut opts: ListOptions,
    ) -> impl Stream<Item = WatchEvent, Error = Error> + Send {
        opts.watch = true;
        let req = self.url(gvr, namespace, None, opts).and_then(|url| {
            Request::builder()
                .method(Method::GET)
                .uri(hyper_uri(url))
                .body(Body::empty())
                .map_err(|e| e.into())
        });
        do_watch(&self.client, req)
    }

    pub fn list<T>(
        &self,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        opts: ListOptions,
    ) -> impl Future<Item = T, Error = Error> + Send
    where
        T: DeserializeOwned + Send + 'static,
    {
        let req = self.url(gvr, namespace, None, opts).and_then(|url| {
            Request::builder()
                .method(Method::GET)
                .uri(hyper_uri(url))
                .body(Body::empty())
                .map_err(|e| e.into())
        });
        do_request(Arc::clone(&self.client), req)
    }

    pub fn iter<T>(
        &self,
        rsrc: T,
    ) -> impl Stream<Item = <T::List as List>::Item, Error = Error> + Send
    where
        T: Resource,
        T::List: List + DeserializeOwned + Send + 'static,
        <T::List as List>::Item: TypeMeta + DeserializeOwned + Default + Send + 'static,
    {
        self.iter_opt(rsrc, Default::default())
    }

    pub fn iter_opt<T>(
        &self,
        rsrc: T,
        opts: ListOptions,
    ) -> impl Stream<Item = <T::List as List>::Item, Error = Error> + Send
    where
        T: Resource,
        T::List: List + DeserializeOwned + Send + 'static,
        <T::List as List>::Item: TypeMeta + DeserializeOwned + Default + Send + 'static,
    {
        self._do_iter::<T::List>(rsrc.gvr(), None, opts)
    }

    fn _do_iter<L>(
        &self,
        gvr: GroupVersionResource,
        namespace: Option<&str>,
        opts: ListOptions,
    ) -> impl Stream<Item = L::Item, Error = Error> + Send
    where
        L: List + DeserializeOwned + Send + 'static,
        L::Item: TypeMeta + DeserializeOwned + Default + Send + 'static,
    {
        let url = self.url(&gvr, namespace, None, opts.clone());

        let client = Arc::clone(&self.client);
        let fetch_pages = move |url: Url| {
            stream::unfold(Some((url, opts)), move |context| {
                context.and_then(|(mut url, mut opts)| {
                    let req = Request::builder()
                        .method(Method::GET)
                        .uri(hyper_uri(url.clone()))
                        .body(Body::empty())
                        .map_err(|e| e.into());
                    let res = do_request(Arc::clone(&client), req).and_then(move |list: L| {
                        let next = match list.listmeta().continu {
                            Some(ref continu) => {
                                opts.continu = continu.clone();
                                let query = serde_urlencoded::to_string(&opts)?;
                                url.set_query(Some(&query));
                                Some((url, opts))
                            }
                            None => None,
                        };
                        Ok((list, next))
                    });
                    Some(res)
                })
            })
        };

        future::result(url)
            .and_then(move |url| future::ok(fetch_pages(url)))
            .flatten_stream()
            .map(|page| stream::iter_ok(page.into_items().into_iter()))
            .flatten()
    }
}

#[test]
fn test_url() {
    let mut context: ConfigContext = Default::default();
    context.cluster.server = "https://192.168.42.147:8443".into();
    let http = hyper::client::HttpConnector::new(1);
    let client = Client::new_from_context(http, context).unwrap();

    let url = client
        .url(
            &GroupVersionResource {
                group: "",
                version: "v1",
                resource: "pods",
            },
            Some("myns"),
            Some("myname"),
            GetOptions::default(),
        )
        .unwrap();
    assert_eq!(
        url.to_string(),
        "https://192.168.42.147:8443/api/v1/namespaces/myns/pods/myname"
    );

    let url = client
        .url(
            &GroupVersionResource {
                group: "rbac.authorization.k8s.io",
                version: "v1beta1",
                resource: "clusterroles",
            },
            None,
            Some("myrole"),
            GetOptions {
                pretty: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(url.to_string(), "https://192.168.42.147:8443/apis/rbac.authorization.k8s.io/v1beta1/clusterroles/myrole?pretty=true");

    let url = client
        .url(
            &GroupVersionResource {
                group: "",
                version: "v1",
                resource: "namespaces",
            },
            None,
            None,
            ListOptions {
                resource_version: "abcdef".into(),
                limit: 27,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        url.to_string(),
        "https://192.168.42.147:8443/api/v1/namespaces?resourceVersion=abcdef&limit=27"
    );
}
