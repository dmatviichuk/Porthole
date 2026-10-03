use std::collections::BTreeMap;

use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::OwnerReference, jiff::Timestamp};
use kube::{ResourceExt, api::DynamicObject};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::{pods, quantity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Ok,
    Progress,
    Failed,
    Done,
    Ending,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Owner {
    pub kind: String,
    pub name: String,
}

/// One table row for any resource kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRow {
    pub uid: String,
    pub name: String,
    pub namespace: Option<String>,
    /// Creation time, unix seconds.
    pub created: Option<i64>,
    pub owner: Option<Owner>,
    pub labels: BTreeMap<String, String>,
    /// Only on detail watches (one object): values such as last-applied-configuration
    /// can be kilobytes, too much to resend for every row of a list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<BTreeMap<String, String>>,
    pub status: Option<String>,
    pub health: Option<Health>,
    /// Kind-specific columns, keyed by the column ids the frontend defines for the kind.
    pub fields: Map<String, Value>,
}

/// `detail` adds what only a single-object view needs: annotations and ConfigMap data.
pub fn summarize(kind: &str, obj: &DynamicObject, detail: bool) -> ResourceRow {
    let mut row = ResourceRow {
        uid: obj.uid().unwrap_or_default(),
        name: obj.name_any(),
        namespace: obj.namespace(),
        created: obj.metadata.creation_timestamp.as_ref().map(|t| t.0.as_second()),
        owner: controller(obj.owner_references()),
        labels: obj.labels().clone(),
        annotations: detail.then(|| obj.annotations().clone()),
        status: None,
        health: None,
        fields: Map::new(),
    };
    let d = &obj.data;
    let terminating = obj.metadata.deletion_timestamp.is_some();
    match kind {
        "Pod" => pod(obj, &mut row),
        "Deployment" | "StatefulSet" | "ReplicaSet" => replicas(kind, d, &mut row),
        "DaemonSet" => daemon_set(d, &mut row),
        "Job" => job(d, &mut row),
        "CronJob" => cron_job(d, &mut row),
        "Service" => service(d, &mut row),
        "Ingress" => ingress(d, &mut row),
        "ConfigMap" => config_map(d, &mut row, detail),
        "Secret" => {
            row.fields.insert("type".into(), json!(str_at(d, "/type")));
            row.fields.insert("keys".into(), json!(count_keys(d, "/data")));
        }
        "Namespace" => {
            let phase = str_at(d, "/status/phase").unwrap_or("Active");
            let health = if phase == "Active" { Health::Ok } else { Health::Ending };
            set_status(&mut row, phase, health);
        }
        "Node" => node(d, &mut row),
        "PersistentVolumeClaim" => pvc(d, &mut row),
        "PersistentVolume" => pv(d, &mut row),
        "Event" => event(obj, &mut row),
        _ => {}
    }
    if terminating && kind != "Pod" {
        set_status(&mut row, "Terminating", Health::Ending);
    }
    row
}

fn controller(owners: &[OwnerReference]) -> Option<Owner> {
    owners
        .iter()
        .find(|o| o.controller == Some(true))
        .or(owners.first())
        .map(|o| Owner {
            kind: o.kind.clone(),
            name: o.name.clone(),
        })
}

fn set_status(row: &mut ResourceRow, status: &str, health: Health) {
    row.status = Some(status.to_owned());
    row.health = Some(health);
}

fn str_at<'a>(v: &'a Value, pointer: &str) -> Option<&'a str> {
    v.pointer(pointer)?.as_str()
}

fn int_at(v: &Value, pointer: &str) -> Option<i64> {
    v.pointer(pointer)?.as_i64()
}

fn count_keys(v: &Value, pointer: &str) -> usize {
    v.pointer(pointer).and_then(Value::as_object).map_or(0, Map::len)
}

fn array_at<'a>(v: &'a Value, pointer: &str) -> &'a [Value] {
    v.pointer(pointer).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// `[{name, image}]` of a pod template's containers, e.g. under `/spec/template/spec`.
