use kube::{
    Api, Client, ResourceExt,
    api::{DynamicObject, ListParams},
    core::ApiResource,
};
use serde::Serialize;
use serde_json::Value;

use crate::{error::Result, quantity};

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct ContainerUsage {
    pub name: String,
    pub cpu: f64,
    pub memory: f64,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct PodUsage {
    pub namespace: String,
    pub name: String,
    /// Sum of the containers, millicores.
    pub cpu: f64,
    /// Sum of the containers, bytes.
    pub memory: f64,
    pub containers: Vec<ContainerUsage>,
}

fn metrics_api(plural: &str, kind: &str) -> ApiResource {
    ApiResource {
        group: "metrics.k8s.io".into(),
        version: "v1beta1".into(),
        api_version: "metrics.k8s.io/v1beta1".into(),
        kind: kind.into(),
        plural: plural.into(),
    }
}

fn usage_of(v: &Value) -> (f64, f64) {
    let cpu = v
        .pointer("/usage/cpu")
        .and_then(Value::as_str)
        .and_then(quantity::millicores);
    let memory = v
        .pointer("/usage/memory")
        .and_then(Value::as_str)
        .and_then(quantity::parse);
    (cpu.unwrap_or(0.0), memory.unwrap_or(0.0))
}

/// Per-pod and per-container usage; `namespace` None covers the whole cluster.
pub async fn pod_usage(client: Client, namespace: Option<&str>) -> Result<Vec<PodUsage>> {
    let resource = metrics_api("pods", "PodMetrics");
    let api = match namespace {
        Some(ns) => Api::<DynamicObject>::namespaced_with(client, ns, &resource),
        None => Api::<DynamicObject>::all_with(client, &resource),
    };
    let list = api.list(&ListParams::default()).await?;
    Ok(list
        .items
        .iter()
        .map(|m| {
            let containers: Vec<ContainerUsage> = m
                .data
                .get("containers")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|c| {
                    let (cpu, memory) = usage_of(c);
                    let name = c.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
                    ContainerUsage { name, cpu, memory }
                })
                .collect();
            PodUsage {
                namespace: m.namespace().unwrap_or_default(),
                name: m.name_any(),
                cpu: containers.iter().map(|c| c.cpu).sum(),
                memory: containers.iter().map(|c| c.memory).sum(),
                containers,
            }
        })
        .collect())
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct NodeUsage {
    pub name: String,
    /// Millicores.
    pub cpu: f64,
    /// Bytes.
    pub memory: f64,
}

/// Current per-node usage from metrics-server (`metrics.k8s.io`). The API cannot be
/// watched, so callers poll; metrics-server itself only refreshes every 15 s or so.
pub async fn node_usage(client: Client) -> Result<Vec<NodeUsage>> {
    let api = Api::<DynamicObject>::all_with(client, &metrics_api("nodes", "NodeMetrics"));
    let list = api.list(&ListParams::default()).await?;
    Ok(list
        .items
        .iter()
        .map(|m| {
            let (cpu, memory) = usage_of(&m.data);
            NodeUsage {
                name: m.name_any(),
                cpu,
                memory,
            }
        })
        .collect())
}
