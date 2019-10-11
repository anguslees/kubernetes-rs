#![warn(unused_extern_crates)]

use base64;
use failure::{format_err, Error};
use har;
use har::v1_3;
use pest;
use pest::Parser;
use pest_derive::Parser;
use pretty_env_logger;
use serde_json;
use std::env;
use std::fs;
use std::result::Result;

#[derive(Parser)]
#[grammar_inline = r#"
level = { "I" | "E" | "W" }
digit = { '0'..'9' }
ws = _{ (" " | " " | " ")+ }
digits = { digit+ }
pid = @{ digits }
filename = { (ALPHABETIC | "_" | ".")+ }

code = @{ level ~ digit ~ digit ~ digit ~ digit }



time = @{ digits ~ ":" ~ digits ~ ":" ~ digits ~ "." ~ digits }
prelude = @{ code ~ ws ~ time ~ ws ~ pid ~ ws ~ filename ~ ":" ~ digits ~ "] " }
junk = { (!NEWLINE ~ ANY)* }
//response = { "Response Body: " ~ json }
method = @{ "POST" | "GET" | "PATCH" | "PUT" | "DELETE" }
url = { (!(NEWLINE | "'") ~ ANY)+ }
request_line = _{ prelude ~ method ~ ws ~ url ~ NEWLINE}
curl_header = {"-H" ~ ws ~ "\"" ~ field ~ ": " ~ value ~ "\""}
curl_headers = { (ws ~ curl_header)* ~ ws }
curl_request = _{ prelude ~ "curl -k -v -X" ~ method ~ curl_headers ~ "'" ~ url ~ "'" ~ NEWLINE}
field = { (ASCII_ALPHA | "-")+ }
value = { (!(NEWLINE | "\"") ~ ANY)+ }
header_line = { prelude ~ ws ~ field ~ ": " ~ value ~ NEWLINE }
header_lines = _{ (header_line)* }
request_headers = { prelude ~ "Request Headers:" ~ NEWLINE ~ header_lines }
response_status = { prelude ~ continued }
response_headers = { prelude ~ "Response Headers:" ~ NEWLINE ~ header_lines }
body = @{continued}
response_body = { prelude ~ "Response Body: " ~ body }
v8_request = _{ request_line ~ request_headers ~ response_status ~ response_headers ~ response_body }
v10_request = _{ curl_request ~ response_status ~ response_headers ~ response_body }
request =  { v10_request | v8_request }
content = { junk }
continued = { junk ~ NEWLINE ~ (!prelude ~ junk ~ NEWLINE)* }
log_entry = { prelude ~ continued }
file = {
  SOI ~
  (request | log_entry | (ANY* ~ NEWLINE)) * ~
  EOI
  }
"#]

struct LogParser;

struct ParseHeaders<'a> {
    iter: pest::iterators::Pairs<'a, Rule>,
}

impl<'a> Iterator for ParseHeaders<'a> {
    type Item = v1_3::Headers;

    fn next(&mut self) -> Option<v1_3::Headers> {
        let mut header = v1_3::Headers {
            name: "".to_string(),
            value: "".to_string(),
            comment: None,
        };
        for pair in &mut self.iter {
            match pair.as_rule() {
                Rule::curl_header | Rule::header_line => {
                    for pair in pair.into_inner() {
                        match pair.as_rule() {
                            Rule::prelude => {}
                            Rule::field => header.name.push_str(pair.as_str()),
                            Rule::value => {
                                header.value.push_str(pair.as_str());
                                return Some(header);
                            }
                            e => panic!("Unexpected req field-value header pair {:?}", e),
                        }
                    }
                    unreachable!()
                }
                Rule::prelude => {}
                e => panic!("Unexpected req header pair {:?}", e),
            }
        }
        if header.name.len() != 0 {
            panic!("Half-parsed header {:?}", header.name)
        }
        None
    }
}

