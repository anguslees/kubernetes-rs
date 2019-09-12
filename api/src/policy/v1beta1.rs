use kubernetes_apimachinery::meta::v1::{DeleteOptions, Metadata, ObjectMeta};
use kubernetes_apimachinery::meta::{GroupVersion, TypeMetaImpl};
use serde::{Deserialize, Serialize};

const API_GROUP: &str = "policy/v1beta1";
pub const GROUP_VERSION: GroupVersion = GroupVersion {
    group: "policy",
    version: "v1beta1",
};

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Metadata)]
#[serde(rename_all = "camelCase")]
pub struct Eviction {
    #[serde(flatten)]
    typemeta: TypeMetaImpl<Eviction>,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub delete_options: DeleteOptions,
}
