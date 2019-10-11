use crate::policy::v1beta1::Eviction;
use async_trait::async_trait;
use failure::Error;
use futures::io::{AsyncRead, AsyncWrite};
use http::Method;
use kubernetes_apimachinery::client::{ApiClient, ResourceClient};
use kubernetes_apimachinery::meta::v1::{
    CreateOptions, ItemList, LabelSelector, Metadata, ObjectMeta,
};
use kubernetes_apimachinery::meta::{
    GroupVersion, GroupVersionResource, IntOrString, Integer, NamespaceScope, Quantity, Resource,
    Time, TypeMetaImpl,
};
use kubernetes_apimachinery::request::{Request, APPLICATION_JSON};
use kubernetes_apimachinery::{ApiService, HttpService, HttpUpgradeService};
use serde::{Deserialize, Serialize};
use serde_json::{self, Map, Value};
use std::default::Default;

// TODO(gus): Uses of serde_json::{Map,Value} below are probably incorrect.

const API_GROUP: &str = "v1";
pub const GROUP_VERSION: GroupVersion = GroupVersion {
    group: "",
    version: "v1",
};

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == Default::default()
}

pub struct Pods;
impl Pods {
    pub fn gvr() -> GroupVersionResource<'static> {
        GROUP_VERSION.with_resource("pods")
    }
}

