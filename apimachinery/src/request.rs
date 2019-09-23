use crate::meta::{GroupVersionResource, ResourceScope};
use failure::Error;
use http;
use http::header::{HeaderValue, ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_urlencoded;

pub const APPLICATION_JSON: &str = "application/json";
pub const JSON_PATCH: &str = "application/json-patch+json";
pub const MERGE_PATCH: &str = "application/merge-patch+json";
pub const STRATEGIC_MERGE_PATCH: &str = "application/strategic-merge-patch+json";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Patch {
    Json(serde_json::Value),
    Merge(serde_json::Value),
    StrategicMerge(serde_json::Value),
}

impl Patch {
    pub fn content_type(&self) -> &'static str {
        match self {
            Patch::Json(_) => JSON_PATCH,
            Patch::Merge(_) => MERGE_PATCH,
            Patch::StrategicMerge(_) => STRATEGIC_MERGE_PATCH,
        }
    }
}

#[derive(Debug)]
pub struct Request<Body, Opt> {
    pub group: String,
    pub version: String,
    pub resource: String,
    pub namespace: Option<String>,
    pub name: Option<String>,
    pub subresource: Option<String>,
    pub method: http::Method,
    pub opts: Opt,
    pub content_type: Option<&'static str>,
    pub body: Body,
}

impl<Body, Opt> Request<Body, Opt> {
    pub fn gvr(&self) -> GroupVersionResource {
        GroupVersionResource {
            group: &self.group,
            version: &self.version,
            resource: &self.resource,
        }
    }
}

pub fn url_path<Opt>(
    gvr: &GroupVersionResource,
    namespace: Option<&str>,
    name: Option<&str>,
    subresource: Option<&str>,
    opts: Opt,
) -> String
where
    Opt: Serialize,
{
    let gv = gvr.as_gv();
    let mut components = vec![""];
    components.push(gv.api_prefix());
    if gvr.group != "" {
        components.push(&gvr.group);
    }
    components.push(&gvr.version);
    if let Some(ref ns) = namespace {
        components.extend(&["namespaces", ns]);
    }
    components.push(&gvr.resource);
    if let Some(ref n) = name {
        components.push(n);
    }
    if let Some(ref sub) = subresource {
        components.push(sub);
    }

    let mut path = components.join("/");

    let query = serde_urlencoded::to_string(&opts).unwrap();
    if query != "" {
        path.push_str("?");
        path.push_str(&query);
    }

    path
}

impl<Body, Opt> Request<Body, Opt>
where
    Body: Serialize,
    Opt: Serialize,
{
    pub fn url_path(&self) -> String {
        url_path(
            &self.gvr(),
            self.namespace.as_ref().map(|s| s.as_str()),
            self.name.as_ref().map(|s| s.as_str()),
            self.subresource.as_ref().map(|s| s.as_str()),
            &self.opts,
        )
    }

    pub fn into_http_request(self, server_base: &str) -> Result<http::Request<Vec<u8>>, Error> {
        let mut b = http::Request::builder();

        let req = b
            .uri(format!("{}{}", server_base, self.url_path()))
            .method(self.method)
            .header(ACCEPT, HeaderValue::from_static(APPLICATION_JSON));

        let req = match self.content_type {
            None => req.body(vec![]),
            Some(ct) => {
                let b = serde_json::to_vec(&self.body)?;
                req.header(CONTENT_TYPE, HeaderValue::from_static(ct))
                    .body(b)
            }
        };

        req.map_err(|e| e.into())
    }
}

#[derive(Debug)]
pub struct Builder<Body, Opt> {
    group: String,
    version: String,
    resource: String,
    namespace: Option<String>,
    name: Option<String>,
    subresource: Option<String>,
    method: http::Method,
    opts: Opt,
    content_type: Option<&'static str>,
    body: Body,
}

impl Request<(), ()> {
    pub fn builder(gvr: GroupVersionResource) -> Builder<(), ()> {
        Builder {
            group: gvr.group.to_string(),
            version: gvr.version.to_string(),
            resource: gvr.resource.to_string(),
            subresource: None,
            namespace: None,
            name: None,
            method: http::Method::GET,
            opts: (),
            content_type: None,
            body: (),
        }
    }
}