fn images(v: &Value, pod_spec: &str) -> Value {
    let images: Vec<Value> = array_at(v, &format!("{pod_spec}/containers"))
        .iter()
        .map(|c| json!({ "name": c.get("name"), "image": c.get("image") }))
        .collect();
    json!(images)
}

fn time_at(v: &Value, pointer: &str) -> Option<i64> {
    str_at(v, pointer)?.parse::<Timestamp>().ok().map(|t| t.as_second())
}

/// The condition of `type_`, if its status is `status`.
fn condition<'a>(d: &'a Value, type_: &str, status: &str) -> Option<&'a Value> {
    array_at(d, "/status/conditions").iter().find(|c| {
        c.get("type").and_then(Value::as_str) == Some(type_) && c.get("status").and_then(Value::as_str) == Some(status)
    })
}

fn reason(condition: &Value, fallback: &str) -> String {
    condition
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_owned()
}

fn pod(obj: &DynamicObject, row: &mut ResourceRow) {
    let mut value = serde_json::to_value(obj).unwrap_or_default();
    // List and watch items omit apiVersion/kind, which the typed Pod expects.
    value["apiVersion"] = json!("v1");
    value["kind"] = json!("Pod");
    let Ok(pod) = serde_json::from_value::<Pod>(value) else {
        return;
    };
    let summary = pods::summarize_pod(&pod);
    let refs = pods::references(&pod);
    let health = pods::pod_health(&summary.status, summary.ready, summary.total);
    set_status(row, &summary.status, health);
    if let Value::Object(fields) = json!({
        "ready": format!("{}/{}", summary.ready, summary.total),
        "readyCount": summary.ready,
        "total": summary.total,
        "restarts": summary.restarts,
        "node": summary.node,
        "ip": summary.ip,
        "containers": summary.containers,
        "active": summary.active,
        "cpuRequest": summary.requests.cpu,
        "cpuLimit": summary.limits.cpu,
        "memRequest": summary.requests.memory,
        "memLimit": summary.limits.memory,
        "configMaps": refs.config_maps,
        "secrets": refs.secrets,
        "pvcs": refs.pvcs,
    }) {
        row.fields = fields;
    }
}

fn replicas(kind: &str, d: &Value, row: &mut ResourceRow) {
    let desired = int_at(d, "/spec/replicas").unwrap_or(1);
    let ready = int_at(d, "/status/readyReplicas").unwrap_or(0);
    let updated = int_at(d, "/status/updatedReplicas").unwrap_or(0);
    let available = int_at(d, "/status/availableReplicas").unwrap_or(0);
    row.fields.insert("ready".into(), json!(format!("{ready}/{desired}")));
    row.fields.insert("readyCount".into(), json!(ready));
    row.fields.insert("desired".into(), json!(desired));
    row.fields.insert("upToDate".into(), json!(updated));
    row.fields.insert("available".into(), json!(available));
    row.fields.insert("images".into(), images(d, "/spec/template/spec"));

    let stalled = condition(d, "Progressing", "False").or(condition(d, "ReplicaFailure", "True"));
    if desired == 0 {
        set_status(row, "Scaled down", Health::Done);
    } else if let Some(c) = stalled {
        set_status(row, &reason(c, "Failed"), Health::Failed);
    } else if ready >= desired && (kind != "Deployment" || updated >= desired) {
        set_status(row, "Running", Health::Ok);
    } else {
        set_status(row, "Updating", Health::Progress);
    }
}

fn daemon_set(d: &Value, row: &mut ResourceRow) {
    let desired = int_at(d, "/status/desiredNumberScheduled").unwrap_or(0);
    let ready = int_at(d, "/status/numberReady").unwrap_or(0);
    row.fields.insert("ready".into(), json!(format!("{ready}/{desired}")));
    row.fields.insert("readyCount".into(), json!(ready));
    row.fields.insert("desired".into(), json!(desired));
    row.fields.insert(
        "upToDate".into(),
        json!(int_at(d, "/status/updatedNumberScheduled").unwrap_or(0)),
    );
    row.fields.insert(
        "available".into(),
        json!(int_at(d, "/status/numberAvailable").unwrap_or(0)),
    );
    row.fields.insert("images".into(), images(d, "/spec/template/spec"));
    if ready >= desired {
        set_status(row, "Running", Health::Ok);
    } else {
        set_status(row, "Updating", Health::Progress);
    }
}

