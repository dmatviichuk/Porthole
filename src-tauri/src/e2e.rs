//! End-to-end run of the backend against a real cluster: apply, watch, logs, shell, edit, delete.
//!
//!     PORTHOLE_E2E_CONTEXT=<context> cargo test e2e -- --ignored --nocapture
//!
//! Creates the namespace `porthole-e2e` and deletes it at the end. Never point it at a shared cluster.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api,
    api::TerminalSize,
    api::{DynamicObject, Patch, PatchParams},
    core::GroupVersionKind,
    runtime::watcher,
};
use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::sync::mpsc;

use crate::{
    clusters::Clusters,
    exec::{self, TerminalInput},
    logs::{self, LogOptions, LogTarget},
    resources::{self, ResourceType},
    summary, watch,
};

const NS: &str = "porthole-e2e";

const MANIFESTS: &str = r#"
apiVersion: v1
kind: Namespace
metadata:
  name: porthole-e2e
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: web
  namespace: porthole-e2e
spec:
  replicas: 2
  selector:
    matchLabels: { app: web }
  template:
    metadata:
      labels: { app: web }
    spec:
      terminationGracePeriodSeconds: 1
      containers:
        - name: web
          image: busybox:1.37
          command: ["sh", "-c", "i=0; while true; do echo tick-$i; i=$((i+1)); sleep 1; done"]
---
apiVersion: v1
kind: Pod
metadata:
  name: crasher
  namespace: porthole-e2e
spec:
  containers:
    - name: boom
      image: busybox:1.37
      command: ["sh", "-c", "echo boom; exit 3"]
"#;

/// The resource type serving a manifest's kind (discovery lists the preferred version).
async fn resolve(clusters: &Clusters, context: &str, gvk: &GroupVersionKind) -> ResourceType {
    let types = clusters.resources(context, false).await.unwrap();
    let found = types
        .iter()
        .map(|info| &info.resource)
        .find(|r| r.group == gvk.group && r.kind == gvk.kind)
        .unwrap_or_else(|| panic!("the cluster does not serve {gvk:?}"));
    ResourceType {
        version: gvk.version.clone(),
        ..found.clone()
    }
}

/// JSON messages a channel received, in order.
type Inbox = Arc<Mutex<Vec<Value>>>;

fn json_channel<T>() -> (Channel<T>, Inbox) {
    let inbox: Inbox = Arc::default();
    let sink = inbox.clone();
    let channel = Channel::new(move |body| {
        if let InvokeResponseBody::Json(json) = body {
            sink.lock().unwrap().push(serde_json::from_str(&json).unwrap());
        }
        Ok(())
    });
    (channel, inbox)
}

/// Folds Reset/Changes watch messages into the current rows, keyed by uid.
fn current_rows(inbox: &Inbox) -> HashMap<String, Value> {
    let mut rows = HashMap::new();
    for message in inbox.lock().unwrap().iter() {
        match message["type"].as_str() {
            Some("reset") => {
                rows.clear();
                for item in message["items"].as_array().unwrap() {
                    rows.insert(item["uid"].as_str().unwrap().to_owned(), item.clone());
                }
            }
            Some("changes") => {
                for uid in message["deletes"].as_array().unwrap() {
                    rows.remove(uid.as_str().unwrap());
                }
                for item in message["upserts"].as_array().unwrap() {
                    rows.insert(item["uid"].as_str().unwrap().to_owned(), item.clone());
                }
            }
            _ => {}
        }
    }
    rows
}

