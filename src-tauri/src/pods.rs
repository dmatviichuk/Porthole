use std::collections::{BTreeMap, BTreeSet};

use k8s_openapi::{
    api::core::v1::{Container, ContainerStatus, Pod},
    apimachinery::pkg::api::resource::Quantity,
};
use serde::Serialize;

use crate::{quantity, summary::Health};

/// The pod-specific columns of a row.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSummary {
    /// What `kubectl get pods` prints in the STATUS column.
    pub status: String,
    pub ready: u32,
    pub total: u32,
    pub restarts: i32,
    pub node: Option<String>,
    pub ip: Option<String>,
    pub containers: Vec<ContainerRow>,
    /// Still holds node resources: not Succeeded or Failed.
    pub active: bool,
    pub requests: Resources,
    pub limits: Resources,
}

/// CPU in millicores, memory in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Resources {
    pub cpu: f64,
    pub memory: f64,
}

impl Resources {
    fn from_map(map: Option<&BTreeMap<String, Quantity>>) -> Self {
        let get = |key: &str| map.and_then(|m| m.get(key)).map(|q| q.0.as_str());
        Resources {
            cpu: get("cpu").and_then(quantity::millicores).unwrap_or(0.0),
            memory: get("memory").and_then(quantity::parse).unwrap_or(0.0),
        }
    }

    fn add(self, other: Self) -> Self {
        Resources {
            cpu: self.cpu + other.cpu,
            memory: self.memory + other.memory,
        }
    }

    fn max(self, other: Self) -> Self {
        Resources {
            cpu: self.cpu.max(other.cpu),
            memory: self.memory.max(other.memory),
        }
    }
}

