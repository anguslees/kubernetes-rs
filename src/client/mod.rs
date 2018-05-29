use futures::{future, stream, Future, Stream};
use tokio_core::reactor::Handle;
use hyper_tls::HttpsConnector;
use native_tls::{TlsConnector,Pkcs12,Certificate};
use hyper::{self,Method};
use hyper::header::{ContentType,ContentLength};
use serde_json;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_urlencoded;
use url::Url;
use std::default::Default;
use std::fmt;
use std::env;
use std::path::PathBuf;
use failure::{Error,ResultExt};
use openssl;

pub mod config;
mod resplit;

use self::config::ConfigContext;
use super::{Metadata,List,GroupVersionResource};
use super::api::meta::v1::{WatchEvent,Status};

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
    RequiredAttributeError{attr: attr}
}

#[derive(Debug)]
pub struct Client<C> {
    pub client: hyper::Client<C>,
    config: ConfigContext,
}

impl<C: Clone> Clone for Client<C> {
    fn clone(&self) -> Client<C> {
        Client {
            client: self.client.clone(),
            config: self.config.clone(),
        }
    }
}

impl Client<HttpsConnector<hyper::client::HttpConnector>> {
    pub fn new(threads: usize, handle: &Handle) -> Result<Self, Error> {
        let config_path = env::var_os(config::CONFIG_ENV).map(PathBuf::from)
            .or_else(config::default_path)
            .ok_or(format_err!("Unable to find config"))?;
        debug!("Reading config from {}", config_path.display());
        let config = config::load_from_file(&config_path)
            .context(format!("unable to read {}", config_path.display()))?;
        let context = config.config_context(&config.current_context)?;
        let client = Client::new_from_context(threads, handle, context)?;
        Ok(client)
    }

    pub fn new_from_context(threads: usize, handle: &Handle, config: ConfigContext) -> Result<Self, Error> {
        let mut http = hyper::client::HttpConnector::new(threads, handle);
        http.enforce_http(false);
        let mut tls = TlsConnector::builder()?;
        if let (Some(certdata), Some(keydata)) = (
            config.user.client_certificate_read(), config.user.client_key_read()) {
            debug!("Setting user client cert");
            let cert = openssl::x509::X509::from_pem(&certdata?)?;
            let pkey = openssl::pkey::PKey::private_key_from_pem(&keydata?)?;
            // openssl pkcs12 -export -clcerts -inkey kubecfg.key -in kubecfg.crt -out kubecfg.p12 -name "kubecfg"
            let password = "";
            let p12 = openssl::pkcs12::Pkcs12::builder()
                .build(password, "kubeconfig", &pkey, &cert)?;
            tls.identity(Pkcs12::from_der(&p12.to_der()?, password)?)?;
        }

        if let Some(data) = config.cluster.certificate_authority_read() {
            debug!("Setting cluster CA cert");
            let cert = Certificate::from_pem(&data?)?;
            // FIXME: want to validate against _only_ this cert ..
            tls.add_root_certificate(cert)?;
        }

        // FIXME: config.cluster.insecure_skip_tls_verify

        let hyper_client = hyper::Client::configure()
            .connector(HttpsConnector::from((http, tls.build()?)))
            .build(handle);

        Self::new_with_client(hyper_client, config)
    }
}

impl<C> Client<C> {
    pub fn new_with_client(client: hyper::Client<C>, config: ConfigContext) -> Result<Self, Error> {
        Ok(Client{client: client, config: config})
    }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == Default::default()
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default,rename_all="camelCase")]
pub struct GetOptions {
    #[serde(skip_serializing_if="is_default")]
    pub pretty: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default,rename_all="camelCase")]
pub struct ListOptions {
    #[serde(skip_serializing_if="is_default")]
    pub resource_version: String, // Vec<u8>
    #[serde(skip_serializing_if="is_default")]
    pub timeout_seconds: u32,
    #[serde(skip_serializing_if="is_default")]
    pub watch: bool,  // NB: set explicitly by watch()
    #[serde(skip_serializing_if="is_default")]
    pub pretty: bool,
    #[serde(skip_serializing_if="is_default")]
    pub field_selector: String,
    #[serde(skip_serializing_if="is_default")]
    pub label_selector: String,
    #[serde(skip_serializing_if="is_default")]
    pub include_uninitialized: bool,
    #[serde(skip_serializing_if="is_default")]
    pub limit: u32,
    #[serde(skip_serializing_if="is_default",rename="continue")]
    pub continu: String, // Vec<u8>
}

fn hyper_uri(u: Url) -> hyper::Uri {
    u.to_string().parse()
        .expect("attempted to convert invalid uri")
}

