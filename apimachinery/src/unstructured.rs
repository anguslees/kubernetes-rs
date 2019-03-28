use crate::meta::v1::{List, ListMeta, Metadata, ObjectMeta};
use crate::meta::{ClusterScope, GroupVersionResource, NamespaceScope, Resource, ResourceScope};
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

pub enum DynamicScope {
    Cluster(ClusterScope),
    Namespace(NamespaceScope),
}

impl ResourceScope for DynamicScope {
    fn url_segments(&self) -> Vec<&str> {
        match self {
            Self::Cluster(n) => n.url_segments(),
            Self::Namespace(n) => n.url_segments(),
        }
    }
    fn name(&self) -> Option<&str> {
        match self {
            Self::Cluster(n) => n.name(),
            Self::Namespace(n) => n.name(),
        }
    }
    fn namespace(&self) -> Option<&str> {
        match self {
            Self::Cluster(n) => n.namespace(),
            Self::Namespace(n) => n.namespace(),
        }
    }
}

pub struct DynamicResource {
    pub group: String,
    pub version: String,
    pub singular: String,
    pub plural: String,
}

impl Resource for &DynamicResource {
    type Item = Value;
    type List = Value;
    type Scope = DynamicScope;

    fn gvr(&self) -> GroupVersionResource {
        GroupVersionResource {
            group: &self.group,
            version: &self.version,
            resource: &self.plural,
        }
    }

    fn singular(&self) -> String {
        self.singular.clone()
    }

    fn plural(&self) -> String {
        self.plural.clone()
    }
}
