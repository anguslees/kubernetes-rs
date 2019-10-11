use crate::meta::v1::Status;
use crate::request::APPLICATION_JSON;
use failure::{bail, Error, Fail, ResultExt};
use http;
use http::header::{HeaderValue, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use std::fmt;
use std::str;

#[derive(Fail, Debug)]
pub struct DecodeError {
    line: usize,
    column: usize,
    input: Vec<u8>,
}

impl DecodeError {
    pub fn new(cause: &serde_json::Error, input: Vec<u8>) -> Self {
        DecodeError {
            line: cause.line(),
            column: cause.column(),
            input: input,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "unable to parse: ")?;

        if let Ok(s) = str::from_utf8(&self.input) {
            let line = s
                .lines()
                .nth(self.line - 1)
                .expect("position outside content");
            let start_n = self.column.saturating_sub(1024);
            let snippet = &line[start_n..self.column];
            write!(f, "{}", snippet)?;
        }

        Ok(())
    }
}

#[derive(Debug, Fail)]
#[fail(display = "unexpected content-type: {:?}", value)]
pub struct UnknownContentTypeError {
    value: HeaderValue,
}

#[derive(Debug)]
pub struct Response<B> {
    status: http::StatusCode,
    body: B,
}

impl<B> Response<B> {
    pub fn ok(body: B) -> Self {
        Response {
            status: http::StatusCode::OK,
            body: body,
        }
    }

    pub fn status(&self) -> http::StatusCode {
        self.status
    }

    pub fn body(&self) -> &B {
        &self.body
    }

    pub fn into_body(self) -> B {
        self.body
    }
}

impl<B> Response<B>
where
    B: DeserializeOwned + Send,
{
    // Should be TryFrom, once that stabilises.
    pub fn from_http_response<A: AsRef<[u8]>>(resp: http::Response<A>) -> Result<Self, Error> {
        let (parts, body) = resp.into_parts();
        if parts.status.is_success() {
            let b = match parts.headers.get(CONTENT_TYPE) {
                Some(ct) if ct == APPLICATION_JSON => serde_json::from_slice(body.as_ref())
                    .with_context(|e| DecodeError::new(e, body.as_ref().to_vec()))?,
                Some(ct) => {
                    bail!(UnknownContentTypeError { value: ct.clone() });
                }
                // This will most likely produce an error if a
                // non-trivial body was expected.
                None => serde_json::from_value(serde_json::Value::Null)?,
            };
            Ok(Response {
                status: parts.status,
                body: b,
            })
        } else {
            let status = Status::from_vec(body.as_ref().to_vec())?;
            Err(status.into())
        }
    }
}

#[test]
fn deser_empty() {
    let hresp = http::Response::builder()
        .status(http::StatusCode::OK)
        // no content-type
        .body(b"ignored".to_vec())
        .unwrap();
    let resp: Response<()> = Response::from_http_response(hresp).unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
}

#[test]
fn deser_json() {
    use serde_json::{json, Value};
    let hresp = http::Response::builder()
        .status(http::StatusCode::OK)
        .header(CONTENT_TYPE, APPLICATION_JSON)
        .body(br#"{"foo": "bar"}"#.to_vec())
        .unwrap();
    let resp: Response<Value> = Response::from_http_response(hresp).unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    assert_eq!(*resp.body(), json!({"foo": "bar"}));
}

#[test]
fn deser_error() {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    struct SampleObject {
        field: String,
    }

    let hresp = http::Response::builder()
        .header(CONTENT_TYPE, APPLICATION_JSON)
        .body(br#"{"field": 42}"#.to_vec()) // Note: `field` value is wrong type
        .unwrap();
    let r: Result<Response<SampleObject>, _> = Response::from_http_response(hresp);
    assert!(r.is_err());
    let e = r.unwrap_err();
    assert_eq!(e.as_fail().to_string(), "unable to parse: {\"field\": 42");
    assert!(e
        .as_fail()
        .cause()
        .unwrap()
        .downcast_ref::<serde_json::Error>()
        .is_some());
}