async fn wait_for(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    println!("  ok: {what} ({:.1}s)", start.elapsed().as_secs_f32());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a disposable cluster: PORTHOLE_E2E_CONTEXT=<context>"]
async fn e2e_backend_against_a_cluster() {
    let ctx = std::env::var("PORTHOLE_E2E_CONTEXT").expect("set PORTHOLE_E2E_CONTEXT");
    let clusters = Clusters::default();
    let client = clusters.client(&ctx).await.unwrap();

    println!("discovery");
    let types = clusters.resources(&ctx, false).await.unwrap();
    assert!(
        types
            .iter()
            .any(|t| t.resource.kind == "Deployment" && t.resource.group == "apps")
    );
    let pods_type: ResourceType = resolve(&clusters, &ctx, &GroupVersionKind::gvk("", "v1", "Pod")).await;
    assert!(pods_type.namespaced);

    println!("version and node metrics");
    let version = client.apiserver_version().await.unwrap().git_version;
    assert!(version.starts_with("v1."), "{version}");
    let usage = crate::metrics::node_usage(client.clone()).await.unwrap();
    assert!(
        !usage.is_empty() && usage.iter().all(|n| n.cpu > 0.0 && n.memory > 0.0),
        "{usage:?}"
    );
    println!(
        "  ok: {version}, {} node(s), first uses {:.0}m CPU",
        usage.len(),
        usage[0].cpu
    );
    let pods_usage = crate::metrics::pod_usage(client.clone(), Some("kube-system"))
        .await
        .unwrap();
    assert!(
        pods_usage.iter().any(|p| p.memory > 0.0 && !p.containers.is_empty()),
        "{pods_usage:?}"
    );
    println!("  ok: {} kube-system pods with usage", pods_usage.len());

    println!("set up test objects (the app itself cannot create resources)");
    for doc in MANIFESTS.split("\n---\n") {
        let object: DynamicObject = serde_saphyr::from_str(doc).unwrap();
        let gvk = GroupVersionKind::try_from(object.types.as_ref().unwrap()).unwrap();
        let resource = resolve(&clusters, &ctx, &gvk).await;
        let api = resource.api(client.clone(), object.metadata.namespace.as_deref());
        let name = object.metadata.name.clone().unwrap();
        api.patch(&name, &PatchParams::apply("porthole-e2e"), &Patch::Apply(&object))
            .await
            .unwrap();
    }
    println!("  ok: namespace, deployment and crashing pod applied");

    println!("watch pods with kubectl statuses");
    let (channel, inbox) = json_channel();
    let pods_watch = tokio::spawn(watch::stream_rows(
        "e2e/pods".into(),
        pods_type.api(client.clone(), Some(NS)),
        watcher::Config::default(),
        |obj| summary::summarize("Pod", obj, false),
        channel,
    ));
    wait_for("both web pods running and ready", Duration::from_secs(120), || {
        let rows = current_rows(&inbox);
        rows.values()
            .filter(|r| r["owner"]["kind"] == "ReplicaSet" && r["health"] == "ok")
            .count()
            == 2
    })
    .await;
    wait_for("crasher reported as failing", Duration::from_secs(90), || {
        current_rows(&inbox).values().any(|r| {
            r["name"] == "crasher"
                && r["health"] == "failed"
                && matches!(r["status"].as_str(), Some("CrashLoopBackOff" | "Error"))
        })
    })
    .await;
    let web_pod = current_rows(&inbox)
        .values()
        .find(|r| r["owner"]["kind"] == "ReplicaSet")
        .map(|r| r["name"].as_str().unwrap().to_owned())
        .unwrap();

    println!("follow logs");
    let (channel, logs_inbox) = json_channel();
    let target = LogTarget {
        namespace: NS.into(),
        pod: web_pod.clone(),
        container: "web".into(),
    };
    let options = LogOptions {
        tail_lines: Some(5),
        previous: false,
        follow: true,
    };
    let logs_task = tokio::spawn(logs::stream_logs(client.clone(), vec![target], options, channel));
    wait_for("tick lines with kubelet timestamps", Duration::from_secs(30), || {
        logs_inbox.lock().unwrap().iter().any(|m| {
            m["type"] == "lines"
                && m["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|l| l["text"].as_str().unwrap_or_default().starts_with("tick-") && l["ts"].as_str().is_some())
        })
    })
    .await;
    logs_task.abort();

    println!("interactive shell");
    let output: Arc<Mutex<Vec<u8>>> = Arc::default();
    let sink = output.clone();
    let output_channel = Channel::new(move |body| {
        if let InvokeResponseBody::Raw(bytes) = body {
            sink.lock().unwrap().extend(bytes);
        }
        Ok(())
    });
    let (events_channel, exec_events) = json_channel();
    let (input, input_rx) = mpsc::unbounded_channel();
    let shell = tokio::spawn(exec::run_shell(
        Api::<Pod>::namespaced(client.clone(), NS),
        web_pod.clone(),
        "web".into(),
        TerminalSize { width: 100, height: 30 },
        input_rx,
        output_channel,
        events_channel,
    ));
    wait_for("shell connected", Duration::from_secs(20), || {
        exec_events.lock().unwrap().iter().any(|e| e["type"] == "connected")
    })
    .await;
    input.send(TerminalInput::Resize { cols: 120, rows: 40 }).unwrap();
    input
        .send(TerminalInput::Data(b"stty size; echo e2e-$((40+2))\n".to_vec()))
        .unwrap();
    wait_for("command output and resized tty", Duration::from_secs(20), || {
        let text = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
        text.contains("e2e-42") && text.contains("40 120")
    })
    .await;
    input.send(TerminalInput::Data(b"exit 7\n".to_vec())).unwrap();
    wait_for("exit code 7 reported", Duration::from_secs(20), || {
        exec_events
            .lock()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "exited" && e["code"] == 7)
    })
    .await;
    shell.await.unwrap();

    println!("YAML edit round trip");
    let deployments = resolve(&clusters, &ctx, &GroupVersionKind::gvk("apps", "v1", "Deployment")).await;
    let yaml = resources::get_yaml(client.clone(), &deployments, Some(NS), "web")
        .await
        .unwrap();
    assert!(
        yaml.contains("kind: Deployment") && !yaml.contains("managedFields"),
        "{yaml}"
    );
    let edited = yaml.replacen("replicas: 2", "replicas: 1", 1);
    assert_ne!(edited, yaml, "replicas line not found in:\n{yaml}");
    resources::replace(client.clone(), &deployments, Some(NS), "web", &edited)
        .await
        .unwrap();
    // The same stale document again must fail: its resourceVersion is now out of date.
    let stale = resources::replace(client.clone(), &deployments, Some(NS), "web", &edited).await;
    let message = stale.expect_err("a stale replace must conflict").to_string();
    assert!(message.contains("modified"), "unexpected conflict message: {message}");
    // Renaming in the editor must not create a copy.
    let renamed = edited.replacen("name: web", "name: web-copy", 1);
    let refused = resources::replace(client.clone(), &deployments, Some(NS), "web", &renamed).await;
    assert!(
        refused
            .expect_err("rename must be refused")
            .to_string()
            .contains("metadata.name must stay web")
    );
    println!("  ok: replace applied, stale replace rejected ({message})");
    wait_for("deployment scaled to one pod", Duration::from_secs(60), || {
        let rows = current_rows(&inbox);
        rows.values()
            .filter(|r| r["owner"]["kind"] == "ReplicaSet" && r["status"] != "Terminating")
            .count()
            == 1
    })
    .await;

    println!("delete");
    resources::delete(client.clone(), &pods_type, Some(NS), "crasher")
        .await
        .unwrap();
    wait_for("crasher gone from the watch", Duration::from_secs(60), || {
        !current_rows(&inbox).values().any(|r| r["name"] == "crasher")
    })
    .await;
    pods_watch.abort();
    let namespaces = resolve(&clusters, &ctx, &GroupVersionKind::gvk("", "v1", "Namespace")).await;
    resources::delete(client.clone(), &namespaces, None, NS).await.unwrap();
    println!("  ok: namespace {NS} deleting");
}