fn job(d: &Value, row: &mut ResourceRow) {
    let completions = int_at(d, "/spec/completions").unwrap_or(1);
    let succeeded = int_at(d, "/status/succeeded").unwrap_or(0);
    let active = int_at(d, "/status/active").unwrap_or(0);
    row.fields
        .insert("completions".into(), json!(format!("{succeeded}/{completions}")));
    row.fields.insert("readyCount".into(), json!(succeeded));
    row.fields.insert("desired".into(), json!(completions));
    row.fields.insert("images".into(), images(d, "/spec/template/spec"));
    if let (Some(start), Some(end)) = (time_at(d, "/status/startTime"), time_at(d, "/status/completionTime")) {
        row.fields.insert("duration".into(), json!(end - start));
    }
    if let Some(c) = condition(d, "Failed", "True") {
        set_status(row, &reason(c, "Failed"), Health::Failed);
    } else if condition(d, "Complete", "True").is_some() || condition(d, "SuccessCriteriaMet", "True").is_some() {
        set_status(row, "Completed", Health::Done);
    } else if condition(d, "Suspended", "True").is_some() {
        set_status(row, "Suspended", Health::Done);
    } else if active > 0 {
        set_status(row, "Running", Health::Progress);
    } else {
        set_status(row, "Pending", Health::Progress);
    }
}

fn cron_job(d: &Value, row: &mut ResourceRow) {
    let active = array_at(d, "/status/active").len();
    let suspended = d.pointer("/spec/suspend").and_then(Value::as_bool).unwrap_or(false);
    row.fields.insert("schedule".into(), json!(str_at(d, "/spec/schedule")));
    row.fields.insert("active".into(), json!(active));
    row.fields
        .insert("lastSchedule".into(), json!(time_at(d, "/status/lastScheduleTime")));
    row.fields
        .insert("images".into(), images(d, "/spec/jobTemplate/spec/template/spec"));
    if suspended {
        set_status(row, "Suspended", Health::Done);
    } else if active > 0 {
        set_status(row, "Running", Health::Progress);
    } else {
        set_status(row, "Scheduled", Health::Ok);
    }
}

fn service(d: &Value, row: &mut ResourceRow) {
    let type_ = str_at(d, "/spec/type").unwrap_or("ClusterIP");
    let ports: Vec<String> = array_at(d, "/spec/ports")
        .iter()
        .map(|p| {
            let port = p.get("port").and_then(Value::as_i64).unwrap_or_default();
            let protocol = p.get("protocol").and_then(Value::as_str).unwrap_or("TCP");
            match p.get("nodePort").and_then(Value::as_i64) {
                Some(node_port) => format!("{port}:{node_port}/{protocol}"),
                None => format!("{port}/{protocol}"),
            }
        })
        .collect();
    let port_list: Vec<Value> = array_at(d, "/spec/ports")
        .iter()
        .map(|p| {
            json!({
                "name": p.get("name"),
                "port": p.get("port"),
                "targetPort": p.get("targetPort"),
                "nodePort": p.get("nodePort"),
                "protocol": p.get("protocol").and_then(Value::as_str).unwrap_or("TCP"),
            })
        })
        .collect();
    let external: Vec<&str> = array_at(d, "/status/loadBalancer/ingress")
        .iter()
        .filter_map(|i| i.get("hostname").or(i.get("ip")).and_then(Value::as_str))
        .chain(array_at(d, "/spec/externalIPs").iter().filter_map(Value::as_str))
        .collect();
    row.fields.insert("type".into(), json!(type_));
    row.fields
        .insert("clusterIP".into(), json!(str_at(d, "/spec/clusterIP")));
    row.fields.insert("external".into(), json!(external.join(", ")));
    row.fields.insert("ports".into(), json!(ports.join(", ")));
    row.fields.insert("portList".into(), json!(port_list));
    row.fields.insert(
        "selector".into(),
        d.pointer("/spec/selector").cloned().unwrap_or(json!({})),
    );
}