fn parse_log(input: &str) -> Result<String, Error> {
    // the iterator must succeed given the definition of file - otherwise parse fails.
    let log = LogParser::parse(Rule::file, &input)?.next().unwrap();
    let mut result = String::new();
    let mut entries: Vec<v1_3::Entries> = Vec::new();
    for record in log.into_inner() {
        match record.as_rule() {
            Rule::log_entry => {
                // result.push_str(&format!("plain entry {}", record.as_str())),
            }
            Rule::request => {
                // println!("YY{:?}", record);
                let mut entry = v1_3::Entries {
                    pageref: None,
                    // TODO: put something in this; log misses date.
                    started_date_time: String::new(),
                    // TODO: comes from response time
                    time: 0,
                    request: v1_3::Request {
                        method: "unset".to_string(),
                        url: "unset".to_string(),
                        http_version: "unknown".to_string(),
                        cookies: Vec::new(),
                        headers: Vec::new(),
                        query_string: Vec::new(),
                        post_data: None,
                        headers_size: -1,
                        body_size: -1,
                        comment: None,
                        headers_compression: None,
                    },
                    response: v1_3::Response {
                        charles_status: None,
                        status: -1,
                        status_text: "".to_string(),
                        http_version: "unknown".to_string(),
                        cookies: Vec::new(),
                        headers: Vec::new(),
                        content: v1_3::Content {
                            size: -1,
                            compression: None,
                            mime_type: "".to_string(),
                            text: None,
                            encoding: None,
                            comment: None,
                        },
                        redirect_url: "".to_string(),
                        headers_size: -1,
                        body_size: -1,
                        comment: None,
                        headers_compression: None,
                    },
                    cache: v1_3::Cache {
                        before_request: None,
                        after_request: None,
                    },
                    timings: v1_3::Timings {
                        blocked: None,
                        dns: None,
                        connect: None,
                        send: -1,
                        wait: -1,
                        receive: -1,
                        ssl: None,
                        comment: None,
                    },
                    // TODO - infer from url?
                    server_ip_address: None,
                    connection: None,
                    comment: None,
                };
                for element in record.into_inner() {
                    match element.as_rule() {
                        Rule::method => entry.request.method = element.as_str().to_string(),
                        Rule::url => entry.request.url = element.as_str().to_string(),
                        Rule::request_headers | Rule::curl_headers => {
                            for header in (ParseHeaders {
                                iter: element.into_inner(),
                            }) {
                                entry.request.headers.push(header)
                            }
                        }
                        // TODO: parse this with more detail
                        Rule::response_status => {
                            //println!("{:?}", element)
                        }
                        Rule::response_headers => {
                            for header in (ParseHeaders {
                                iter: element.into_inner(),
                            }) {
                                entry.response.headers.push(header)
                            }
                        }
                        Rule::response_body => {
                            for pair in element.into_inner() {
                                match pair.as_rule() {
                                    Rule::prelude => {}
                                    Rule::body => {
                                        entry.response.content.text =
                                            Some(base64::encode(pair.as_str()));
                                        entry.response.content.encoding =
                                            Some("base64".to_string());
                                    }
                                    e => Err(format_err!("Unexpected response body pair {:?}", e))?,
                                }
                            }
                        }
                        Rule::prelude => {}
                        e => Err(format_err!("Unexpected parse rule 2 encountered {:?}", e))?,
                    }
                }
                entries.push(entry);
            }
            Rule::EOI => (),
            e => Err(format_err!("Unexpected parse rule 1 encountered {:?}", e))?,
        }
    }
    let har_log = har::Har {
        log: har::Spec::V1_3(v1_3::Log {
            browser: None,
            creator: v1_3::Creator {
                name: "kubernetes-rs".to_string(),
                version: "0".to_string(),
                comment: None,
            },
            pages: None,
            entries: entries,
            comment: None,
        }),
    };
    result.push_str(&format!("{}\n", serde_json::to_string_pretty(&har_log)?));
    Ok(result)
}

fn main_() -> Result<(), Error> {
    let creator = v1_3::Creator {
        name: "kubernetes-rs".to_string(),
        version: "0.2".to_string(),
        comment: None,
    };
    let entries: Vec<v1_3::Entries> = Vec::new();
    let _har = v1_3::Log {
        browser: None,
        comment: None,
        creator: creator,
        entries: entries,
        pages: None,
    };
    for arg in env::args().skip(1) {
        // TODO: support stdin
        let input = fs::read_to_string(arg)?;
        let output = parse_log(&input)?;
        print!("{}", output);
    }
    Ok(())
}

