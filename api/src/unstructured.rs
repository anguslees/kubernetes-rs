use crate::meta::v1::{List, ListMeta, Metadata, ObjectMeta};
use serde_json::{self, Value};
use std::borrow::Cow;

impl Metadata for Value {
    fn kind(&self) -> &str {
        self["kind"].as_str().unwrap_or_default()
    }

    fn api_version(&self) -> &str {
        self["apiVersion"].as_str().unwrap_or_default()
    }

    fn metadata(&self) -> Cow<ObjectMeta> {
        serde_json::from_value(self["metadata"].clone()).unwrap_or_default()
    }
}

impl List for Value {
    type Item = Value;

    fn listmeta(&self) -> Cow<ListMeta> {
        serde_json::from_value(self["metadata"].clone()).unwrap_or_default()
    }

    fn items(&self) -> &[Value] {
        static EMPTY: [Value; 0] = [];
        self["items"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&EMPTY)
    }

    fn items_mut(&mut self) -> &mut [Value] {
        let vec = match *self {
            Value::Array(ref mut v) => v,
            _ => {
                self["items"] = Value::Array(vec![]);
                self["items"].as_array_mut().unwrap()
            }
        };
        vec.as_mut_slice()
    }

    fn into_items(mut self) -> Vec<Value> {
        match self["items"].take() {
            Value::Array(v) => v,
            _ => vec![],
        }
    }
}