fn ingress(d: &Value, row: &mut ResourceRow) {
    let hosts: Vec<&str> = array_at(d, "/spec/rules")
        .iter()
        .filter_map(|r| r.get("host").and_then(Value::as_str))
        .collect();
    let address: Vec<&str> = array_at(d, "/status/loadBalancer/ingress")
        .iter()
        .filter_map(|i| i.get("hostname").or(i.get("ip")).and_then(Value::as_str))
        .collect();
    let mut backends: Vec<&str> = array_at(d, "/spec/rules")
        .iter()
        .flat_map(|r| array_at(r, "/http/paths"))
        .filter_map(|p| str_at(p, "/backend/service/name"))
        .chain(str_at(d, "/spec/defaultBackend/service/name"))
        .collect();
    backends.sort_unstable();
    backends.dedup();
    row.fields
        .insert("class".into(), json!(str_at(d, "/spec/ingressClassName")));
    row.fields.insert("hosts".into(), json!(hosts.join(", ")));
    row.fields.insert("address".into(), json!(address.join(", ")));
    row.fields.insert("backends".into(), json!(backends));
}

fn config_map(d: &Value, row: &mut ResourceRow, detail: bool) {
    let keys = count_keys(d, "/data") + count_keys(d, "/binaryData");
    row.fields.insert("keys".into(), json!(keys));
    if detail {
        // Values only for single-object watches; a namespace of dashboards can be megabytes.
        row.fields
            .insert("data".into(), d.get("data").cloned().unwrap_or(json!({})));
        let binary: Vec<&String> = d
            .get("binaryData")
            .and_then(Value::as_object)
            .map(|m| m.keys().collect())
            .unwrap_or_default();
        row.fields.insert("binaryKeys".into(), json!(binary));
    }
}

fn node(d: &Value, row: &mut ResourceRow) {
    let ready = condition(d, "Ready", "True").is_some();
    let cordoned = d
        .pointer("/spec/unschedulable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (status, health) = match (ready, cordoned) {
        (true, false) => ("Ready", Health::Ok),
        (true, true) => ("Ready,SchedulingDisabled", Health::Progress),
        (false, false) => ("NotReady", Health::Failed),
        (false, true) => ("NotReady,SchedulingDisabled", Health::Failed),
    };
    set_status(row, status, health);
    let roles: Vec<&str> = row
        .labels
        .keys()
        .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
        .filter(|r| !r.is_empty())
        .collect();
    let internal_ip = array_at(d, "/status/addresses")
        .iter()
        .find(|a| a.get("type").and_then(Value::as_str) == Some("InternalIP"))
        .and_then(|a| a.get("address").and_then(Value::as_str));
    let instance_type = row.labels.get("node.kubernetes.io/instance-type").cloned();
    row.fields.insert("roles".into(), json!(roles.join(", ")));
    row.fields
        .insert("version".into(), json!(str_at(d, "/status/nodeInfo/kubeletVersion")));
    row.fields.insert("internalIP".into(), json!(internal_ip));
    row.fields.insert("instanceType".into(), json!(instance_type));
    row.fields
        .insert("osImage".into(), json!(str_at(d, "/status/nodeInfo/osImage")));
    row.fields
        .insert("os".into(), json!(str_at(d, "/status/nodeInfo/operatingSystem")));
    row.fields
        .insert("arch".into(), json!(str_at(d, "/status/nodeInfo/architecture")));
    row.fields
        .insert("addresses".into(), json!(array_at(d, "/status/addresses")));
    row.fields.insert(
        "cpu".into(),
        json!(str_at(d, "/status/allocatable/cpu").and_then(quantity::millicores)),
    );
    row.fields.insert(
        "memory".into(),
        json!(str_at(d, "/status/allocatable/memory").and_then(quantity::parse)),
    );
    row.fields.insert(
        "podCapacity".into(),
        json!(str_at(d, "/status/allocatable/pods").and_then(quantity::parse)),
    );
}

