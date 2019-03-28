use crate::meta::GroupVersionResource;
use failure::Error;
use http;
use http::header::{HeaderValue, ACCEPT, CONTENT_TYPE};
use serde::Serialize;
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
pub struct Request<B, O> {
    pub group: String,
    pub version: String,
    pub resource: String,
    pub namespace: Option<String>,
    pub name: Option<String>,
    pub subresource: Option<String>,
    pub method: http::Method,
    pub opts: O,
    pub content_type: Option<&'static str>,
    pub body: B,
}

impl<B, O> Request<B, O> {
    pub fn gvr(&self) -> GroupVersionResource {
        GroupVersionResource {
            group: &self.group,
            version: &self.version,
            resource: &self.resource,
        }
    }
}

pub fn url_path<O>(
    gvr: &GroupVersionResource,
    namespace: Option<&str>,
    name: Option<&str>,
    subresource: Option<&str>,
    opts: O,
) -> String
where
    O: Serialize,
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

impl<B, O> Request<B, O>
where
    B: Serialize,
    O: Serialize,
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

#[test]
fn namespaced_to_http() {
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
