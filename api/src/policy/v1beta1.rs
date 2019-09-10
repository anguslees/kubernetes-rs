use kubernetes_apimachinery::meta::v1::{DeleteOptions, Metadata, ObjectMeta};
use kubernetes_apimachinery::meta::{GroupVersion, TypeMeta, TypeMetaImpl};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

const API_GROUP: &str = "policy/v1beta1";
pub const GROUP_VERSION: GroupVersion = GroupVersion {
    group: "policy",
    version: "v1beta1",
};

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Eviction {
    #[serde(flatten)]
    typemeta: TypeMetaImpl<Eviction>,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub delete_options: DeleteOptions,
}

impl TypeMeta for Eviction {
    fn api_version() -> &'static str {
        API_GROUP
    }
    fn kind() -> &'static str {
        "Eviction"
    }
}

impl Metadata for Eviction {
    fn api_version(&self) -> &str {
        <Eviction as TypeMeta>::api_version()
    }
    fn kind(&self) -> &str {
        <Eviction as TypeMeta>::kind()
    }
    fn metadata(&self) -> Cow<ObjectMeta> {
        Cow::Borrowed(&self.metadata)
    }
}