impl<C: hyper::client::Connect + Clone> Client<C> {
    fn req_build<O>(
        &self,
        method: Method,
        gvr: &GroupVersionResource,
        namespace: Option<&str>,
        name: Option<&str>,
        opts: O,
    ) -> Result<hyper::Request, Error>
        where O: Serialize + Default + PartialEq + fmt::Debug
    {
        let mut url: Url = self.config.cluster.server.parse()?;

        {
            let mut path = url.path_segments_mut()
                .map_err(|_| format_err!("URL scheme does not support paths"))?;
            path.clear();
            path.push(if gvr.group == "" && gvr.version == "v1" { "api" } else { "apis" });
            if gvr.group != "" {
                path.push(&gvr.group);
            }
            path.push(&gvr.version);
            if let Some(ns) = namespace {
                path.extend(&["namespaces", ns]);
            }
            path.push(&gvr.resource);
            if let Some(n) = name {
                path.push(n);
            }
        }

        let params = if is_default(&opts) {
            None
        } else {
            let urlopts = serde_urlencoded::to_string(&opts)
                .context(format!("while encoding URL parameters {:?}", opts))?;
            Some(urlopts)
        };
        url.set_query(params.as_ref().map(|v| v.as_str()));

        let req = hyper::Request::new(method, hyper_uri(url));
        Ok(req)
    }

    pub fn get<T>(&self, gvr: &GroupVersionResource, namespace: Option<&str>, name: &str, opts: GetOptions) -> Box<Future<Item=T, Error=Error>>
        where T: DeserializeOwned + 'static
    {
        match self.req_build(Method::Get, gvr, namespace, Some(name), opts) {
            Ok(req) => self.request(req),
            Err(e) => Box::new(future::err(e)),
        }
    }

    pub fn put<T>(&self, gvr: &GroupVersionResource, value: &T, opts: GetOptions) -> Box<Future<Item=T, Error=Error>>
        where T: Metadata + Serialize + DeserializeOwned + 'static
    {
        let req = || {
            let metadata = value.metadata();
            let namespace = &metadata.namespace; // NB: assumes input object is correctly qualified
            let name = metadata.name.as_ref().ok_or(required_attr("name"))?;
            self.req_build(Method::Post, gvr, namespace.as_ref().map(|v| v.as_str()), Some(&name), opts)
                .and_then(|mut req| {
                    let json = serde_json::to_vec(value)?;
                    req.headers_mut().set(ContentType::json());
                    req.headers_mut().set(ContentLength(json.len() as u64));
                    req.set_body(json);
                    Ok(req)
                })
        };
        match req() {
            Ok(req) => self.request(req),
            Err(e) => Box::new(future::err(e)),
        }
    }

    pub fn watch(&self, gvr: &GroupVersionResource, namespace: Option<&str>, name: &str, mut opts: ListOptions) -> Box<Stream<Item=WatchEvent, Error=Error>>
    {
        opts.watch = true;
        match self.req_build(Method::Get, gvr, namespace, Some(name), opts) {
            Ok(req) => self.watch_request(req),
            Err(e) => Box::new(future::err(e).into_stream()),
        }
    }

    pub fn list<T>(&self, gvr: &GroupVersionResource, namespace: Option<&str>, opts: ListOptions) -> Box<Future<Item=T, Error=Error>>
        where T: DeserializeOwned + 'static
    {
        match self.req_build(Method::Get, gvr, namespace, None, opts) {
            Ok(req) => self.request(req),
            Err(e) => Box::new(future::err(e)),
        }
    }

    pub fn iter<L,T>(&self, gvr: &GroupVersionResource, namespace: Option<&str>, opts: ListOptions) -> Box<Stream<Item=T, Error=Error>>
        where L: List<T> + DeserializeOwned + 'static, T: 'static
    {
        let url = match self.req_build(Method::Get, gvr, namespace, None, opts.clone()) {
            Ok(v) => Url::parse(&v.uri().to_string()).unwrap(),
            Err(e) => return Box::new(future::err(e).into_stream()),
        };
        let client_copy = self.clone();
        let res = stream::unfold((url, opts, true), move |(mut url, mut opts, more)| {
            if more {
                let req = hyper::Request::new(Method::Get, hyper_uri(url.clone()));
                let res = client_copy.request(req)
                    .and_then(move |list: L| {
                        let (opts, more) = {
                            let meta = list.listmeta();
                            debug!("listmeta: {:#?}", meta);
                            let more = meta.continu.is_some();
                            opts.continu = meta.continu.as_ref()
                                .map(|s| s.clone())
                                .unwrap_or_default();
                            let query = serde_urlencoded::to_string(&opts)?;
                            url.set_query(Some(&query));
                            (opts, more)
                        };
                        Ok((list, (url, opts, more)))
                    });
                Some(res)
            } else {
                None
            }
        })
            .map(|list| stream::iter_ok(list.into_items().into_iter()))
            .flatten();
        Box::new(res)
    }