/// Effective requests and limits, as the scheduler counts them: the larger of the
/// long-running containers (plus sidecars) and the biggest init container, plus pod overhead.
/// A container without a limit adds nothing to the limits, matching `kubectl describe node`.
pub fn pod_resources(pod: &Pod) -> (Resources, Resources) {
    let Some(spec) = pod.spec.as_ref() else {
        return Default::default();
    };
    let init = spec.init_containers.as_deref().unwrap_or_default();
    let sidecars = init.iter().filter(|c| c.restart_policy.as_deref() == Some("Always"));
    let one_shot_init = init.iter().filter(|c| c.restart_policy.as_deref() != Some("Always"));
    let pick = |c: &Container, limits: bool| {
        let r = c.resources.as_ref();
        Resources::from_map(if limits {
            r.and_then(|r| r.limits.as_ref())
        } else {
            r.and_then(|r| r.requests.as_ref())
        })
    };
    let overhead = Resources::from_map(spec.overhead.as_ref());
    let effective = |limits: bool| {
        let running = spec
            .containers
            .iter()
            .chain(sidecars.clone())
            .fold(Resources::default(), |acc, c| acc.add(pick(c, limits)));
        let init_peak = one_shot_init
            .clone()
            .fold(Resources::default(), |acc, c| acc.max(pick(c, limits)));
        running.max(init_peak).add(overhead)
    };
    (effective(false), effective(true))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerRow {
    pub name: String,
    pub image: String,
    pub init: bool,
    pub ready: bool,
    pub restarts: i32,
    pub state: String,
    pub requests: Resources,
    pub limits: Resources,
}

pub fn summarize_pod(pod: &Pod) -> PodSummary {
    let status = pod.status.as_ref();
    let main_statuses = status.and_then(|s| s.container_statuses.as_deref()).unwrap_or_default();
    let init_statuses = status
        .and_then(|s| s.init_container_statuses.as_deref())
        .unwrap_or_default();
    let main_specs = pod.spec.as_ref().map(|s| s.containers.as_slice()).unwrap_or_default();
    let init_specs = pod
        .spec
        .as_ref()
        .and_then(|s| s.init_containers.as_deref())
        .unwrap_or_default();

    // Sidecars (init containers with restartPolicy Always) count as regular containers, like kubectl.
    let sidecar_statuses = init_statuses.iter().filter(|s| is_sidecar(init_specs, &s.name));
    let counted: Vec<&ContainerStatus> = main_statuses.iter().chain(sidecar_statuses).collect();
    let total = main_specs.len() + init_specs.iter().filter(|c| is_sidecar(init_specs, &c.name)).count();

    let containers = init_specs
        .iter()
        .map(|c| container_row(c, init_statuses, true))
        .chain(main_specs.iter().map(|c| container_row(c, main_statuses, false)))
        .collect();

    let phase = status.and_then(|s| s.phase.as_deref());
    let (requests, limits) = pod_resources(pod);
    PodSummary {
        status: pod_status(pod),
        ready: counted.iter().filter(|s| s.ready).count() as u32,
        total: total as u32,
        restarts: main_statuses.iter().chain(init_statuses).map(|s| s.restart_count).sum(),
        node: pod.spec.as_ref().and_then(|s| s.node_name.clone()),
        ip: status.and_then(|s| s.pod_ip.clone()),
        containers,
        active: !matches!(phase, Some("Succeeded" | "Failed")),
        requests,
        limits,
    }
}

/// ConfigMaps, Secrets and PersistentVolumeClaims a pod mounts or reads env from.
#[derive(Debug, Default, PartialEq)]
pub struct References {
    pub config_maps: BTreeSet<String>,
    pub secrets: BTreeSet<String>,
    pub pvcs: BTreeSet<String>,
}

pub fn references(pod: &Pod) -> References {
    let mut refs = References::default();
    let Some(spec) = pod.spec.as_ref() else { return refs };
    for volume in spec.volumes.iter().flatten() {
        if let Some(cm) = &volume.config_map {
            refs.config_maps.insert(cm.name.clone());
        }
        if let Some(name) = volume.secret.as_ref().and_then(|s| s.secret_name.clone()) {
            refs.secrets.insert(name);
        }
        if let Some(claim) = &volume.persistent_volume_claim {
            refs.pvcs.insert(claim.claim_name.clone());
        }
        // Kubernetes adds `kube-api-access-*` (token + kube-root-ca.crt) to every pod; it says nothing about the app.
        let injected = volume.name.starts_with("kube-api-access-");
        let projected = volume.projected.iter().filter(|_| !injected);
        for source in projected.flat_map(|p| p.sources.iter().flatten()) {
            if let Some(cm) = &source.config_map {
                refs.config_maps.insert(cm.name.clone());
            }
            if let Some(secret) = &source.secret {
                refs.secrets.insert(secret.name.clone());
            }
        }
    }
    let containers = spec.containers.iter().chain(spec.init_containers.iter().flatten());
    for container in containers {
        for from in container.env_from.iter().flatten() {
            if let Some(cm) = &from.config_map_ref {
                refs.config_maps.insert(cm.name.clone());
            }
            if let Some(secret) = &from.secret_ref {
                refs.secrets.insert(secret.name.clone());
            }
        }
        for value_from in container.env.iter().flatten().filter_map(|e| e.value_from.as_ref()) {
            if let Some(key) = &value_from.config_map_key_ref {
                refs.config_maps.insert(key.name.clone());
            }
            if let Some(key) = &value_from.secret_key_ref {
                refs.secrets.insert(key.name.clone());
            }
        }
    }
    refs.config_maps.retain(|n| !n.is_empty());
    refs.secrets.retain(|n| !n.is_empty());
    refs
}

/// Buckets a kubectl status string into the colours the UI uses.
pub fn pod_health(status: &str, ready: u32, total: u32) -> Health {
    match status {
        "Running" if ready == total => Health::Ok,
        "Running" | "Pending" | "ContainerCreating" | "PodInitializing" | "NotReady" => Health::Progress,
        "Completed" | "Succeeded" => Health::Done,
        "Terminating" => Health::Ending,
        s if is_init_progress(s) => Health::Progress,
        _ => Health::Failed,
    }
}

/// `Init:1/3` is progress; `Init:CrashLoopBackOff` is a failure.
fn is_init_progress(status: &str) -> bool {
    status
        .strip_prefix("Init:")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(done, total)| done.parse::<u32>().is_ok() && total.parse::<u32>().is_ok())
}

fn is_sidecar(init_specs: &[Container], name: &str) -> bool {
    init_specs
        .iter()
        .any(|c| c.name == name && c.restart_policy.as_deref() == Some("Always"))
}

