use crate::config::{self, ConfigContext, CONFIG_ENV};
use apimachinery::client::ApiClient;
use apimachinery::meta::v1::Status;
use apimachinery::{HttpService, HttpUpgradeService};
use bytes::{Bytes, BytesMut};
use failure::{Error, ResultExt};
use futures::{future, Future, Sink, Stream};
use http;
use hyper;
use hyper_tls::HttpsConnector;
use native_tls::{Certificate, Identity, TlsConnector};
use openssl;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_codec::{BytesCodec, Framed};

/// An implementation of apimachinery::HttpClient using hyper.
#[derive(Debug, Clone)]
pub struct Client<C> {
    client: Arc<hyper::Client<C>>,
    config: ConfigContext,
}

impl Client<HttpsConnector<hyper::client::HttpConnector>> {
    pub fn new() -> Result<ApiClient<Self>, Error> {
        let dns_threads = 1; // Only need a single DNS lookup
        let http = hyper::client::HttpConnector::new(dns_threads);
        Client::new_from_http(http)
    }

    pub fn new_from_http(http: hyper::client::HttpConnector) -> Result<ApiClient<Self>, Error> {
        let config_path = env::var_os(CONFIG_ENV)
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
    ) -> Result<ApiClient<Self>, Error> {
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

        let hyper_client = hyper::Client::builder()
            .keep_alive(true)
            .build(HttpsConnector::from((http, tls.build()?)));

        Self::new_with_client(hyper_client, config)
    }
}

impl<C> Client<C> {
    pub fn new_with_client(
        client: hyper::Client<C>,
        config: ConfigContext,
    ) -> Result<ApiClient<Self>, Error> {
        let base_url = config.cluster.server.clone();
        let c = Client {
            client: Arc::new(client),
            config: config,
        };
        Ok(ApiClient::new(c, base_url))
    }
}

impl<C> Client<C>
where
    C: hyper::client::connect::Connect + 'static,
{
    fn http_request(
        &self,
        req: http::Request<hyper::Body>,
    ) -> impl Future<Item = http::Response<hyper::Body>, Error = Error> {
        // Avoid printing body, since it may not be Debug
        debug!("Request: {} {}", req.method(), req.uri());
        self.client
            .request(req)
            .inspect(|res| {
                // NB: http::header will omit headers marked "sensitive"
                debug!("Response: {} {:?}", res.status(), res.headers());
                // Verbose!
                //debug!("Response: {:#?}", res);
            })
            .from_err::<Error>()
            .and_then(|res: http::Response<_>| {
                let httpstatus = res.status();
                let r = if httpstatus.is_success() {
                    Ok(res)
                } else {
                    Err(res)
                };
                future::result(r).or_else(move |res| {
                    res.into_body()
                        .concat2()
                        .from_err::<Error>()
                        .and_then(move |body| {
                            let status = Status::from_vec(body.to_vec())?;
                            Err(status.into())
                        })
                })
            })
    }
}

impl<C> HttpService for Client<C>
where
    C: hyper::client::connect::Connect + 'static,
{
    type Body = hyper::Chunk;
    type Future = Box<Future<Item = http::Response<Self::Body>, Error = Error> + Send>;
    type Stream = Box<Stream<Item = Self::Body, Error = Error> + Send>;
    type StreamFuture = Box<Future<Item = http::Response<Self::Stream>, Error = Error> + Send>;

    fn request(&self, req: http::Request<Vec<u8>>) -> Self::Future {
        let hreq = req.map(|b| b.into());
        let f = self
            .http_request(hreq)
            .and_then(|resp: http::Response<hyper::Body>| {
                // Read body content into memory
                let (parts, body) = resp.into_parts();
                body.concat2()
                    .from_err()
                    .map(move |b| http::Response::from_parts(parts, b.into()))
            });
        Box::new(f)
    }

    fn watch(&self, req: http::Request<Vec<u8>>) -> Self::StreamFuture {
        let hreq = req.map(|b| b.into());
        let f = self
            .http_request(hreq)
            .from_err()
            .map(|resp: http::Response<hyper::Body>| {
                resp.map(|body| Box::new(body.from_err()) as Self::Stream)
            });
        Box::new(f)
    }
}

impl<C> HttpUpgradeService for Client<C>
where
    C: hyper::client::connect::Connect + 'static,
{
    type Sink = Box<Sink<SinkItem = Self::SinkItem, SinkError = Error> + Send>;
    type SinkItem = Bytes;
    type Stream = Box<Stream<Item = Self::StreamItem, Error = Error> + Send>;
    type StreamItem = BytesMut;
    type Future = Box<Future<Item = (Self::Stream, Self::Sink), Error = Error> + Send>;

    fn upgrade(&self, req: http::Request<()>) -> Self::Future {
        let f = self
            .http_request(req.map(|()| hyper::Body::empty()))
            .and_then(|res| res.into_body().on_upgrade().from_err())
            .map(|upgraded| {
                let (sink, stream) = Framed::new(upgraded, BytesCodec::new()).split();
                (
                    Box::new(stream.from_err()) as Self::Stream,
                    Box::new(sink.sink_from_err()) as Self::Sink,
                )
            });
        Box::new(f)
    }
}