fn pvc(d: &Value, row: &mut ResourceRow) {
    let phase = str_at(d, "/status/phase").unwrap_or("Pending");
    let health = match phase {
        "Bound" => Health::Ok,
        "Pending" => Health::Progress,
        _ => Health::Failed,
    };
    set_status(row, phase, health);
    row.fields
        .insert("capacity".into(), json!(str_at(d, "/status/capacity/storage")));
    row.fields
        .insert("storageClass".into(), json!(str_at(d, "/spec/storageClassName")));
    row.fields.insert("volume".into(), json!(str_at(d, "/spec/volumeName")));
}

fn pv(d: &Value, row: &mut ResourceRow) {
    let phase = str_at(d, "/status/phase").unwrap_or("Pending");
    let health = match phase {
        "Available" | "Bound" => Health::Ok,
        "Released" => Health::Done,
        "Pending" => Health::Progress,
        _ => Health::Failed,
    };
    set_status(row, phase, health);
    let claim = match (str_at(d, "/spec/claimRef/namespace"), str_at(d, "/spec/claimRef/name")) {
        (Some(ns), Some(name)) => Some(format!("{ns}/{name}")),
        _ => None,
    };
    row.fields
        .insert("capacity".into(), json!(str_at(d, "/spec/capacity/storage")));
    row.fields
        .insert("storageClass".into(), json!(str_at(d, "/spec/storageClassName")));
    row.fields.insert("claim".into(), json!(claim));
    row.fields.insert(
        "reclaimPolicy".into(),
        json!(str_at(d, "/spec/persistentVolumeReclaimPolicy")),
    );
}

