mod clusters;
#[cfg(test)]
mod e2e;
mod error;
mod exec;
mod logs;
mod metrics;
mod pods;
mod quantity;
mod resources;
mod sessions;
mod shell_env;
mod summary;
mod watch;

use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{Api, TerminalSize},
    runtime::watcher,
};
use tauri::{
    State,
    ipc::{Channel, InvokeResponseBody},
};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use crate::{
    clusters::{Clusters, Contexts},
    error::{Error, Result},
    exec::{ExecEvent, TerminalInput},
    logs::{LogEvent, LogOptions, LogTarget},
    resources::{ResourceInfo, ResourceType},
    sessions::Sessions,
    summary::ResourceRow,
    watch::WatchEvent,
};

#[tauri::command]
async fn list_contexts() -> Result<Contexts> {
    clusters::list_contexts()
}

/// Stops every stream and forgets cached clients, then re-reads the kubeconfig.
#[tauri::command]
async fn reload_kubeconfig(clusters: State<'_, Clusters>, sessions: State<'_, Sessions>) -> Result<Contexts> {
    sessions.stop_all();
    clusters.reset().await;
    clusters::list_contexts()
}

/// The API server's version, e.g. `v1.37.0-eks-6c518f7`.
#[tauri::command]
async fn cluster_version(context: String, clusters: State<'_, Clusters>) -> Result<String> {
    Ok(clusters.client(&context).await?.apiserver_version().await?.git_version)
}

#[tauri::command]
async fn node_usage(context: String, clusters: State<'_, Clusters>) -> Result<Vec<metrics::NodeUsage>> {
    metrics::node_usage(clusters.client(&context).await?).await
}

#[tauri::command]
async fn pod_usage(
    context: String,
    namespace: Option<String>,
    clusters: State<'_, Clusters>,
) -> Result<Vec<metrics::PodUsage>> {
    metrics::pod_usage(clusters.client(&context).await?, namespace.as_deref()).await
}

#[tauri::command]
async fn list_resources(context: String, refresh: bool, clusters: State<'_, Clusters>) -> Result<Vec<ResourceInfo>> {
    Ok(clusters.resources(&context, refresh).await?.as_ref().clone())
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchOptions {
    /// None watches every namespace.
    namespace: Option<String>,
    field_selector: Option<String>,
    /// Include annotations; meant for single-object watches.
    #[serde(default)]
    detail: bool,
}

#[tauri::command]
async fn watch_resources(
    context: String,
    resource: ResourceType,
    options: WatchOptions,
    channel: Channel<WatchEvent<ResourceRow>>,
    clusters: State<'_, Clusters>,
    sessions: State<'_, Sessions>,
) -> Result<u64> {
    let api = resource.api(clusters.client(&context).await?, options.namespace.as_deref());
    let mut config = watcher::Config::default();
    if let Some(selector) = &options.field_selector {
        config = config.fields(selector);
    }
    let label = format!(
        "{}/{} {}",
        context,
        resource.plural,
        options.namespace.as_deref().unwrap_or("*")
    );
    let kind = resource.kind;
    let detail = options.detail;
    let rows = watch::stream_rows(
        label,
        api,
        config,
        move |obj| summary::summarize(&kind, obj, detail),
        channel,
    );
    Ok(sessions.spawn(rows, None))
}

#[tauri::command]
fn stop_session(id: u64, sessions: State<'_, Sessions>) {
    sessions.stop(id);
}

/// The webview calls this on load: streams from before a reload have no listener left.
#[tauri::command]
fn stop_all_sessions(sessions: State<'_, Sessions>) {
    sessions.stop_all();
}

#[tauri::command]
async fn get_yaml(
    context: String,
    resource: ResourceType,
    namespace: Option<String>,
    name: String,
    clusters: State<'_, Clusters>,
) -> Result<String> {
    resources::get_yaml(clusters.client(&context).await?, &resource, namespace.as_deref(), &name).await
}

/// Saves edited YAML over the object it was loaded from. The app never creates resources:
/// see `resources::parse_edit` for what a save may change.
#[tauri::command]
async fn save_yaml(
    context: String,
    resource: ResourceType,
    namespace: Option<String>,
    name: String,
    yaml: String,
    clusters: State<'_, Clusters>,
) -> Result<()> {
    let client = clusters.client(&context).await?;
    resources::replace(client, &resource, namespace.as_deref(), &name, &yaml).await
}

#[tauri::command]
async fn delete_resource(
    context: String,
    resource: ResourceType,
    namespace: Option<String>,
    name: String,
    clusters: State<'_, Clusters>,
) -> Result<()> {
    resources::delete(clusters.client(&context).await?, &resource, namespace.as_deref(), &name).await
}

#[tauri::command]
async fn stream_logs(
    context: String,
    targets: Vec<LogTarget>,
    options: LogOptions,
    channel: Channel<LogEvent>,
    clusters: State<'_, Clusters>,
    sessions: State<'_, Sessions>,
) -> Result<u64> {
    let client = clusters.client(&context).await?;
    Ok(sessions.spawn(logs::stream_logs(client, targets, options, channel), None))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn start_shell(
    context: String,
    namespace: String,
    pod: String,
    container: String,
    cols: u16,
    rows: u16,
    output: Channel<InvokeResponseBody>,
    events: Channel<ExecEvent>,
    clusters: State<'_, Clusters>,
    sessions: State<'_, Sessions>,
) -> Result<u64> {
    let api: Api<Pod> = Api::namespaced(clusters.client(&context).await?, &namespace);
    let (input, input_rx) = mpsc::unbounded_channel();
    let size = TerminalSize {
        width: cols,
        height: rows,
    };
    let shell = exec::run_shell(api, pod, container, size, input_rx, output, events);
    Ok(sessions.spawn(shell, Some(input)))
}

#[tauri::command]
fn shell_input(id: u64, data: String, sessions: State<'_, Sessions>) -> Result<()> {
    send_input(&sessions, id, TerminalInput::Data(data.into_bytes()))
}

#[tauri::command]
fn shell_resize(id: u64, cols: u16, rows: u16, sessions: State<'_, Sessions>) -> Result<()> {
    send_input(&sessions, id, TerminalInput::Resize { cols, rows })
}

fn send_input(sessions: &Sessions, id: u64, input: TerminalInput) -> Result<()> {
    let sender = sessions.input(id).ok_or(Error::SessionGone(id))?;
    sender.send(input).map_err(|_| Error::SessionGone(id))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    shell_env::import_login_shell_env();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("porthole_lib=info,kube=warn")),
        )
        .init();

    tauri::Builder::default()
        .manage(Clusters::default())
        .manage(Sessions::default())
        .invoke_handler(tauri::generate_handler![
            list_contexts,
            reload_kubeconfig,
            list_resources,
            cluster_version,
            node_usage,
            pod_usage,
            watch_resources,
            stop_session,
            stop_all_sessions,
            get_yaml,
            save_yaml,
            delete_resource,
            stream_logs,
            start_shell,
            shell_input,
            shell_resize,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