    pub fn watch_list(&self, gvr: &GroupVersionResource, namespace: Option<&str>, mut opts: ListOptions) -> Box<Stream<Item=WatchEvent, Error=Error>>
    {
        opts.watch = true;
        match self.req_build(Method::Get, gvr, namespace, None, opts) {
            Ok(req) => self.watch_request(req),
            Err(e) => Box::new(future::err(e).into_stream()),
        }
    }

    pub fn request<T>(&self, req: hyper::Request) -> Box<Future<Item=T, Error=Error>>
        where T: DeserializeOwned + 'static
    {
        debug!("Request: {:#?}", req);
        let f = self.client.request(req).from_err::<Error>()
            // TODO: add method/uri context to error
            .inspect(|res| debug!("Response: {:#?}", res))
            .and_then(|res| {
                let status = res.status();
                res.body().concat2().map(move |body| (status, body)).from_err()
            })
            .and_then(move |(httpstatus, body)| -> Result<T, Error> {
                if !httpstatus.is_success() {
                    debug!("failure body: {:#?}", ::std::str::from_utf8(body.as_ref()));
                    let status: Status = serde_json::from_slice(body.as_ref())
                        .map_err(|e| {
                            debug!("Failed to parse error Status ({}), falling back to HTTP status", e);
                            HttpStatusError{status: httpstatus}
                        })?;
                    Err(status.into())
                } else {
                    let o = serde_json::from_slice(body.as_ref())
                        .context(format!("while parsing response body: {}", String::from_utf8_lossy(body.as_ref())))?;
                    Ok(o)
                }
            });
        Box::new(f)
    }

    pub fn watch_request<T>(&self, req: hyper::Request) -> Box<Stream<Item=T, Error=Error>>
        where T: DeserializeOwned + 'static
    {
        debug!("Request: {:#?}", req);
        let f = self.client.request(req).from_err::<Error>()
            // TODO: add method/uri context to error
            .inspect(|res| debug!("Response: {:#?}", res))
            .and_then(|res| -> Box<Future<Item=_, Error=Error>> {
                let httpstatus = res.status();
                if !httpstatus.is_success() {
                    let err = res.body()
                        .concat2().from_err::<Error>()
                        .and_then(move |body| {
                            debug!("failure body: {:#?}", ::std::str::from_utf8(body.as_ref()));
                            let status: Status = serde_json::from_slice(body.as_ref())
                                .map_err(|e| {
                                    debug!("Failed to parse error Status ({}), falling back to HTTP status", e);
                                    HttpStatusError{status: httpstatus}
                                })?;

                            Err(status.into())
                        });
                    Box::new(err)
                } else {
                    let stream = resplit::new(res.body(), |&c| c == b'\n').from_err()
                        .inspect(|line| debug!("Got line: {:#?}", ::std::str::from_utf8(line).unwrap_or("<invalid utf8>")))
                        .and_then(move |line| {
                            let o: T = serde_json::from_slice(line.as_ref())
                                .context(format!("while parsing watch line : {}", String::from_utf8_lossy(line.as_ref())))?;
                            Ok(o)
                        });
                    Box::new(future::ok(stream))
                }
            })
            .flatten_stream();
        Box::new(f)
    }
}

#[test]
fn test_req_build() {
    use tokio_core::reactor::Core;
    let core = Core::new().unwrap();
    let mut context: ConfigContext = Default::default();
    context.cluster.server = "https://192.168.42.147:8443".into();
    let client = Client::new_from_context(1, &core.handle(), context).unwrap();

    let req = client.req_build(
        Method::Get,
        &GroupVersionResource{group: "", version: "v1", resource: "pods"},
        Some("myns"),
        Some("myname"),
        GetOptions::default(),
    ).unwrap();
    assert_eq!(*req.uri(), "https://192.168.42.147:8443/api/v1/namespaces/myns/pods/myname");

    let req = client.req_build(
        Method::Post,
        &GroupVersionResource{group: "rbac.authorization.k8s.io", version: "v1beta1", resource: "clusterroles"},
        None,
        Some("myrole"),
        GetOptions{pretty: true, ..Default::default()},
    ).unwrap();
    assert_eq!(*req.uri(), "https://192.168.42.147:8443/apis/rbac.authorization.k8s.io/v1beta1/clusterroles/myrole?pretty=true");

    let req = client.req_build(
        Method::Get,
        &GroupVersionResource{group: "", version: "v1", resource: "namespaces"},
        None,
        None,
        ListOptions{resource_version: "abcdef".into(), limit: 27, ..Default::default()},
    ).unwrap();
    assert_eq!(*req.uri(), "https://192.168.42.147:8443/api/v1/namespaces?resourceVersion=abcdef&limit=27");
}
