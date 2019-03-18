use failure::{err_msg, Error};
use har;
use serde_json::Value;

use ::kubernetes_api::core::v1::NamespaceList;

fn entries(ahar: &har::Har) -> Result<&Vec<har::v1_3::Entries>, Error> {
    match &ahar.log {
        har::Spec::V1_3(log) => Ok(&log.entries),
        _ => Err(err_msg("unexpected har version")),
    }
}

fn response(entry: &har::v1_3::Entries) -> Result<String, Error> {
    // https://github.com/rust-lang/rust/issues/46871
    // let body = &entry.response.content.text?;
    let body = entry
        .response
        .content
        .text
        .as_ref()
        .ok_or(err_msg("no response body"))?;
    Ok(match &entry.response.content.encoding {
        None => body.clone(),
        Some(enc) if enc == "base64" => String::from_utf8(base64::decode(&body)?)?,
        Some(enc) => Err(err_msg(format!("unknown encoding {}", enc)))?,
    })
}

#[test]
fn test_namespace_har() -> Result<(), Error> {
    // This is not generalised. We should aim to make this model driven and fully generalised, but EFUTURE.
    // Load the HAR
    // HAR errors are awkwardly incompatibly... feel free to whinge,
    // but even map_err was throwing from::From failures - I think due to har's Result wrapper.
    let har = match har::from_path("testdata/traces/json/get-namespaces.har") {
        Ok(something) => Ok(something),
        Err(e) => Err(err_msg(format!("{}", e))),
    }?;
    for entry in entries(&har)? {
        println!("url {:?}", entry.request.url);
        // XXX todos:
        // Check that the GVK url route is correct - route that URL and match back to the GVK or vice versa.
        // cross check the self link?
        // its a get, no payload upload verification
        // verify the GET options from the URL are understood - do we have a parser for those ?

        // parse the response by its expected type
        let response_str = response(entry)?;
        println!("response {}", response_str);
        let typed: NamespaceList = serde_json::from_str(&response_str)?;
        println!("list {:?}", typed);
        println!("list-to-json {}", serde_json::to_string_pretty(&typed)?);
        // reserialise the response to JSON, bounce it back through Value to eliminate the *ordering* in the output imposed by the use of structured types.
        let typed_json = serde_json::to_string_pretty(&serde_json::from_str::<Value>(
            &serde_json::to_string_pretty(&typed)?,
        )?)?;
        // let typed_interim: Value = serde_json::from_str(&typed_j)
        // compare the resulting JSON
        // canonicalise the reference JSON to avoid over-time library output variations.
        let interim: Value = serde_json::from_str(&response_str)?;
        let canonical = serde_json::to_string_pretty(&interim)?;
        println!("{}", canonical);
        println!("{}", typed_json);
        // XXX: should be eq, but need to:
        // - fix Pods kind in output
        // - remove extra apiVersion and kind etc
        // - implement omitEmpty
        // XXX or alternatively have a different round trip verification approach
        assert_ne!(canonical, typed_json);
        // compare equal.
    }
    Ok(())
}