fn event(obj: &DynamicObject, row: &mut ResourceRow) {
    let d = &obj.data;
    let warning = str_at(d, "/type") == Some("Warning");
    let reason = str_at(d, "/reason").unwrap_or("");
    set_status(row, reason, if warning { Health::Failed } else { Health::Ok });
    let object = match (str_at(d, "/involvedObject/kind"), str_at(d, "/involvedObject/name")) {
        (Some(kind), Some(name)) => format!("{kind}/{name}"),
        _ => String::new(),
    };
    let last_seen = time_at(d, "/lastTimestamp").or(time_at(d, "/eventTime")).or(obj
        .metadata
        .creation_timestamp
        .as_ref()
        .map(|t| t.0.as_second()));
    row.fields.insert("type".into(), json!(str_at(d, "/type")));
    row.fields.insert("message".into(), json!(str_at(d, "/message")));
    row.fields.insert("object".into(), json!(object));
    row.fields
        .insert("count".into(), json!(int_at(d, "/count").unwrap_or(1)));
    row.fields.insert("lastSeen".into(), json!(last_seen));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// List rows: what every table watch produces.
    fn summarize(kind: &str, obj: &DynamicObject) -> ResourceRow {
        super::summarize(kind, obj, false)
    }

    fn object(data: Value) -> DynamicObject {
        let mut value = json!({
            "metadata": {
                "name": "web", "namespace": "default", "uid": "u-1",
                "creationTimestamp": "2026-10-03T10:00:00Z",
                "labels": { "app": "web" },
                "ownerReferences": [{ "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "web-abc",
                                      "uid": "rs-1", "controller": true }]
            }
        });
        if let (Value::Object(target), Value::Object(extra)) = (&mut value, data) {
            target.extend(extra);
        }
        let mut obj: DynamicObject = serde_json::from_value(value).unwrap();
        obj.types = None;
        obj
    }

    #[test]
    fn common_fields_for_every_kind() {
        let row = summarize("Widget", &object(json!({ "spec": {} })));
        assert_eq!(
            (row.uid.as_str(), row.name.as_str(), row.namespace.as_deref()),
            ("u-1", "web", Some("default"))
        );
        assert_eq!(row.created, Some(1_791_021_600));
        assert_eq!(
            row.owner,
            Some(Owner {
                kind: "ReplicaSet".into(),
                name: "web-abc".into()
            })
        );
        assert_eq!(row.labels.get("app").map(String::as_str), Some("web"));
        assert_eq!((row.status, row.health), (None, None));
    }

    #[test]
    fn annotations_and_config_data_only_on_detail_rows() {
        let mut obj = object(json!({ "data": { "LOG_LEVEL": "info" }, "binaryData": { "cert.der": "AAEC" } }));
        obj.metadata.annotations = Some(BTreeMap::from([("note".into(), "hello".into())]));
        let list = summarize("ConfigMap", &obj);
        assert_eq!((list.annotations, list.fields.get("data")), (None, None));
        assert_eq!(list.fields["keys"], json!(2));
        let detail = super::summarize("ConfigMap", &obj, true);
        assert_eq!(detail.annotations.unwrap()["note"], "hello");
        assert_eq!(detail.fields["data"], json!({ "LOG_LEVEL": "info" }));
        assert_eq!(detail.fields["binaryKeys"], json!(["cert.der"]));
    }

    #[test]
    fn pod_rows_carry_kubectl_status_and_references() {
        let row = summarize(
            "Pod",
            &object(json!({
                "spec": {
                    "containers": [{ "name": "app", "image": "nginx",
                                     "envFrom": [{ "configMapRef": { "name": "settings" } }] }],
                    "volumes": [{ "name": "data", "persistentVolumeClaim": { "claimName": "data-web-0" } }]
                },
                "status": { "phase": "Running", "containerStatuses": [{
                    "name": "app", "image": "nginx", "imageID": "", "ready": false, "restartCount": 4,
                    "state": { "waiting": { "reason": "CrashLoopBackOff" } } }] }
            })),
        );
        assert_eq!(row.status.as_deref(), Some("CrashLoopBackOff"));
        assert_eq!(row.health, Some(Health::Failed));
        assert_eq!(row.fields["ready"], json!("0/1"));
        assert_eq!(row.fields["restarts"], json!(4));
        assert_eq!(row.fields["containers"][0]["name"], json!("app"));
        assert_eq!(row.fields["configMaps"], json!(["settings"]));
        assert_eq!(row.fields["pvcs"], json!(["data-web-0"]));
    }

    #[test]
    fn deployment_status_follows_replicas_and_conditions() {
        let running = summarize(
            "Deployment",
            &object(json!({
                "spec": { "replicas": 2, "template": { "spec": { "containers": [{ "name": "app", "image": "nginx:1.29" }] } } },
                "status": { "readyReplicas": 2, "updatedReplicas": 2, "availableReplicas": 2 }
            })),
        );
        assert_eq!(
            (running.status.as_deref(), running.health),
            (Some("Running"), Some(Health::Ok))
        );
        assert_eq!(running.fields["ready"], json!("2/2"));
        assert_eq!(
            running.fields["images"],
            json!([{ "name": "app", "image": "nginx:1.29" }])
        );

        let rolling = summarize(
            "Deployment",
            &object(json!({
                "spec": { "replicas": 3 }, "status": { "readyReplicas": 3, "updatedReplicas": 1 }
            })),
        );
        assert_eq!(rolling.health, Some(Health::Progress));

        let stuck = summarize(
            "Deployment",
            &object(json!({
                "spec": { "replicas": 1 },
                "status": { "conditions": [{ "type": "Progressing", "status": "False", "reason": "ProgressDeadlineExceeded" }] }
            })),
        );
        assert_eq!(
            (stuck.status.as_deref(), stuck.health),
            (Some("ProgressDeadlineExceeded"), Some(Health::Failed))
        );

        let zero = summarize("Deployment", &object(json!({ "spec": { "replicas": 0 } })));
        assert_eq!(zero.health, Some(Health::Done));
    }

    #[test]
    fn jobs_and_cronjobs() {
        let done = summarize(
            "Job",
            &object(json!({
                "spec": { "completions": 1 },
                "status": { "succeeded": 1, "startTime": "2026-10-03T10:00:00Z", "completionTime": "2026-10-03T10:01:30Z",
                            "conditions": [{ "type": "Complete", "status": "True" }] }
            })),
        );
        assert_eq!(
            (done.status.as_deref(), done.health),
            (Some("Completed"), Some(Health::Done))
        );
        assert_eq!(done.fields["duration"], json!(90));

        let failed = summarize(
            "Job",
            &object(json!({
                "status": { "failed": 6, "conditions": [{ "type": "Failed", "status": "True", "reason": "BackoffLimitExceeded" }] }
            })),
        );
        assert_eq!(failed.status.as_deref(), Some("BackoffLimitExceeded"));

        let cron = summarize(
            "CronJob",
            &object(json!({ "spec": { "schedule": "*/5 * * * *", "suspend": true } })),
        );
        assert_eq!(
            (cron.status.as_deref(), cron.fields["schedule"].as_str()),
            (Some("Suspended"), Some("*/5 * * * *"))
        );
    }

    #[test]
    fn services_ingresses_nodes_and_namespaces() {
        let svc = summarize(
            "Service",
            &object(json!({
                "spec": { "type": "LoadBalancer", "clusterIP": "10.0.0.1", "selector": { "app": "web" },
                          "ports": [{ "port": 443, "nodePort": 30443, "protocol": "TCP", "targetPort": "https" },
                                    { "port": 53, "protocol": "UDP" }] },
                "status": { "loadBalancer": { "ingress": [{ "hostname": "lb.example.com" }] } }
            })),
        );
        assert_eq!(svc.fields["ports"], json!("443:30443/TCP, 53/UDP"));
        assert_eq!(svc.fields["external"], json!("lb.example.com"));
        assert_eq!(svc.fields["selector"], json!({ "app": "web" }));
        assert_eq!(svc.fields["portList"][0]["targetPort"], json!("https"));

        let ing = summarize(
            "Ingress",
            &object(json!({
                "spec": {
                    "defaultBackend": { "service": { "name": "fallback" } },
                    "rules": [{ "host": "a.example.com", "http": { "paths": [
                        { "path": "/", "backend": { "service": { "name": "web" } } },
                        { "path": "/api", "backend": { "service": { "name": "web" } } }
                    ] } }]
                }
            })),
        );
        assert_eq!(ing.fields["backends"], json!(["fallback", "web"]));

        let mut node_obj = object(json!({
            "spec": { "unschedulable": true },
            "status": { "conditions": [{ "type": "Ready", "status": "True" }],
                        "nodeInfo": { "kubeletVersion": "v1.36.1", "osImage": "Bottlerocket OS 1.66.0",
                                      "operatingSystem": "linux", "architecture": "amd64" },
                        "allocatable": { "cpu": "3920m", "memory": "16302624Ki", "pods": "110" },
                        "addresses": [{ "type": "InternalIP", "address": "10.0.0.7" }] }
        }));
        node_obj.metadata.labels = Some(BTreeMap::from([(
            "node-role.kubernetes.io/control-plane".into(),
            String::new(),
        )]));
        let node = summarize("Node", &node_obj);
        assert_eq!(node.status.as_deref(), Some("Ready,SchedulingDisabled"));
        assert_eq!(node.fields["roles"], json!("control-plane"));
        assert_eq!(
            (node.fields["cpu"].as_f64(), node.fields["podCapacity"].as_f64()),
            (Some(3920.0), Some(110.0))
        );
        assert_eq!(node.fields["memory"].as_f64(), Some(16_693_886_976.0));
        assert_eq!(
            (node.fields["os"].as_str(), node.fields["arch"].as_str()),
            (Some("linux"), Some("amd64"))
        );
        assert_eq!(node.fields["internalIP"], json!("10.0.0.7"));

        let mut ns = object(json!({ "status": { "phase": "Active" } }));
        assert_eq!(summarize("Namespace", &ns).health, Some(Health::Ok));
        ns.metadata.deletion_timestamp = ns.metadata.creation_timestamp.clone();
        assert_eq!(summarize("Namespace", &ns).status.as_deref(), Some("Terminating"));
    }
}