pub fn main_result<F>(main_: F)
where
    F: FnOnce() -> Result<(), Error>,
{
    pretty_env_logger::init();
    let status = match main_() {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("Error: {}", e);
            for c in e.iter_chain().skip(1) {
                eprintln!(" Caused by {}", c);
            }
            eprintln!("{}", e.backtrace());
            1
        }
    };
    ::std::process::exit(status);
}

/// Collects all the given inputs as kubectl -v8 web transactions and outputs
/// as one har.
/// `kubectl version -v8 2> version.log && log2har version.log > version.har`
fn main() {
    main_result(main_)
}

#[cfg(test)]
mod test {
    #[test]
    fn parse_v8() -> Result<(), super::Error> {
        let input = r#"I0225 20:16:03.123108   14148 round_trippers.go:383] GET https://localhost:6445/version?timeout=32s
I0225 20:16:03.123108   14148 round_trippers.go:390] Request Headers:
I0225 20:16:03.123108   14148 round_trippers.go:393]     Accept: application/json, */*
I0225 20:16:03.123108   14148 round_trippers.go:393]     User-Agent: kubectl/v1.13.0 (windows/amd64) kubernetes/ddf47ac
I0225 20:16:03.146122   14148 round_trippers.go:408] Response Status: 200 OK in 23 milliseconds
I0225 20:16:03.146122   14148 round_trippers.go:411] Response Headers:
I0225 20:16:03.146122   14148 round_trippers.go:414]     Content-Type: application/json
I0225 20:16:03.146122   14148 round_trippers.go:414]     Content-Length: 263
I0225 20:16:03.146122   14148 round_trippers.go:414]     Date: Thu, 21 Feb 2019 21:04:01 GMT
I0225 20:16:03.147299   14148 request.go:942] Response Body: {
  "major": "1",
  "minor": "13",
  "gitVersion": "v1.13.0",
  "gitCommit": "ddf47ac13c1a9483ea035a79cd7c10005ff21a6d",
  "gitTreeState": "clean",
  "buildDate": "2018-12-03T20:56:12Z",
  "goVersion": "go1.11.2",
  "compiler": "gc",
  "platform": "linux/amd64"
}
"#;
        let expected = r#"{
  "log": {
    "version": "1.3",
    "creator": {
      "name": "kubernetes-rs",
      "version": "0"
    },
    "browser": null,
    "pages": null,
    "entries": [
      {
        "pageref": null,
        "startedDateTime": "",
        "time": 0,
        "request": {
          "method": "GET",
          "url": "https://localhost:6445/version?timeout=32s",
          "httpVersion": "unknown",
          "cookies": [],
          "headers": [
            {
              "name": "Accept",
              "value": "application/json, */*"
            },
            {
              "name": "User-Agent",
              "value": "kubectl/v1.13.0 (windows/amd64) kubernetes/ddf47ac"
            }
          ],
          "queryString": [],
          "headersSize": -1,
          "bodySize": -1
        },
        "response": {
          "status": -1,
          "statusText": "",
          "httpVersion": "unknown",
          "cookies": [],
          "headers": [
            {
              "name": "Content-Type",
              "value": "application/json"
            },
            {
              "name": "Content-Length",
              "value": "263"
            },
            {
              "name": "Date",
              "value": "Thu, 21 Feb 2019 21:04:01 GMT"
            }
          ],
          "content": {
            "size": -1,
            "mimeType": "",
            "text": "ewogICJtYWpvciI6ICIxIiwKICAibWlub3IiOiAiMTMiLAogICJnaXRWZXJzaW9uIjogInYxLjEzLjAiLAogICJnaXRDb21taXQiOiAiZGRmNDdhYzEzYzFhOTQ4M2VhMDM1YTc5Y2Q3YzEwMDA1ZmYyMWE2ZCIsCiAgImdpdFRyZWVTdGF0ZSI6ICJjbGVhbiIsCiAgImJ1aWxkRGF0ZSI6ICIyMDE4LTEyLTAzVDIwOjU2OjEyWiIsCiAgImdvVmVyc2lvbiI6ICJnbzEuMTEuMiIsCiAgImNvbXBpbGVyIjogImdjIiwKICAicGxhdGZvcm0iOiAibGludXgvYW1kNjQiCn0K",
            "encoding": "base64"
          },
          "redirectURL": "",
          "headersSize": -1,
          "bodySize": -1
        },
        "cache": {},
        "timings": {
          "send": -1,
          "wait": -1,
          "receive": -1
        }
      }
    ]
  }
}
"#;
        assert_eq!(&expected, &super::parse_log(&input)?);
        Ok(())
    }

    #[test]
    fn parse_v10() -> Result<(), super::Error> {
        let input = r#"I0219 14:38:39.123370   13292 round_trippers.go:383] curl -k -v -XGET  -H "User-Agent: kubectl/v1.13.0 (windows/amd64) kubernetes/ddf47ac" -H "Accept: application/json;as=Table;v=v1beta1;g=meta.k8s.io, application/json" 'https://localhost:6445/api/v1/namespaces?limit=500'
I0219 14:38:39.151342   13292 round_trippers.go:408] Response Status: 200 OK in 27 milliseconds
I0219 14:38:39.151342   13292 round_trippers.go:411] Response Headers:
I0219 14:38:39.151342   13292 round_trippers.go:414]     Content-Type: application/json
I0219 14:38:39.152341   13292 round_trippers.go:414]     Content-Length: 263
I0219 14:38:39.152341   13292 round_trippers.go:414]     Date: Sun, 17 Feb 2019 15:21:36 GMT
I0219 14:38:39.153335   13292 request.go:942] Response Body: {
  "major": "1",
  "minor": "13",
  "gitVersion": "v1.13.0",
  "gitCommit": "ddf47ac13c1a9483ea035a79cd7c10005ff21a6d",
  "gitTreeState": "clean",
  "buildDate": "2018-12-03T20:56:12Z",
  "goVersion": "go1.11.2",
  "compiler": "gc",
  "platform": "linux/amd64"
}
"#;
        let expected = r#"{
  "log": {
    "version": "1.3",
    "creator": {
      "name": "kubernetes-rs",
      "version": "0"
    },
    "browser": null,
    "pages": null,
    "entries": [
      {
        "pageref": null,
        "startedDateTime": "",
        "time": 0,
        "request": {
          "method": "GET",
          "url": "https://localhost:6445/api/v1/namespaces?limit=500",
          "httpVersion": "unknown",
          "cookies": [],
          "headers": [
            {
              "name": "User-Agent",
              "value": "kubectl/v1.13.0 (windows/amd64) kubernetes/ddf47ac"
            },
            {
              "name": "Accept",
              "value": "application/json;as=Table;v=v1beta1;g=meta.k8s.io, application/json"
            }
          ],
          "queryString": [],
          "headersSize": -1,
          "bodySize": -1
        },
        "response": {
          "status": -1,
          "statusText": "",
          "httpVersion": "unknown",
          "cookies": [],
          "headers": [
            {
              "name": "Content-Type",
              "value": "application/json"
            },
            {
              "name": "Content-Length",
              "value": "263"
            },
            {
              "name": "Date",
              "value": "Sun, 17 Feb 2019 15:21:36 GMT"
            }
          ],
          "content": {
            "size": -1,
            "mimeType": "",
            "text": "ewogICJtYWpvciI6ICIxIiwKICAibWlub3IiOiAiMTMiLAogICJnaXRWZXJzaW9uIjogInYxLjEzLjAiLAogICJnaXRDb21taXQiOiAiZGRmNDdhYzEzYzFhOTQ4M2VhMDM1YTc5Y2Q3YzEwMDA1ZmYyMWE2ZCIsCiAgImdpdFRyZWVTdGF0ZSI6ICJjbGVhbiIsCiAgImJ1aWxkRGF0ZSI6ICIyMDE4LTEyLTAzVDIwOjU2OjEyWiIsCiAgImdvVmVyc2lvbiI6ICJnbzEuMTEuMiIsCiAgImNvbXBpbGVyIjogImdjIiwKICAicGxhdGZvcm0iOiAibGludXgvYW1kNjQiCn0K",
            "encoding": "base64"
          },
          "redirectURL": "",
          "headersSize": -1,
          "bodySize": -1
        },
        "cache": {},
        "timings": {
          "send": -1,
          "wait": -1,
          "receive": -1
        }
      }
    ]
  }
}
"#;
        assert_eq!(&expected, &super::parse_log(&input)?);
        Ok(())
    }
}