fn container_row(spec: &Container, statuses: &[ContainerStatus], init: bool) -> ContainerRow {
    let status = statuses.iter().find(|s| s.name == spec.name);
    let state = status.and_then(|s| s.state.as_ref());
    let state = match state {
        Some(st) if st.running.is_some() => "Running".to_owned(),
        Some(st) if st.waiting.is_some() => st
            .waiting
            .as_ref()
            .and_then(|w| w.reason.clone())
            .unwrap_or_else(|| "Waiting".to_owned()),
        Some(st) if st.terminated.is_some() => st
            .terminated
            .as_ref()
            .and_then(|t| t.reason.clone())
            .unwrap_or_else(|| "Terminated".to_owned()),
        _ => "Pending".to_owned(),
    };
    ContainerRow {
        name: spec.name.clone(),
        image: spec.image.clone().unwrap_or_default(),
        init,
        ready: status.is_some_and(|s| s.ready),
        restarts: status.map_or(0, |s| s.restart_count),
        state,
        requests: Resources::from_map(spec.resources.as_ref().and_then(|r| r.requests.as_ref())),
        limits: Resources::from_map(spec.resources.as_ref().and_then(|r| r.limits.as_ref())),
    }
}

fn non_empty(value: Option<&String>) -> Option<&str> {
    value.map(String::as_str).filter(|v| !v.is_empty())
}

