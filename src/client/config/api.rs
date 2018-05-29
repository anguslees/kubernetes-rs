use std::collections::BTreeMap;
use std::path::PathBuf;
use serde_json::{Map,Value};
use serde_base64;

// See k8s.io/client-go/tools/clientcmd/api/types.go:Config

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct NameClusterPair {
    pub name: String,
    pub cluster: Cluster,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct NameUserPair {
    pub name: String,
    pub user: AuthInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct NameContextPair {
    pub name: String,
    pub context: Context,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct Config {
    #[serde(rename="apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(default)]
    pub clusters: Vec<NameClusterPair>,
    #[serde(default)]
    pub users: Vec<NameUserPair>,
    #[serde(default)]
    pub contexts: Vec<NameContextPair>,
    #[serde(default)]
    pub current_context: String,
    #[serde(default)]
    pub extensions: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct Preferences {
    #[serde(default)]
    pub colors: bool,
    #[serde(default)]
    pub extensions: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct Cluster {
    pub server: String,
    #[serde(default)]
    pub insecure_skip_tls_verify: bool,
    #[serde(default)]
    pub certificate_authority: PathBuf,
    #[serde(default,with="serde_base64")]
    pub certificate_authority_data: Vec<u8>,
    #[serde(default)]
    pub extensions: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct AuthInfo {
    #[serde(default)]
    pub client_certificate: PathBuf,
    #[serde(default,with="serde_base64")]
    pub client_certificate_data: Vec<u8>,
    #[serde(default)]
    pub client_key: PathBuf,
    #[serde(default,with="serde_base64")]
    pub client_key_data: Vec<u8>,
    #[serde(default)]
    pub token: String,
    #[serde(default,rename="tokenFile")]
    pub token_file: PathBuf,
    #[serde(default)]
    pub act_as: String,
    #[serde(default)]
    pub act_as_groups: Vec<String>,
    #[serde(default)]
    pub act_as_user_extra: BTreeMap<String,Vec<String>>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub auth_provider: Option<AuthProviderConfig>,
    #[serde(default)]
    pub extensions: Map<String,Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct Context {
    pub cluster: String,
    pub user: String,
    pub namespace: Option<String>,
    #[serde(default)]
    pub extensions: Map<String,Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all="kebab-case")]
pub struct AuthProviderConfig {
    pub name: String,
    #[serde(default)]
    pub config: BTreeMap<String, String>,
}
