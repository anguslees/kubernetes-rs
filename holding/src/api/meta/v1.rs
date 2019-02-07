use api::{Integer, Time, TypeMeta, TypeMetaStruct};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt;
use std::sync::Once;
use List;

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMeta {
    pub cluster_name: Option<String>,
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub creation_timestamp: Option<Time>,
    pub deletion_grace_period_seconds: Option<Integer>,
    pub deletion_timestamp: Option<Time>,
    #[serde(default)]
    pub finalizers: Vec<String>,
    pub generate_name: Option<String>,
    pub generation: Option<Integer>,
    pub initializers: Option<Initializers>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub owner_references: Vec<OwnerReference>,
    pub resource_version: Option<String>,
    pub self_link: Option<String>,
    pub uid: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListMeta {
    #[serde(rename = "continue")]
    pub continu: Option<String>,
    #[serde(default)]
    pub resource_version: String,
    #[serde(default)]
    pub self_link: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Initializers {
    #[serde(default)]
    pub pending: Vec<Initializer>,
    pub result: Option<Status>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Initializer {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    //pub api_version: String,
    //pub kind: String,
    pub metadata: ListMeta,
    pub code: Integer,
    pub details: Option<StatusDetails>,
    pub message: String,
    pub reason: Option<StatusReason>,
    pub status: StatusStatus,
}

impl StdError for Status {
    fn description(&self) -> &str {
        if self.message != "" {
            &self.message
        } else {
            "request failed"
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref reason) = self.reason {
            write!(f, "{:?}", reason)?;
        } else {
            write!(f, "{:?}", self.status)?;
        }

        if self.message != "" {
            write!(f, ": {}", self.message)?;
        }
        if let Some(ref d) = self.details {
            for cause in &d.causes {
                match (&cause.message, &cause.reason) {
                    (&Some(ref msg), _) => write!(f, ", caused by {}", msg)?,
                    (&None, &Some(ref reason)) => write!(f, ", caused by {:?}", reason)?,
                    (&None, &None) => (),
                }
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum StatusStatus {
    Success,
    Failure,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum StatusReason {
    Unauthorized,
    Forbidden,
    NotFound,
    AlreadyExists,
    Conflict,
    Gone,
    Invalid,
    ServerTimeout,
    Timeout,
    TooManyRequests,
    BadRequest,
    MethodNotAllowed,
    InternalError,
    Expired,
    ServiceUnavailable,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusDetails {
    #[serde(default)]
    pub causes: Vec<StatusCause>,
    pub group: Option<String>,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub retry_after_seconds: Option<Integer>,
    pub uid: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusCause {
    pub field: String,
    pub message: Option<String>,
    pub reason: Option<CauseType>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum CauseType {
    FieldValueNotFound,
    FieldValueRequired,
    FieldValueDuplicate,
    FieldValueInvalid,
    FieldValueNotSupported,
    UnexpectedServerResponse,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OwnerReference {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub uid: String,
    #[serde(default)]
    pub block_owner_deletion: bool,
    #[serde(default)]
    pub controller: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelSelector {
    #[serde(default)]
    pub match_expressions: Vec<LabelSelectorRequirement>,
    #[serde(default)]
    pub match_labels: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelSelectorRequirement {
    pub key: String,
    pub operator: LabelSelectorOperator,
    pub values: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum LabelSelectorOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct APIResourceList {
    pub group_version: String,
    pub resources: Vec<APIResource>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct APIResource {
    pub name: String,
    pub singular_name: String,
    pub namespaced: bool,
    pub group: Option<String>,
    pub version: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub verbs: Vec<String>,
    #[serde(default)]
    pub short_names: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct APIGroupList {
    pub groups: Vec<APIGroup>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct APIGroup {
    pub name: String,
    pub versions: GroupVersionForDiscovery,
    pub preferred_version: Option<GroupVersionForDiscovery>,
    #[serde(rename = "serverAddressByClientCIDRs")]
    pub server_address_by_client_cidrs: Vec<ServerAddressByClientCIDR>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroupVersionForDiscovery {
    pub group_version: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerAddressByClientCIDR {
    #[serde(rename = "clientCIDR")]
    pub client_cidr: String,
    pub server_address: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchEvent {
    #[serde(rename = "type")]
    pub typ: EventType,
    pub object: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Added,
    Modified,
    Deleted,
    Error,
}

/// Not part of the standard k8s API
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemList<T>
where
    T: TypeMeta,
{
    #[serde(flatten)]
    typemeta: TypeMetaStruct<ItemList<T>>,
    pub metadata: ListMeta,
    #[serde(default)]
    pub items: Vec<T>,
}

impl<T> TypeMeta for ItemList<T>
where
    T: TypeMeta,
{
    fn api_version() -> &'static str {
        T::api_version()
    }
    fn kind() -> &'static str {
        // Rust can't concatenate two string constants into another
        // constant at compile time (yet), so build a static string on
        // first call at runtime.  TODO: Re-evaluate once constant
        // functions are a thing.
        static mut KIND: Option<String> = None;
        static INIT: Once = ::std::sync::ONCE_INIT;
        let k: &Option<String> = unsafe {
            INIT.call_once(|| {
                KIND = Some(format!("{}List", T::kind()));
            });
            &KIND
        };
        k.as_ref().unwrap()
    }
}

impl<T> List<T> for ItemList<T>
where
    T: TypeMeta,
{
    fn listmeta(&self) -> Cow<ListMeta> {
        Cow::Borrowed(&self.metadata)
    }
    fn items(&self) -> &[T] {
        &self.items
    }
    fn items_mut(&mut self) -> &mut [T] {
        &mut self.items
    }
    fn into_items(self) -> Vec<T> {
        self.items
    }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == Default::default()
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GetOptions {
    #[serde(skip_serializing_if = "is_default")]
    pub pretty: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub resource_version: String,
    #[serde(skip_serializing_if = "is_default")]
    pub include_uninitialized: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Preconditions {
    #[serde(default)]
    pub uid: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DeletionPropagation {
    Orphan,
    Background,
    Foreground,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct DeleteOptions {
    #[serde(skip_serializing_if = "is_default")]
    pub grace_period_seconds: Option<u64>,
    #[serde(skip_serializing_if = "is_default", default)]
    pub preconditions: Vec<Preconditions>,
    #[serde(skip_serializing_if = "is_default")]
    pub orphan_dependents: Option<bool>,
    #[serde(skip_serializing_if = "is_default")]
    pub propagation_policy: Option<DeletionPropagation>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ListOptions {
    #[serde(skip_serializing_if = "is_default")]
    pub resource_version: String, // Vec<u8>
    #[serde(skip_serializing_if = "is_default")]
    pub timeout_seconds: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub watch: bool, // NB: set explicitly by watch()
    #[serde(skip_serializing_if = "is_default")]
    pub pretty: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub field_selector: String,
    #[serde(skip_serializing_if = "is_default")]
    pub label_selector: String,
    #[serde(skip_serializing_if = "is_default")]
    pub include_uninitialized: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub limit: u32,
    #[serde(skip_serializing_if = "is_default", rename = "continue")]
    pub continu: String, // Vec<u8>
}
