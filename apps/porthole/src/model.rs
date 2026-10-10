//! View state and calculations ported from the 1.1.0 frontend.
use crate::{
    resources::ResourceType,
    summary::{Health, ResourceRow},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum View {
    Applications,
    Overview,
    AllResources,
    Resources(ResourceType),
    Resource {
        resource: ResourceType,
        namespace: Option<String>,
        object: String,
        tab: Tab,
        #[serde(default)]
        log_container: Option<String>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Tab {
    Overview,
    Logs,
    Events,
    Yaml,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Theme {
    Light,
    Dark,
    #[default]
    Auto,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterLabel {
    pub tag: String,
    pub color: String,
    pub confirm_deletes: bool,
}
impl Default for ClusterLabel {
    fn default() -> Self {
        Self {
            tag: String::new(),
            color: "gray".into(),
            confirm_deletes: false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub context: Option<String>,
    pub namespaces: BTreeMap<String, Option<String>>,
    pub theme: Theme,
    pub sidebar_width: f32,
    pub dock_height: f32,
    pub labels: BTreeMap<String, ClusterLabel>,
    pub columns: BTreeMap<String, Vec<String>>,
    pub timestamps: bool,
    pub wrap_logs: bool,
    pub format_logs: bool,
}
impl Default for Prefs {
    fn default() -> Self {
        Self {
            context: None,
            namespaces: BTreeMap::new(),
            theme: Theme::Auto,
            sidebar_width: 236.0,
            dock_height: 280.0,
            labels: BTreeMap::new(),
            columns: BTreeMap::new(),
            timestamps: true,
            wrap_logs: true,
            format_logs: true,
        }
    }
}
pub fn resource(kind: &str) -> ResourceType {
    let (group, plural, namespaced) = match kind {
        "Pod" => ("", "pods", true),
        "Namespace" => ("", "namespaces", false),
        "Node" => ("", "nodes", false),
        "Event" => ("", "events", true),
        "Service" => ("", "services", true),
        "ConfigMap" => ("", "configmaps", true),
        "Secret" => ("", "secrets", true),
        "PersistentVolumeClaim" => ("", "persistentvolumeclaims", true),
        "Ingress" => ("networking.k8s.io", "ingresses", true),
        "Deployment" => ("apps", "deployments", true),
        "StatefulSet" => ("apps", "statefulsets", true),
        "DaemonSet" => ("apps", "daemonsets", true),
        "ReplicaSet" => ("apps", "replicasets", true),
        "Job" => ("batch", "jobs", true),
        "CronJob" => ("batch", "cronjobs", true),
        _ => ("", "", true),
    };
    ResourceType {
        group: group.into(),
        version: "v1".into(),
        kind: kind.into(),
        plural: plural.into(),
        namespaced,
    }
}
pub const WARM: &[&str] = &[
    "Namespace",
    "Pod",
    "Deployment",
    "StatefulSet",
    "DaemonSet",
    "Job",
    "CronJob",
    "Node",
    "Service",
    "Ingress",
    "PersistentVolumeClaim",
];
pub const WORKLOADS: &[&str] = &["Deployment", "StatefulSet", "DaemonSet", "CronJob", "Job"];
pub fn has_logs(kind: &str) -> bool {
    matches!(
        kind,
        "Pod" | "Deployment" | "StatefulSet" | "DaemonSet" | "Job" | "CronJob" | "ReplicaSet"
    )
}
pub fn type_key(r: &ResourceType) -> String {
    if r.group.is_empty() {
        r.plural.clone()
    } else {
        format!("{}.{}", r.plural, r.group)
    }
}
pub fn api_version(r: &ResourceType) -> String {
    if r.group.is_empty() {
        r.version.clone()
    } else {
        format!("{}/{}", r.group, r.version)
    }
}
pub fn plural(kind: &str) -> String {
    if kind.ends_with('y') && !kind[..kind.len() - 1].ends_with(['a', 'e', 'i', 'o', 'u']) {
        format!("{}ies", &kind[..kind.len() - 1])
    } else if kind.ends_with(['s', 'x']) || kind.ends_with("ch") || kind.ends_with("sh") {
        format!("{kind}es")
    } else {
        format!("{kind}s")
    }
}
pub fn matches(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query.to_lowercase().split_whitespace().all(|t| text.contains(t))
}
pub fn num(r: &ResourceRow, k: &str) -> f64 {
    r.fields.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}
pub fn text(r: &ResourceRow, k: &str) -> String {
    r.fields.get(k).map(value_text).unwrap_or_default()
}
pub fn value_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(value_text).collect::<Vec<_>>().join(", "),
        _ => v.to_string(),
    }
}
pub fn active(r: &ResourceRow) -> bool {
    r.fields.get("active") == Some(&Value::Bool(true))
}
pub fn age(created: Option<i64>, now: i64) -> String {
    let Some(c) = created else {
        return String::new();
    };
    let s = (now - c).max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86400 {
        let h = s / 3600;
        let m = s % 3600 / 60;
        if h < 10 && m > 0 {
            format!("{h}h{m}m")
        } else {
            format!("{h}h")
        }
    } else if s < 365 * 86400 {
        format!("{}d", s / 86400)
    } else {
        format!("{}y", s / (365 * 86400))
    }
}
pub fn duration(s: i64) -> String {
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!(
            "{}m{}",
            s / 60,
            if s % 60 > 0 {
                format!("{}s", s % 60)
            } else {
                String::new()
            }
        )
    } else {
        format!(
            "{}h{}",
            s / 3600,
            if s % 3600 / 60 > 0 {
                format!("{}m", s % 3600 / 60)
            } else {
                String::new()
            }
        )
    }
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Amounts {
    pub cpu: f64,
    pub memory: f64,
}
impl Amounts {
    pub fn add(&mut self, other: Self) {
        self.cpu += other.cpu;
        self.memory += other.memory;
    }
    pub fn get(self, cpu: bool) -> f64 {
        if cpu { self.cpu } else { self.memory }
    }
}
pub fn requests(r: &ResourceRow) -> Amounts {
    Amounts {
        cpu: num(r, "cpuRequest"),
        memory: num(r, "memRequest"),
    }
}
pub fn limits(r: &ResourceRow) -> Amounts {
    Amounts {
        cpu: num(r, "cpuLimit"),
        memory: num(r, "memLimit"),
    }
}
pub fn percent(part: f64, whole: f64) -> f64 {
    if whole > 0.0 { part / whole * 100.0 } else { 0.0 }
}
pub fn amount(cpu: bool, v: f64) -> String {
    if cpu {
        if v >= 1000.0 {
            format!("{:.0}m", v)
        } else {
            format!("{v:.2}m")
        }
    } else if v >= 1073741824.0 {
        format!("{:.2}Gi", v / 1073741824.0)
    } else {
        format!("{:.2}Mi", v / 1048576.0)
    }
}
#[derive(Clone)]
pub struct Application {
    pub resource: ResourceType,
    pub row: ResourceRow,
    pub pods: Vec<ResourceRow>,
    pub pods_label: String,
    pub ratio: f64,
}
pub fn app_key(kind: &str, namespace: Option<&str>, name: &str) -> String {
    format!("{kind}/{}/{name}", namespace.unwrap_or_default())
}
fn rank(h: Option<Health>) -> i8 {
    match h {
        Some(Health::Failed) => 4,
        Some(Health::Progress) => 3,
        Some(Health::Ok) => 2,
        Some(Health::Ending) => 1,
        Some(Health::Done) => 0,
        None => -1,
    }
}
pub fn owner_key(p: &ResourceRow, cron: &HashMap<String, String>) -> Option<String> {
    let o = p.owner.as_ref()?;
    let ns = p.namespace.as_deref();
    match o.kind.as_str() {
        "ReplicaSet" => {
            let hash = p.labels.get("pod-template-hash")?;
            let name = o.name.strip_suffix(&format!("-{hash}"))?;
            Some(app_key("Deployment", ns, name))
        }
        "StatefulSet" | "DaemonSet" => Some(app_key(&o.kind, ns, &o.name)),
        "Job" => {
            let key = app_key("Job", ns, &o.name);
            Some(cron.get(&key).cloned().unwrap_or(key))
        }
        _ => None,
    }
}
pub fn applications(workloads: Vec<(ResourceType, Vec<ResourceRow>)>, pods: Vec<ResourceRow>) -> Vec<Application> {
    let mut cron = HashMap::new();
    for (r, rows) in &workloads {
        if r.kind == "Job" {
            for j in rows {
                if let Some(o) = &j.owner
                    && o.kind == "CronJob"
                {
                    cron.insert(
                        app_key("Job", j.namespace.as_deref(), &j.name),
                        app_key("CronJob", j.namespace.as_deref(), &o.name),
                    );
                }
            }
        }
    }
    let mut apps: Vec<Application> = vec![];
    let mut index = HashMap::new();
    for (r, rows) in workloads {
        for row in rows {
            if r.kind == "Job" && row.owner.as_ref().is_some_and(|o| o.kind == "CronJob") {
                continue;
            }
            index.insert(app_key(&r.kind, row.namespace.as_deref(), &row.name), apps.len());
            apps.push(Application {
                resource: r.clone(),
                row,
                pods: vec![],
                pods_label: String::new(),
                ratio: 0.0,
            });
        }
    }
    for p in pods {
        if let Some(i) = owner_key(&p, &cron).and_then(|key| index.get(&key)).copied() {
            apps[i].pods.push(p);
        } else {
            apps.push(Application {
                resource: resource("Pod"),
                row: p.clone(),
                pods: vec![p],
                pods_label: String::new(),
                ratio: 0.0,
            });
        }
    }
    for a in &mut apps {
        match a.resource.kind.as_str() {
            "Pod" => {
                a.pods_label = text(&a.row, "ready");
                a.ratio = if num(&a.row, "total") > 0.0 {
                    num(&a.row, "readyCount") / num(&a.row, "total")
                } else {
                    0.0
                };
            }
            "CronJob" => {
                let n = a
                    .pods
                    .iter()
                    .filter(|p| matches!(p.health, Some(Health::Ok | Health::Progress)))
                    .count();
                a.pods_label = n.to_string();
                a.ratio = n as f64;
            }
            _ => {
                a.pods_label = if a.row.fields.contains_key("completions") {
                    text(&a.row, "completions")
                } else {
                    text(&a.row, "ready")
                };
                a.ratio = if num(&a.row, "desired") > 0.0 {
                    num(&a.row, "readyCount") / num(&a.row, "desired")
                } else {
                    1.0
                };
            }
        }
        if a.resource.kind != "Job" && a.resource.kind != "CronJob" {
            for p in &a.pods {
                if rank(p.health) > rank(a.row.health) {
                    a.row.health = p.health;
                    a.row.status = p.status.clone();
                }
            }
        }
    }
    apps
}
#[derive(Clone, Deserialize)]
pub struct Container {
    pub name: String,
    pub image: String,
    pub init: bool,
    pub ready: bool,
    pub restarts: u64,
    pub state: String,
    pub requests: ContainerAmounts,
    pub limits: ContainerAmounts,
}
#[derive(Clone, Deserialize)]
pub struct ContainerAmounts {
    pub cpu: f64,
    pub memory: f64,
}
pub fn containers(r: &ResourceRow) -> Vec<Container> {
    r.fields
        .get("containers")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}
pub fn not_started(s: &str) -> bool {
    matches!(
        s,
        "Pending"
            | "Waiting"
            | "ContainerCreating"
            | "PodInitializing"
            | "ErrImagePull"
            | "ImagePullBackOff"
            | "CreateContainerConfigError"
            | "CreateContainerError"
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_formatting() {
        assert_eq!(age(Some(0), 12001), "3h20m");
        assert_eq!(age(Some(0), 864000), "10d");
        assert_eq!(duration(7300), "2h1m");
        assert_eq!(amount(true, 624.4), "624.40m");
        assert_eq!(amount(false, 1073741824.0), "1.00Gi");
        assert!(matches("web RUN", "web Running"));
        assert!(!matches("web failed", "web Running"));
    }
}
