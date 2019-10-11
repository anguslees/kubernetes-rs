use crate::config::{self, ConfigContext, CONFIG_ENV};
use async_trait::async_trait;
use failure::{format_err, Error, ResultExt};
use futures::compat::{Compat01As03, Future01CompatExt, Stream01CompatExt};
use futures::io::AsyncRead;
use futures::stream::TryStreamExt;
use http;
use hyper;
use hyper_tls::HttpsConnector;
use kubernetes_apimachinery::client::ApiClient;
use kubernetes_apimachinery::meta::v1::Status;
use kubernetes_apimachinery::{HttpService, HttpUpgradeService};
use log::debug;
use native_tls::{Certificate, Identity, TlsConnector};
use openssl;
use std::env;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

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
    // NB: hyper's ResponseFuture.inner is 'static, which requires this 'static
    C: hyper::client::connect::Connect + 'static,
{
    async fn http_request(
        &self,
        req: http::Request<hyper::Body>,
    ) -> Result<http::Response<hyper::Body>, Error> {
        // Avoid printing body, since it may not be Debug
        debug!("Request: {} {}", req.method(), req.uri());
        let res = self.client.request(req).compat().await?;

        // NB: http::header will omit headers marked "sensitive"
        debug!("Response: {} {:?}", res.status(), res.headers());
        // Verbose!
        //debug!("Response: {:#?}", res);

        let httpstatus = res.status();
        if httpstatus.is_success() {
            Ok(res)
        } else {
            // HTTP non-2xx response
            let body = res.into_body().compat().try_concat().await?;
            let status = Status::from_vec(body.to_vec())?;
            Err(status.into())
        }
    }
}

#[async_trait]
impl<C> HttpService for Client<C>
where
    C: hyper::client::connect::Connect + 'static,
{
    type Body = hyper::Chunk;
    type Read = Pin<Box<dyn AsyncRead + Send>>;

    async fn request(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Body>, Error> {
        let hreq = req.map(|b| b.into());
        let resp = self.http_request(hreq).await?;

        // Read body content into memory
        let (parts, body) = resp.into_parts();
        let body = body.compat().try_concat().await?;
        let resp = http::Response::from_parts(parts, body.into());
        Ok(resp)
    }

    async fn watch(
        &self,
        req: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Self::Read>, Error> {
        let hreq = req.map(|b| b.into());
        let resp = self.http_request(hreq).await?;

        let (parts, body) = resp.into_parts();
        let bodyreader = Compat01As03::new(body)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            .into_async_read();
        let resp = http::Response::from_parts(parts, Box::pin(bodyreader) as Self::Read);
        Ok(resp)
    }
}

#[async_trait]
impl<C> HttpUpgradeService for Client<C>
where
    C: hyper::client::connect::Connect + 'static,
{
    type Upgraded = Compat01As03<hyper::upgrade::Upgraded>;

    async fn upgrade(&self, req: http::Request<()>) -> Result<Self::Upgraded, Error> {
        let res = self
            .http_request(req.map(|()| hyper::Body::empty()))
            .await?;
        let upgraded = res.into_body().on_upgrade().compat().await?;
        Ok(Compat01As03::new(upgraded))
    }
}