/// Port of kubectl's `printPod` status logic (pkg/printers/internalversion/printers.go).
pub fn pod_status(pod: &Pod) -> String {
    let status = pod.status.as_ref();
    let mut reason = non_empty(status.and_then(|s| s.reason.as_ref()))
        .or(non_empty(status.and_then(|s| s.phase.as_ref())))
        .unwrap_or("Unknown")
        .to_owned();

    let init_specs = pod
        .spec
        .as_ref()
        .and_then(|s| s.init_containers.as_deref())
        .unwrap_or_default();
    let init_statuses = status
        .and_then(|s| s.init_container_statuses.as_deref())
        .unwrap_or_default();
    let mut initializing = false;
    for (i, c) in init_statuses.iter().enumerate() {
        let state = c.state.as_ref();
        let terminated = state.and_then(|s| s.terminated.as_ref());
        let waiting = state.and_then(|s| s.waiting.as_ref());
        if terminated.is_some_and(|t| t.exit_code == 0) {
            continue;
        }
        if is_sidecar(init_specs, &c.name) && c.started == Some(true) {
            continue;
        }
        initializing = true;
        reason = if let Some(t) = terminated {
            match (non_empty(t.reason.as_ref()), t.signal.unwrap_or(0)) {
                (Some(r), _) => format!("Init:{r}"),
                (None, 0) => format!("Init:ExitCode:{}", t.exit_code),
                (None, signal) => format!("Init:Signal:{signal}"),
            }
        } else if let Some(r) = non_empty(waiting.and_then(|w| w.reason.as_ref())).filter(|r| *r != "PodInitializing") {
            format!("Init:{r}")
        } else {
            format!("Init:{i}/{}", init_specs.len())
        };
        break;
    }

    if !initializing {
        let mut has_running = false;
        let statuses = status.and_then(|s| s.container_statuses.as_deref()).unwrap_or_default();
        // Reverse order so the first container's reason wins, as in kubectl.
        for c in statuses.iter().rev() {
            let state = c.state.as_ref();
            let waiting = state.and_then(|s| s.waiting.as_ref());
            let terminated = state.and_then(|s| s.terminated.as_ref());
            if let Some(r) = non_empty(waiting.and_then(|w| w.reason.as_ref())) {
                reason = r.to_owned();
            } else if let Some(t) = terminated {
                reason = match (non_empty(t.reason.as_ref()), t.signal.unwrap_or(0)) {
                    (Some(r), _) => r.to_owned(),
                    (None, 0) => format!("ExitCode:{}", t.exit_code),
                    (None, signal) => format!("Signal:{signal}"),
                };
            } else if c.ready && state.is_some_and(|s| s.running.is_some()) {
                has_running = true;
            }
        }
        if reason == "Completed" && has_running {
            let pod_ready = status
                .and_then(|s| s.conditions.as_deref())
                .unwrap_or_default()
                .iter()
                .any(|c| c.type_ == "Ready" && c.status == "True");
            reason = if pod_ready { "Running" } else { "NotReady" }.to_owned();
        }
    }

    if pod.metadata.deletion_timestamp.is_some() {
        reason = if status.and_then(|s| s.reason.as_deref()) == Some("NodeLost") {
            "Unknown".to_owned()
        } else {
            "Terminating".to_owned()
        };
    }
    reason
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    pub(crate) fn pod(spec: Value, status: Value) -> Pod {
        serde_json::from_value(json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": { "name": "web-1", "namespace": "default", "uid": "u-1" },
            "spec": spec,
            "status": status,
        }))
        .unwrap()
    }

    fn running(name: &str, ready: bool, restarts: i32) -> Value {
        json!({ "name": name, "image": "nginx", "imageID": "", "ready": ready, "restartCount": restarts,
                "state": { "running": { "startedAt": "2026-10-03T10:00:01Z" } } })
    }

    fn two_containers() -> Value {
        json!({ "containers": [{ "name": "app", "image": "nginx" }, { "name": "proxy", "image": "envoy" }] })
    }

    #[test]
    fn running_pod_counts_ready_containers() {
        let p = pod(
            two_containers(),
            json!({ "phase": "Running", "containerStatuses": [
                running("app", true, 1), running("proxy", false, 2)
            ]}),
        );
        let summary = summarize_pod(&p);
        assert_eq!(summary.status, "Running");
        assert_eq!((summary.ready, summary.total, summary.restarts), (1, 2, 3));
        assert_eq!(
            summary.containers.iter().map(|c| c.state.as_str()).collect::<Vec<_>>(),
            ["Running", "Running"]
        );
        assert_eq!(
            pod_health(&summary.status, summary.ready, summary.total),
            Health::Progress
        );
    }

    #[test]
    fn first_waiting_container_reason_wins() {
        let p = pod(
            two_containers(),
            json!({ "phase": "Running", "containerStatuses": [
                { "name": "app", "image": "nginx", "imageID": "", "ready": false, "restartCount": 5,
                  "state": { "waiting": { "reason": "CrashLoopBackOff" } } },
                { "name": "proxy", "image": "envoy", "imageID": "", "ready": false, "restartCount": 0,
                  "state": { "waiting": { "reason": "ImagePullBackOff" } } }
            ]}),
        );
        assert_eq!(pod_status(&p), "CrashLoopBackOff");
    }

    #[test]
    fn terminated_without_reason_shows_exit_code_or_signal() {
        let spec = json!({ "containers": [{ "name": "app" }] });
        let exited = pod(
            spec.clone(),
            json!({ "phase": "Running", "containerStatuses": [
                { "name": "app", "image": "", "imageID": "", "ready": false, "restartCount": 0,
                  "state": { "terminated": { "exitCode": 137 } } }
            ]}),
        );
        let signalled = pod(
            spec,
            json!({ "phase": "Running", "containerStatuses": [
                { "name": "app", "image": "", "imageID": "", "ready": false, "restartCount": 0,
                  "state": { "terminated": { "exitCode": 0, "signal": 9 } } }
            ]}),
        );
        assert_eq!(pod_status(&exited), "ExitCode:137");
        assert_eq!(pod_status(&signalled), "Signal:9");
    }

    #[test]
    fn init_container_progress_and_failures() {
        let spec = json!({ "initContainers": [{ "name": "migrate" }, { "name": "seed" }],
                           "containers": [{ "name": "app" }] });
        let waiting = pod(
            spec.clone(),
            json!({ "phase": "Pending", "initContainerStatuses": [
                { "name": "migrate", "image": "", "imageID": "", "ready": false, "restartCount": 0,
                  "state": { "terminated": { "exitCode": 0, "reason": "Completed" } } },
                { "name": "seed", "image": "", "imageID": "", "ready": false, "restartCount": 0,
                  "state": { "waiting": { "reason": "PodInitializing" } } }
            ]}),
        );
        let failed = pod(
            spec,
            json!({ "phase": "Pending", "initContainerStatuses": [
                { "name": "migrate", "image": "", "imageID": "", "ready": false, "restartCount": 3,
                  "state": { "terminated": { "exitCode": 1, "reason": "Error" } } }
            ]}),
        );
        assert_eq!(pod_status(&waiting), "Init:1/2");
        assert_eq!(pod_status(&failed), "Init:Error");
        assert_eq!(summarize_pod(&failed).restarts, 3);
        assert_eq!(pod_health("Init:1/2", 0, 1), Health::Progress);
        assert_eq!(pod_health("Init:Error", 0, 1), Health::Failed);
    }

    #[test]
    fn running_sidecar_counts_as_a_container() {
        let spec = json!({ "initContainers": [{ "name": "mesh", "restartPolicy": "Always" }],
                           "containers": [{ "name": "app" }] });
        let mut mesh = running("mesh", true, 0);
        mesh["started"] = json!(true);
        let p = pod(
            spec,
            json!({ "phase": "Running",
            "initContainerStatuses": [mesh],
            "containerStatuses": [running("app", true, 0)] }),
        );
        let summary = summarize_pod(&p);
        assert_eq!(summary.status, "Running");
        assert_eq!((summary.ready, summary.total), (2, 2));
        assert_eq!(
            summary
                .containers
                .iter()
                .map(|c| (c.name.as_str(), c.init))
                .collect::<Vec<_>>(),
            [("mesh", true), ("app", false)]
        );
    }

    #[test]
    fn requests_and_limits_follow_scheduler_rules() {
        let spec = json!({
            "initContainers": [
                { "name": "migrate", "resources": { "requests": { "cpu": "2", "memory": "1Gi" } } },
                { "name": "mesh", "restartPolicy": "Always",
                  "resources": { "requests": { "cpu": "100m", "memory": "64Mi" }, "limits": { "cpu": "200m" } } }
            ],
            "containers": [
                { "name": "app", "resources": { "requests": { "cpu": "250m", "memory": "256Mi" },
                                                "limits": { "cpu": "1", "memory": "512Mi" } } },
                { "name": "no-limits", "resources": { "requests": { "cpu": "50m" } } }
            ],
            "overhead": { "cpu": "10m" }
        });
        let (requests, limits) = pod_resources(&pod(spec, json!({ "phase": "Running" })));
        // The init container (2 cores, 1Gi) runs before the others start and outweighs their
        // 400m and 320Mi (sidecar included); the 10m overhead comes on top.
        assert_eq!(
            requests,
            Resources {
                cpu: 2010.0,
                memory: 1_073_741_824.0
            }
        );
        assert_eq!(
            limits,
            Resources {
                cpu: 1210.0,
                memory: 512.0 * 1_048_576.0
            }
        );
        let done = pod(
            json!({ "containers": [{ "name": "a" }] }),
            json!({ "phase": "Succeeded" }),
        );
        assert!(!summarize_pod(&done).active);
    }

    #[test]
    fn references_skip_the_injected_service_account_volume() {
        let spec = json!({
            "containers": [{ "name": "app", "env": [
                { "name": "TOKEN", "valueFrom": { "secretKeyRef": { "name": "api-token", "key": "token" } } }
            ] }],
            "volumes": [
                { "name": "kube-api-access-x7k2p", "projected": { "sources": [
                    { "serviceAccountToken": { "path": "token" } },
                    { "configMap": { "name": "kube-root-ca.crt" } }
                ] } },
                { "name": "config", "projected": { "sources": [{ "configMap": { "name": "app-config" } }] } }
            ]
        });
        let refs = references(&pod(spec, json!({ "phase": "Running" })));
        assert_eq!(refs.config_maps.into_iter().collect::<Vec<_>>(), ["app-config"]);
        assert_eq!(refs.secrets.into_iter().collect::<Vec<_>>(), ["api-token"]);
    }

    #[test]
    fn completed_and_terminating() {
        let spec = json!({ "containers": [{ "name": "job" }] });
        let status = json!({ "phase": "Succeeded", "containerStatuses": [
            { "name": "job", "image": "", "imageID": "", "ready": false, "restartCount": 0,
              "state": { "terminated": { "exitCode": 0, "reason": "Completed" } } }
        ]});
        assert_eq!(pod_status(&pod(spec.clone(), status)), "Completed");

        let mut deleting = pod(
            spec,
            json!({ "phase": "Running", "containerStatuses": [running("job", true, 0)] }),
        );
        deleting.metadata.deletion_timestamp = Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
            "2026-10-03T10:00:00Z".parse().unwrap(),
        ));
        assert_eq!(pod_status(&deleting), "Terminating");
    }
}
