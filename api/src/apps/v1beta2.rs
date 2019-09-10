use crate::core::v1::{ConditionStatus, PodTemplateSpec};
use kubernetes_apimachinery::meta::v1::{LabelSelector, ObjectMeta};
use kubernetes_apimachinery::meta::{IntOrString, Integer, Time};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Deployment {
    pub metadata: ObjectMeta,
    pub spec: DeploymentSpec,
    pub status: DeploymentStatus,
}

fn int1() -> Integer {
    1
}
fn int10() -> Integer {
    10
}
fn int600() -> Integer {
    600
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentSpec {
    #[serde(default)]
    pub min_ready_seconds: Integer,
    pub paused: bool,
    #[serde(default = "int600")]
    pub progress_deadline_seconds: Integer,
    #[serde(default = "int1")]
    pub replicas: Integer,
    #[serde(default = "int10")]
    pub revision_history_limit: Integer,
    pub selector: LabelSelector,
    pub strategy: DeploymentStrategy,
    pub template: PodTemplateSpec,
}

// TODO: This should be an enum, with a redundant adjacent/external tag
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentStrategy {
    #[serde(rename = "type")]
    pub typ: DeploymentStrategyType,
    pub rolling_update: Option<RollingUpdateDeployment>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum DeploymentStrategyType {
    Recreate,
    RollingUpdate,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RollingUpdateDeployment {
    pub max_surge: Option<IntOrString>,
    pub max_unavailable: Option<IntOrString>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentStatus {
    pub available_replicas: Integer,
    pub collision_count: Integer,
    #[serde(default)]
    pub conditions: Vec<DeploymentCondition>,
    pub observed_generation: Integer,
    pub ready_replicas: Integer,
    pub replicas: Integer,
    pub unavailable_replicas: Integer,
    pub updated_replicas: Integer,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentCondition {
    #[serde(rename = "type")]
    pub typ: DeploymentConditionType,
    pub status: ConditionStatus,
    pub last_update_time: Option<Time>,
    pub last_transition_time: Option<Time>,
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum DeploymentConditionType {
    Available,
    Progressing,
    ReplicaFailure,
}
