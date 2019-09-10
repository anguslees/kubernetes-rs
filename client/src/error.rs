use failure::Fail;
use http;
use log::debug;
use serde_json;
use std::str;

#[derive(Debug, Fail)]
pub enum ClientError {
    /// What was being attempted when the decode error occured.
    // https://github.com/rust-lang-nursery/failure/issues/183 prevents
    // easily deferring rendering until needed.
    // For now, we render at instantiation as errors are relatively few
    // we can amortise the costs well.
    ///
    /// description: Human summarised version of the failure.
    /// The underlying failure is found in the first cause of the error chain.
    /// The bytes that failed to decode.
    /// TODO: Also capture the type that was being decoded into.
    #[fail(display = "unable to parse {}: {}", description, _summary)]
    DecodeFailed {
        description: String,
        _summary: String,
        bytes: Vec<u8>,
    },

    #[fail(display = "Unexpected HTTP response status: {}", status)]
    HttpStatusError { status: http::StatusCode },

    #[fail(display = "Attribute {} required but not provided", attr)]
    RequiredAttributeError { attr: &'static str },
}

impl ClientError {
    pub fn new_decode_error(
        description: &str,
        e: &serde_json::Error,
        bytes: Vec<u8>,
    ) -> ClientError {
        ClientError::DecodeFailed {
            description: description.to_string(),
            _summary: context(&e, &bytes),
            bytes: bytes,
        }
    }
}

/// Pull out the 1K preceeding text from the failed document to aid diagnosis by users.
///
/// TODO: handle multi-line JSON, just in case some API server decides to start emitting that.
fn context(e: &serde_json::Error, body_ref: &[u8]) -> String {
    // debug! so that operators running with debug logs get *everything*
    debug!("Parse failure: {}, {:#?}", e, body_ref);
    // Provide a short snippet for errors that may be handled, logged at higher verbosity etc.
    match e.classify() {
        serde_json::error::Category::Io => format!("{}", e),
        _ => {
            // Either bad structure/values in the JSON (so show it) or bad contents (so show it)
            // TODO: ditch the unwrap()
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
            format!("{} {}", body_snippet, e)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct SampleObject {
        pub required_field: String,
    }

    #[test]
    fn test_client_error() {
        let doc = "{\"doc\": 1}";
        let err = serde_json::from_slice::<SampleObject>(doc.as_bytes())
            .err()
            .map(|e| {
                super::ClientError::new_decode_error("error Status", &e, doc.as_bytes().to_vec())
            })
            .unwrap();
        assert_eq!(
            String::from("unable to parse error Status: {\"doc\": 1} missing field `requiredField` at line 1 column 10"),
            format!("{}", err));
    }
}