/// A `Request` builder.
///
/// # Example
///
/// ```rust
/// use kubernetes_apimachinery::request::Request;
/// use kubernetes_apimachinery::meta::v1::GetOptions;
/// # use kubernetes_apimachinery::meta::GroupVersionResource;
/// # struct Service;
/// # impl Service {
/// #   fn gvr() -> GroupVersionResource<'static> {
/// #     GroupVersionResource {
/// #       group: "",
/// #       version: "v1",
/// #       resource: "services",
/// #     }
/// #   }
/// # }
///
/// let req = Request::builder(Service::gvr())
///     .namespace("default")
///     .name("kubernetes")
///     .opts(GetOptions{
///         resource_version: "xyz".to_string(),
///         ..Default::default()
///     })
///     .build();
///
/// assert_eq!(req.method, http::Method::GET);
/// assert_eq!(
///     req.url_path(),
///     "/api/v1/namespaces/default/services/kubernetes?resourceVersion=xyz",
/// );
/// ```
impl<Body, Opt> Builder<Body, Opt> {
    pub fn build(self) -> Request<Body, Opt> {
        Request {
            group: self.group,
            version: self.version,
            resource: self.resource,
            namespace: self.namespace,
            name: self.name,
            subresource: self.subresource,
            method: self.method,
            opts: self.opts,
            content_type: self.content_type,
            body: self.body,
        }
    }

    pub fn namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = Some(ns.into());
        self
    }

    pub fn namespace_maybe(mut self, ns: Option<impl Into<String>>) -> Self {
        self.namespace = ns.map(|inner| inner.into());
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn name_maybe(mut self, name: Option<impl Into<String>>) -> Self {
        self.name = name.map(|inner| inner.into());
        self
    }

    pub fn scope(mut self, name: impl ResourceScope) -> Self {
        self.namespace = name.namespace().map(|s| s.to_string());
        self.name = name.name().map(|s| s.to_string());
        self
    }

    pub fn method(mut self, method: http::Method) -> Self {
        self.method = method;
        self
    }

    pub fn subresource(mut self, sub: impl Into<String>) -> Self {
        self.subresource = Some(sub.into());
        self
    }

    pub fn opts<O>(self, opts: O) -> Builder<Body, O> {
        Builder {
            group: self.group,
            version: self.version,
            resource: self.resource,
            namespace: self.namespace,
            name: self.name,
            subresource: self.subresource,
            method: self.method,
            opts: opts,
            content_type: self.content_type,
            body: self.body,
        }
    }

    pub fn body<B>(self, content_type: &'static str, body: B) -> Builder<B, Opt> {
        Builder {
            group: self.group,
            version: self.version,
            resource: self.resource,
            namespace: self.namespace,
            name: self.name,
            subresource: self.subresource,
            method: self.method,
            opts: self.opts,
            content_type: Some(content_type),
            body: body,
        }
    }

    pub fn opts_mut(&mut self) -> &mut Opt {
        &mut self.opts
    }

    pub fn body_mut(&mut self) -> &mut Body {
        &mut self.body
    }
}

#[test]
fn namespaced_to_http() {
    use serde_json::json;

    let proxy_req = Request {
        group: "".to_string(),
        version: "v1".to_string(),
        resource: "services".to_string(),
        namespace: Some("myns".to_string()),
        name: Some("mysvc".to_string()),
        subresource: Some("proxy".to_string()),
        method: http::Method::POST,
        opts: json!({"path": "foo"}),
        content_type: Some(APPLICATION_JSON),
        body: json!({"foo": "bar"}),
    };

    assert_eq!(
        proxy_req.url_path(),
        "/api/v1/namespaces/myns/services/mysvc/proxy?path=foo"
    );

    let hreq = proxy_req.into_http_request("http://server:1234").unwrap();
    assert_eq!(hreq.method(), http::Method::POST);
    assert_eq!(
        hreq.uri(),
        "http://server:1234/api/v1/namespaces/myns/services/mysvc/proxy?path=foo"
    );

    assert_eq!(hreq.body(), br#"{"foo":"bar"}"#);
}

#[test]
fn non_namespaced() {
    use serde_json::json;

    let delete_req = Request {
        group: "rbac.authorization.k8s.io".to_string(),
        version: "v1".to_string(),
        resource: "clusterroles".to_string(),
        namespace: None,
        name: Some("mycr".to_string()),
        subresource: None,
        method: http::Method::DELETE,
        opts: json!({}),
        content_type: None,
        body: (),
    };

    assert_eq!(
        delete_req.url_path(),
        "/apis/rbac.authorization.k8s.io/v1/clusterroles/mycr",
    );

    let hreq = delete_req.into_http_request("http://example:1234").unwrap();
    assert_eq!(hreq.method(), http::Method::DELETE);
    assert_eq!(
        hreq.uri(),
        "http://example:1234/apis/rbac.authorization.k8s.io/v1/clusterroles/mycr"
    );
    assert_eq!(hreq.body(), b"");
}