impl Resource for Pods {
    type Item = Pod;
    type Scope = NamespaceScope;
    type List = PodList;
    fn gvr(&self) -> GroupVersionResource {
        Self::gvr()
    }
    fn singular(&self) -> String {
        "pod".to_string()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PodLogOptions {
    #[serde(skip_serializing_if = "is_default")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub follow: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub previous: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub since_seconds: Option<i64>,
    #[serde(skip_serializing_if = "is_default")]
    pub since_time: Option<Time>,
    #[serde(skip_serializing_if = "is_default")]
    pub timestamps: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub tail_lines: Option<i64>,
    #[serde(skip_serializing_if = "is_default")]
    pub limit_bytes: Option<i64>,
}

fn booltrue() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PodAttachOptions {
    #[serde(skip_serializing_if = "is_default")]
    pub stdin: bool,
    #[serde(skip_serializing_if = "is_default", default = "booltrue")]
    pub stdout: bool,
    #[serde(skip_serializing_if = "is_default", default = "booltrue")]
    pub stderr: bool,
    #[serde(skip_serializing_if = "is_default", rename = "TTY")]
    pub tty: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub container: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PodExecOptions {
    #[serde(skip_serializing_if = "is_default")]
    pub stdin: bool,
    #[serde(skip_serializing_if = "is_default", default = "booltrue")]
    pub stdout: bool,
    #[serde(skip_serializing_if = "is_default", default = "booltrue")]
    pub stderr: bool,
    #[serde(skip_serializing_if = "is_default", rename = "TTY")]
    pub tty: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub command: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PodPortForwardOptions {
    #[serde(skip_serializing_if = "is_default")]
    pub ports: Vec<i32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PodProxyOptions {
    #[serde(skip_serializing_if = "is_default")]
    pub path: String,
}

/// Trait adding extra methods to the regular Pods ResourceService.
/// These are unusually exciting and may not be implemented for your
/// particular client/server.
#[async_trait]
pub trait PodsServiceExt {
    type Read: AsyncRead;

    async fn read_log(
        &self,
        name: &<Pods as Resource>::Scope,
        opts: PodLogOptions,
    ) -> Result<Self::Read, Error>;

    async fn connect_portforward(
        &self,
        name: &<Pods as Resource>::Scope,
        opts: PodPortForwardOptions,
    ) -> Result<(), Error>;

    async fn proxy<I, O>(
        &self,
        name: &<Pods as Resource>::Scope,
        req: http::Request<I>,
        opts: PodProxyOptions,
    ) -> Result<http::Response<O>, Error>
    where
        I: Send;

    async fn attach<Stdin>(
        &self,
        name: &<Pods as Resource>::Scope,
        stdin: Option<Stdin>,
        opts: PodAttachOptions,
    ) -> Result<(Self::Read, Self::Read), Error>
    where
        Stdin: AsyncWrite + Send;

    // TODO: Needs to also return Future<exit status>, and sigwinch.
    // (basically the return should look more like std::process::Child)
    async fn exec<Stdin>(
        &self,
        name: &<Pods as Resource>::Scope,
        stdin: Option<Stdin>,
        opts: PodExecOptions,
    ) -> Result<(Self::Read, Self::Read), Error>
    where
        Stdin: AsyncWrite + Send;

    async fn create_eviction(
        &self,
        name: &<Pods as Resource>::Scope,
        value: Eviction,
        opts: CreateOptions,
    ) -> Result<Eviction, Error>;
}

#[async_trait]
impl<C> PodsServiceExt for ResourceClient<ApiClient<C>, Pods>
where
    C: HttpService + HttpUpgradeService + Send + Sync,
    ApiClient<C>: Clone,
{
    type Read = <C as HttpUpgradeService>::Upgraded;

    async fn read_log(
        &self,
        name: &<Pods as Resource>::Scope,
        opts: PodLogOptions,
    ) -> Result<Self::Read, Error> {
        let req = Request::builder(Pods::gvr())
            .scope(name)
            .subresource("logs")
            .method(Method::GET)
            .opts(opts)
            .build();

        let r = req.into_http_request(self.api_client().base_url())?;

        let upgraded = self
            .api_client()
            .http_client()
            .upgrade(r.map(|_| ()))
            .await?;
        Ok(upgraded)
    }

    async fn connect_portforward(
        &self,
        _name: &<Pods as Resource>::Scope,
        _opts: PodPortForwardOptions,
    ) -> Result<(), Error> {
        unimplemented!();
    }

    async fn proxy<I, O>(
        &self,
        _name: &<Pods as Resource>::Scope,
        _req: http::Request<I>,
        _opts: PodProxyOptions,
    ) -> Result<http::Response<O>, Error>
    where
        I: Send,
    {
        unimplemented!();
    }

    async fn attach<Stdin>(
        &self,
        _name: &<Pods as Resource>::Scope,
        _stdin: Option<Stdin>,
        _opts: PodAttachOptions,
    ) -> Result<(Self::Read, Self::Read), Error>
    where
        Stdin: AsyncWrite + Send,
    {
        unimplemented!();
    }

    // TODO: Needs to also return Future<exit status>, and sigwinch.
    async fn exec<Stdin>(
        &self,
        _name: &<Pods as Resource>::Scope,
        _stdin: Option<Stdin>,
        _opts: PodExecOptions,
    ) -> Result<(Self::Read, Self::Read), Error>
    where
        Stdin: AsyncWrite + Send,
    {
        unimplemented!();
    }

    async fn create_eviction(
        &self,
        name: &<Pods as Resource>::Scope,
        value: Eviction,
        opts: CreateOptions,
    ) -> Result<Eviction, Error> {
        let req = Request::builder(Pods::gvr())
            .scope(name)
            .subresource("eviction")
            .method(Method::POST)
            .opts(opts)
            .body(APPLICATION_JSON, value)
            .build();

        let resp = self.api_client().request(req).await?;
        Ok(resp.into_body())
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Metadata)]
#[serde(rename_all = "camelCase")]
pub struct Namespace {
    #[serde(flatten)]
    typemeta: TypeMetaImpl<Namespace>,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub spec: NamespaceSpec,
    #[serde(default)]
    pub status: NamespaceStatus,
}

pub type NamespaceList = ItemList<Namespace>;

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceSpec {
    #[serde(default)]
    pub finalizers: Vec<String>,
}

pub const FINALIZER_KUBERNETES: &str = "kubernetes";

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceStatus {
    pub phase: Option<NamespacePhase>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum NamespacePhase {
    Active,
    Terminating,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Metadata)]
#[serde(rename_all = "camelCase")]
pub struct Pod {
    #[serde(flatten)]
    typemeta: TypeMetaImpl<Pod>,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub spec: PodSpec,
    #[serde(default)]
    pub status: PodStatus,
}

pub type PodList = ItemList<Pod>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodTemplateSpec {
    pub metadata: ObjectMeta,
    pub spec: PodSpec,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodSpec {
    pub active_deadline_seconds: Option<Integer>,
    pub affinity: Option<Affinity>,
    pub automount_service_account_token: Option<bool>,
    #[serde(default)]
    pub containers: Vec<Container>,
    #[serde(default = "clusterfirst")]
    pub dns_policy: DNSPolicy,
    #[serde(default)]
    pub host_aliases: Vec<HostAlias>,
    #[serde(default)]
    pub host_ipc: bool,
    #[serde(default)]
    pub host_network: bool,
    #[serde(default, rename = "hostPID")]
    pub host_pid: bool,
    pub hostname: Option<String>,
    #[serde(default)]
    pub image_pull_secrets: Vec<LocalObjectReference>,
    #[serde(default)]
    pub init_containers: Vec<Container>,
    pub node_name: Option<String>,
    #[serde(default)]
    pub node_selector: Map<String, Value>,
    pub priority: Option<Integer>,
    pub priority_class_name: Option<String>,
    #[serde(default = "always")]
    pub restart_policy: RestartPolicy,
    pub scheduler_name: Option<String>,
    pub security_context: Option<PodSecurityContext>,
    pub service_account: Option<String>,
    pub service_account_name: Option<String>,
    pub subdomain: Option<String>,
    #[serde(default = "int30")]
    pub termination_grace_period_seconds: Integer,
    #[serde(default)]
    pub tolerations: Vec<Toleration>,
    #[serde(default)]
    pub volumes: Vec<Volume>,
}

impl Default for PodSpec {
    fn default() -> Self {
        serde_json::from_value(Value::Object(Default::default())).unwrap()
    }
}

#[test]
fn podspec_default() {
    let _: PodSpec = Default::default();
}

fn clusterfirst() -> DNSPolicy {
    DNSPolicy::ClusterFirst
}
fn always() -> RestartPolicy {
    RestartPolicy::Always
}
fn int30() -> Integer {
    30
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum DNSPolicy {
    ClusterFirst,
    ClusterFirstWithHostNet,
    Default,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Affinity {
    pub node_affinity: Option<NodeAffinity>,
    pub pod_affinity: Option<PodAffinity>,
    pub pod_anti_affinity: Option<PodAntiAffinity>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeAffinity {
    #[serde(default)]
    pub preferred_during_scheduling_ignored_during_execution: Vec<PreferredSchedulingTerm>,
    pub required_during_scheduling_ignored_during_execution: Option<NodeSelector>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreferredSchedulingTerm {
    pub preference: Option<NodeSelectorTerm>,
    pub weight: Option<Integer>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSelector {
    pub node_selector_terms: Vec<NodeSelectorTerm>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSelectorTerm {
    #[serde(default)]
    pub match_expressions: Vec<NodeSelectorRequirement>,
    #[serde(default)]
    pub match_fields: Vec<NodeSelectorRequirement>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSelectorRequirement {
    pub key: String,
    pub operator: NodeSelectorOperator,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum NodeSelectorOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
    Gt,
    Lt,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodAffinity {
    pub preferred_during_scheduling_ignored_during_execution: Option<WeightedPodAffinityTerm>,
    #[serde(default)]
    pub required_during_scheduling_ignored_during_execution: Vec<PodAffinityTerm>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeightedPodAffinityTerm {
    pub pod_affinity_term: PodAffinityTerm,
    pub weight: Integer,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodAffinityTerm {
    pub label_selector: LabelSelector,
    #[serde(default)]
    pub namespaces: Vec<String>,
    pub topology_key: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodAntiAffinity {
    #[serde(default)]
    pub preferred_during_scheduling_ignored_during_execution: Vec<WeightedPodAffinityTerm>,
    #[serde(default)]
    pub required_during_scheduling_ignored_during_execution: Vec<PodAffinityTerm>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    pub env_from: Vec<EnvFromSource>,
    pub image: Option<String>,
    pub image_pull_policy: Option<PullPolicy>,
    pub lifecycle: Option<Lifecycle>,
    pub liveness_probe: Option<Probe>,
    pub readiness_probe: Option<Probe>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ports: Vec<ContainerPort>,
    pub resources: Option<ResourceRequirements>,
    pub security_context: Option<SecurityContext>,
    #[serde(default)]
    pub stdin: bool,
    #[serde(default)]
    pub stdin_once: bool,
    #[serde(default)]
    pub tty: bool,
    #[serde(default = "devterminationlog")]
    pub termination_message_path: String,
    #[serde(default = "file")]
    pub termination_message_policy: TerminationMessagePolicy,
    #[serde(default)]
    pub volume_mounts: Vec<VolumeMount>,
    pub working_dir: Option<String>,
}

impl Default for Container {
    fn default() -> Self {
        serde_json::from_value(Value::Object(Default::default())).unwrap()
    }
}

#[test]
fn container_default() {
    let _: Container = Default::default();
}

fn devterminationlog() -> String {
    "/dev/termination-log".into()
}
fn file() -> TerminationMessagePolicy {
    TerminationMessagePolicy::File
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum PullPolicy {
    Always,
    Never,
    IfNotPresent,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum TerminationMessagePolicy {
    File,
    FallbackToLogsOnError,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    #[serde(default)]
    pub value: String,
    pub value_from: Option<EnvVarSource>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum EnvVarSource {
    ConfigMapKeyRef(ConfigMapKeySelector),
    FieldRef(ObjectFieldSelector),
    ResourceFieldRef(ResourceFieldSelector),
    SecretKeyRef(SecretKeySelector),
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapKeySelector {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub optional: bool,
}

fn v1() -> String {
    "v1".into()
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectFieldSelector {
    #[serde(default = "v1")]
    pub api_version: String,
    pub field_path: String,
}

fn quant1() -> Quantity {
    "1".into()
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFieldSelector {
    pub container_name: Option<String>,
    #[serde(default = "quant1")]
    pub divisor: Quantity,
    pub resource: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretKeySelector {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvFromSource {
    pub prefix: Option<String>,
    pub config_map_ref: Option<ConfigMapEnvSource>,
    pub secret_ref: Option<SecretEnvSource>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapEnvSource {
    pub name: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretEnvSource {
    pub name: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lifecycle {
    pub post_start: Option<Handler>,
    pub pre_stop: Option<Handler>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Handler {
    Exec(ExecAction),
    HttpGet(HTTPGetAction),
    TcpSocket(TCPSocketAction),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecAction {
    pub command: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HTTPGetAction {
    pub host: Option<String>,
    #[serde(default)]
    pub http_headers: Vec<HTTPHeader>,
    pub path: String,
    pub port: IntOrString,
    #[serde(default = "http")]
    pub scheme: String,
}

fn http() -> String {
    "HTTP".into()
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HTTPHeader {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TCPSocketAction {
    pub host: Option<String>,
    pub port: IntOrString,
}

fn int1() -> Integer {
    1
}
fn int3() -> Integer {
    3
}
fn int10() -> Integer {
    10
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub exec: Option<ExecAction>,
    pub http_get: Option<HTTPGetAction>,
    pub tcp_socket: Option<TCPSocketAction>,
    #[serde(default = "int3")]
    pub failure_threshold: Integer,
    #[serde(default)]
    pub initial_delay_seconds: Integer,
    #[serde(default = "int10")]
    pub period_seconds: Integer,
    #[serde(default = "int1")]
    pub success_threshold: Integer,
    #[serde(default = "int1")]
    pub timeout_seconds: Integer,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerPort {
    pub container_port: Integer,
    #[serde(rename = "hostIP")]
    pub host_ip: Option<String>,
    pub host_port: Option<Integer>,
    pub name: Option<String>,
    #[serde(default = "tcp")]
    pub protocol: Protocol,
}

fn tcp() -> Protocol {
    Protocol::TCP
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Protocol {
    TCP,
    UDP,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRequirements {
    #[serde(default)]
    pub limits: Map<String, Value>,
    #[serde(default)]
    pub requests: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityContext {
    pub allow_privilege_escalation: Option<bool>,
    pub capabilities: Option<Capabilities>,
    #[serde(default)]
    pub privileged: bool,
    #[serde(default)]
    pub read_only_root_filesystem: bool,
    #[serde(default)]
    pub run_as_non_root: bool,
    pub run_as_user: Option<Integer>,
    #[serde(rename = "seLinuxOptions")]
    pub selinux_options: Option<SELinuxOptions>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub drop: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SELinuxOptions {
    pub level: String,
    pub role: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub user: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeMount {
    pub mount_path: String,
    pub mount_propagation: Option<String>,
    pub name: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub sub_path: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostAlias {
    pub hostnames: Vec<String>,
    pub ip: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalObjectReference {
    pub name: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodSecurityContext {
    pub fs_group: Option<Integer>,
    #[serde(default)]
    pub run_as_non_root: bool,
    pub run_as_user: Option<Integer>,
    #[serde(default)]
    pub supplemental_groups: Vec<Integer>,
    #[serde(rename = "seLinuxOptions")]
    pub selinux_options: Option<SELinuxOptions>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum TaintEffect {
    NoSchedule,
    PreferNoSchedule,
    NoExecute,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum TolerationOperator {
    Exists,
    Equal,
}

fn equal() -> TolerationOperator {
    TolerationOperator::Equal
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Toleration {
    pub effect: Option<TaintEffect>,
    pub key: Option<String>,
    #[serde(default = "equal")]
    pub operator: TolerationOperator,
    pub toleration_seconds: Option<Integer>,
    pub value: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub name: String,
    #[serde(flatten)]
    pub source: VolumeSource,
}

// This is not a real k8s type
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VolumeSource {
    //#[serde(rename="awsElasticBlockStore")]
    //AwsElasticBlockStore(AWSElasticBlockStoreVolumeSource),
    //AzureDisk(AzureDiskVolumeSource),
    //AzureFile(AzureFileVolumeSource),
    //#[serde(rename="cephfs")]
    //CephFS(CephFSVolumeSource),
    //Cinder(CinderVolumeSource),
    ConfigMap(ConfigMapVolumeSource),
    #[serde(rename = "downwardAPI")]
    DownwardAPI(DownwardAPIVolumeSource),
    EmptyDir(EmptyDirVolumeSource),
    //#[serde(rename="fc")]
    //FC(FCVolumeSource)
    //FlexVolume(FlexVolumeSource)
    //Flocker(FlockerVolumeSource)
    //#[serde(rename="gcePersistentDisk")]
    //GCEPersistentDisk(GCEPersistentDiskVolumeSource)
    //GitRepo(GitRepoVolumeSource)
    //Glusterfs(GlusterfsVolumeSource)
    HostPath(HostPathVolumeSource),
    //#[serde(rename="iscsi")]
    //ISCSI(ISCSIVolumeSource),
    #[serde(rename = "nfs")]
    NFS(NFSVolumeSource),
    PersistentVolumeClaim(PersistentVolumeClaimVolumeSource),
    //photonPersistentDisk
    //portworxVolume
    //projected
    //quobyte
    //rbd
    //scaleIO
    Secret(SecretVolumeSource),
    //storageos
    //vsphereVolume
}

fn int0644() -> Integer {
    0o644
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapVolumeSource {
    #[serde(default = "int0644")]
    pub default_mode: Integer,
    #[serde(default)]
    pub items: Vec<KeyToPath>,
    pub name: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyToPath {
    pub key: String,
    pub mode: Option<Integer>,
    pub path: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownwardAPIVolumeSource {
    #[serde(default = "int0644")]
    pub default_mode: Integer,
    #[serde(default)]
    pub items: Vec<DownwardAPIVolumeFile>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownwardAPIVolumeFile {
    pub field_ref: Option<ObjectFieldSelector>,
    pub mode: Option<Integer>,
    pub path: String,
    pub resource_field_ref: Option<ResourceFieldSelector>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmptyDirVolumeSource {
    #[serde(default)]
    pub medium: String,
    pub size_limit: Option<Quantity>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostPathVolumeSource {
    pub path: String,
    #[serde(default, rename = "type")]
    pub typ: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NFSVolumeSource {
    pub path: String,
    #[serde(default)]
    pub read_only: bool,
    pub server: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimVolumeSource {
    pub claim_name: String,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretVolumeSource {
    #[serde(default = "int0644")]
    pub default_mode: Integer,
    #[serde(default)]
    pub items: Vec<KeyToPath>,
    pub secret_name: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum ConditionStatus {
    True,
    False,
    Unknown,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodStatus {
    pub phase: Option<PodPhase>,
    #[serde(default)]
    pub conditions: Vec<PodCondition>,
    pub message: Option<String>,
    pub reason: Option<String>,
    #[serde(rename = "hostIP")]
    pub host_ip: Option<String>,
    #[serde(rename = "podIP")]
    pub pod_ip: Option<String>,
    pub start_time: Option<Time>,
    #[serde(default)]
    pub init_container_statuses: Vec<ContainerStatus>,
    #[serde(default)]
    pub container_statuses: Vec<ContainerStatus>,
    pub qos_class: Option<PodQOSClass>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum PodConditionType {
    ContainersReady,
    Initialized,
    PodScheduled,
    Ready,
    Unschedulable,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodCondition {
    #[serde(rename = "type")]
    pub typ: PodConditionType,
    pub status: ConditionStatus,
    pub last_probe_time: Option<Time>,
    pub last_transition_time: Option<Time>,
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatus {
    pub name: String,
    pub state: Option<ContainerState>,
    pub last_termination_state: Option<ContainerState>,
    pub ready: bool,
    pub restart_count: Integer,
    pub image: String,
    #[serde(rename = "imageID")]
    pub image_id: String,
    #[serde(rename = "containerID")]
    pub container_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum PodPhase {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum PodQOSClass {
    Guaranteed,
    Burstable,
    BestEffort,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ContainerState {
    Waiting(ContainerStateWaiting),
    Running(ContainerStateRunning),
    Terminated(ContainerStateTerminated),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateWaiting {
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateRunning {
    pub started_at: Option<Time>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateTerminated {
    pub exit_code: Integer,
    pub signal: Option<Integer>,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub started_at: Option<Time>,
    pub finished_at: Option<Time>,
    #[serde(rename = "container_ID")]
    pub container_id: Option<String>,
}

#[test]
fn deser_pod() {
    let yaml = r#"
      apiVersion: v1
      kind: Pod
      metadata:
        annotations:
          kubernetes.io/config.hash: 9d80efa9dfda66b126ec4f9b5f7a004f
          kubernetes.io/config.mirror: 9d80efa9dfda66b126ec4f9b5f7a004f
          kubernetes.io/config.seen: 2018-02-17T23:17:05.703130559Z
          kubernetes.io/config.source: file
          scheduler.alpha.kubernetes.io/critical-pod: ""
        creationTimestamp: 2018-02-17T23:21:30Z
        labels:
          component: etcd
          tier: control-plane
        name: etcd-minikube
        namespace: kube-system
        resourceVersion: "82834"
        selfLink: /api/v1/namespaces/kube-system/pods/etcd-minikube
        uid: 491e0972-1439-11e8-bdc8-525400cf4e41
      spec:
        containers:
        - command:
          - etcd
          - --listen-client-urls=http://127.0.0.1:2379
          - --advertise-client-urls=http://127.0.0.1:2379
          - --data-dir=/data
          image: gcr.io/google_containers/etcd-amd64:3.1.10
          imagePullPolicy: IfNotPresent
          livenessProbe:
            failureThreshold: 8
            httpGet:
              host: 127.0.0.1
              path: /health
              port: 2379
              scheme: HTTP
            initialDelaySeconds: 15
            periodSeconds: 10
            successThreshold: 1
            timeoutSeconds: 15
          name: etcd
          resources: {}
          terminationMessagePath: /dev/termination-log
          terminationMessagePolicy: File
          volumeMounts:
          - mountPath: /data
            name: etcd
        dnsPolicy: ClusterFirst
        hostNetwork: true
        nodeName: minikube
        restartPolicy: Always
        schedulerName: default-scheduler
        securityContext: {}
        terminationGracePeriodSeconds: 30
        tolerations:
        - effect: NoExecute
          operator: Exists
        volumes:
        - hostPath:
            path: /data
            type: DirectoryOrCreate
          name: etcd
      status:
        conditions:
        - lastProbeTime: null
          lastTransitionTime: 2018-02-20T18:00:06Z
          status: "True"
          type: Initialized
        - lastProbeTime: null
          lastTransitionTime: 2018-02-20T18:00:09Z
          status: "True"
          type: Ready
        - lastProbeTime: null
          lastTransitionTime: 2018-02-20T18:00:05Z
          status: "True"
          type: PodScheduled
        containerStatuses:
        - containerID: docker://f16f6af6da79cab1ea508dae8cf1b03dd19d8d1a724e20e0c9fff74c88081d78
          image: gcr.io/google_containers/etcd-amd64:3.1.10
          imageID: docker-pullable://gcr.io/google_containers/etcd-amd64@sha256:28cf78933de29fd26d7a879e51ebd39784cd98109568fd3da61b141257fb85a6
          lastState:
            terminated:
              containerID: docker://1eb66b2babd510dc4010b96c8503d8c267a787f824c79826c1ea00e3e09d87a4
              exitCode: 0
              finishedAt: 2018-02-20T07:03:14Z
              reason: Completed
              startedAt: 2018-02-19T23:51:16Z
          name: etcd
          ready: true
          restartCount: 3
          state:
            running:
              startedAt: 2018-02-20T18:00:07Z
        hostIP: 192.168.122.187
        phase: Running
        podIP: 192.168.122.187
        qosClass: BestEffort
        startTime: 2018-02-20T18:00:06Z
        "#;

    let pod: Pod = ::serde_yaml::from_str(yaml).unwrap();
    assert_eq!(pod.status.phase, Some(PodPhase::Running));
    assert_eq!(pod.spec.volumes.len(), 1);
    assert_eq!(pod.spec.volumes[0].name, "etcd");
    assert_eq!(
        pod.spec.volumes[0].source,
        VolumeSource::HostPath(HostPathVolumeSource {
            path: String::from("/data"),
            typ: String::from("DirectoryOrCreate"),
        })
    );

    // roundtrip
    let rt_json = ::serde_json::to_value(&pod).unwrap();
    assert_eq!(rt_json["apiVersion"], "v1");
    assert_eq!(rt_json["kind"], "Pod");
    let pod2: Pod = ::serde_json::from_value(rt_json).unwrap();
    assert_eq!(pod, pod2);
}
