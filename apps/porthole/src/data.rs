//! Shared watches, discovery, metrics, logs and shells, delivered directly to egui.
use crate::{
    channel::Channel,
    clusters::{Clusters, Contexts},
    exec::{self, ExecEvent, TerminalInput},
    logs::{self, LogEvent, LogOptions, LogTarget},
    metrics::{self, NodeUsage, PodUsage},
    model,
    resources::{self, ResourceInfo, ResourceType},
    sessions::Sessions,
    summary::{self, ResourceRow},
    watch::{self, WatchEvent},
};
use eframe::egui;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, api::TerminalSize, runtime::watcher};
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

pub enum Message {
    Watch(String, WatchEvent<ResourceRow>),
    Discovery(String, Result<Vec<ResourceInfo>, String>),
    Nodes(String, Result<Vec<NodeUsage>, String>),
    Pods(String, Result<Vec<PodUsage>, String>),
    Version(String, String),
    Yaml(u64, Result<String, String>),
    Saved(u64, Result<(), String>),
    Deleted(String, Result<(), String>),
    Log(u64, String, LogEvent),
    ShellBytes(u64, Vec<u8>),
    ShellEvent(u64, ExecEvent),
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub rows: Vec<ResourceRow>,
    pub synced: bool,
    pub error: Option<String>,
}
struct Watch {
    snapshot: Snapshot,
    session: u64,
    last_used: Instant,
    last_saved: Option<Instant>,
    persistent: bool,
    warm: bool,
}
#[derive(Default)]
pub struct ContextData {
    pub types: Vec<ResourceInfo>,
    pub discovery_error: Option<String>,
    pub nodes: Option<Vec<NodeUsage>>,
    pub pods: Option<Vec<PodUsage>>,
    pub node_error: Option<String>,
    pub pod_error: Option<String>,
    pub version: Option<String>,
}
pub struct Data {
    pub clusters: Arc<Clusters>,
    pub sessions: Arc<Sessions>,
    pub contexts: Result<Contexts, String>,
    pub context_data: HashMap<String, ContextData>,
    pub pending: Vec<Message>,
    tx: mpsc::Sender<(u64, Message)>,
    rx: mpsc::Receiver<(u64, Message)>,
    generation: u64,
    ctx: egui::Context,
    watches: HashMap<String, Watch>,
    warmed: HashMap<String, (Instant, Vec<u64>)>,
    selected: String,
    cache: PathBuf,
}
impl Data {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let cache = directories::ProjectDirs::from("dev", "dmatviichuk", "Porthole")
            .map(|d| d.cache_dir().to_owned())
            .unwrap_or_else(std::env::temp_dir)
            .join("snapshots");
        Self {
            clusters: Arc::default(),
            sessions: Arc::default(),
            contexts: crate::clusters::list_contexts().map_err(|e| e.to_string()),
            context_data: HashMap::new(),
            pending: vec![],
            tx,
            rx,
            generation: 0,
            ctx,
            watches: HashMap::new(),
            warmed: HashMap::new(),
            selected: String::new(),
            cache,
        }
    }
    pub fn channel<T: Send + 'static>(&self, map: impl Fn(T) -> Message + Send + Sync + 'static) -> Channel<T> {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let generation = self.generation;
        Channel::new(move |v| {
            tx.send((generation, map(v))).map_err(|_| ())?;
            ctx.request_repaint();
            Ok(())
        })
    }
    pub fn drain(&mut self) {
        while let Ok((generation, msg)) = self.rx.try_recv() {
            if generation != self.generation {
                continue;
            }
            match msg {
                Message::Watch(key, event) => {
                    if let Some(w) = self.watches.get_mut(&key) {
                        match event {
                            WatchEvent::Reset { items } => {
                                w.snapshot.rows = items;
                                w.snapshot.synced = true;
                                w.snapshot.error = None;
                            }
                            WatchEvent::Changes { upserts, deletes } => {
                                let remove: HashSet<_> = deletes.into_iter().collect();
                                w.snapshot.rows.retain(|r| !remove.contains(&r.uid));
                                let mut by_uid: HashMap<_, _> = w
                                    .snapshot
                                    .rows
                                    .iter()
                                    .enumerate()
                                    .map(|(i, r)| (r.uid.clone(), i))
                                    .collect();
                                for row in upserts {
                                    if let Some(&i) = by_uid.get(&row.uid) {
                                        w.snapshot.rows[i] = row;
                                    } else {
                                        by_uid.insert(row.uid.clone(), w.snapshot.rows.len());
                                        w.snapshot.rows.push(row);
                                    }
                                }
                                w.snapshot.error = None;
                            }
                            WatchEvent::Error { message } => {
                                w.snapshot.error = Some(message);
                                continue;
                            }
                        }
                        if w.persistent
                            && w.snapshot.synced
                            && w.snapshot.rows.len() <= 20000
                            && w.last_saved.is_none_or(|t| t.elapsed() >= Duration::from_secs(5))
                        {
                            w.last_saved = Some(Instant::now());
                            let rows = w.snapshot.rows.clone();
                            let file = cache_path(&self.cache, &key);
                            tokio::task::spawn_blocking(move || write_json(&file, &rows));
                        }
                    }
                }
                Message::Discovery(c, result) => {
                    let d = self.context_data.entry(c).or_default();
                    match result {
                        Ok(types) => {
                            d.types = types;
                            d.discovery_error = None;
                        }
                        Err(e) => d.discovery_error = Some(e),
                    }
                }
                Message::Nodes(c, result) => {
                    let d = self.context_data.entry(c).or_default();
                    match result {
                        Ok(v) => {
                            d.nodes = Some(v);
                            d.node_error = None;
                        }
                        Err(e) => {
                            d.nodes = None;
                            d.node_error = Some(e);
                        }
                    }
                }
                Message::Pods(c, result) => {
                    let d = self.context_data.entry(c).or_default();
                    match result {
                        Ok(v) => {
                            d.pods = Some(v);
                            d.pod_error = None;
                        }
                        Err(e) => {
                            d.pods = None;
                            d.pod_error = Some(e);
                        }
                    }
                }
                Message::Version(c, v) => {
                    self.context_data.entry(c).or_default().version = Some(v.trim_start_matches('v').into())
                }
                other => self.pending.push(other),
            }
        }
        let expired: Vec<_> = self
            .watches
            .iter()
            .filter(|(key, w)| {
                !(w.warm && key.starts_with(&format!("{}|", self.selected)))
                    && w.last_used.elapsed() > Duration::from_secs(600)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired {
            if let Some(w) = self.watches.remove(&key) {
                self.sessions.stop(w.session);
            }
        }
        let expired: Vec<_> = self
            .warmed
            .iter()
            .filter(|(c, (t, _))| **c != self.selected && t.elapsed() > Duration::from_secs(600))
            .map(|(c, _)| c.clone())
            .collect();
        for c in expired {
            if let Some((_, ids)) = self.warmed.remove(&c) {
                for id in ids {
                    self.sessions.stop(id);
                }
            }
        }
    }
    pub fn warm(&mut self, context: &str) {
        self.selected = context.into();
        if let Some((t, _)) = self.warmed.get_mut(context) {
            *t = Instant::now();
            return;
        }
        self.context_data.entry(context.into()).or_default();
        for kind in model::WARM {
            self.rows(context, &model::resource(kind), None, None, false);
        }
        self.discover(context, false);
        let mut ids = vec![];
        for nodes in [true, false] {
            let clusters = self.clusters.clone();
            let c = context.to_owned();
            let channel = self.channel(move |r: Result<Metrics, String>| match r {
                Ok(Metrics::Nodes(v)) => Message::Nodes(c.clone(), Ok(v)),
                Ok(Metrics::Pods(v)) => Message::Pods(c.clone(), Ok(v)),
                Err(e) => {
                    if nodes {
                        Message::Nodes(c.clone(), Err(e))
                    } else {
                        Message::Pods(c.clone(), Err(e))
                    }
                }
            });
            let c = context.to_owned();
            ids.push(self.sessions.spawn(
                async move {
                    let mut timer = tokio::time::interval(Duration::from_secs(15));
                    loop {
                        timer.tick().await;
                        let result = async {
                            let client = clusters.client(&c).await?;
                            if nodes {
                                metrics::node_usage(client).await.map(Metrics::Nodes)
                            } else {
                                metrics::pod_usage(client, None).await.map(Metrics::Pods)
                            }
                        }
                        .await
                        .map_err(|e| e.to_string());
                        if channel.send(result).is_err() {
                            break;
                        }
                    }
                },
                None,
            ));
        }
        let clusters = self.clusters.clone();
        let c = context.to_owned();
        let channel = self.channel(move |v| Message::Version(c.clone(), v));
        let c = context.to_owned();
        tokio::spawn(async move {
            if let Ok(client) = clusters.client(&c).await
                && let Ok(v) = client.apiserver_version().await
            {
                let _ = channel.send(v.git_version);
            }
        });
        self.warmed.insert(context.into(), (Instant::now(), ids));
    }
    pub fn discover(&mut self, context: &str, refresh: bool) {
        let file = cache_path(&self.cache, &format!("discovery|{context}"));
        if !refresh
            && let Ok(bytes) = std::fs::read(&file)
            && let Ok(types) = serde_json::from_slice(&bytes)
        {
            self.context_data.entry(context.into()).or_default().types = types;
        }
        let clusters = self.clusters.clone();
        let c = context.to_owned();
        let channel = self.channel(move |r| Message::Discovery(c.clone(), r));
        let c = context.to_owned();
        tokio::spawn(async move {
            let r = clusters
                .resources(&c, refresh)
                .await
                .map(|r| r.as_ref().clone())
                .map_err(|e| e.to_string());
            if let Ok(types) = &r {
                write_json(&file, types);
            }
            let _ = channel.send(r);
        });
    }
    pub fn rows(
        &mut self,
        context: &str,
        r: &ResourceType,
        ns: Option<&str>,
        selector: Option<&str>,
        detail: bool,
    ) -> Snapshot {
        if context.is_empty() {
            return Snapshot::default();
        }
        let scoped = selector.is_some() || detail;
        let namespace = if scoped && r.namespaced { ns } else { None };
        let key = format!(
            "{}|{}|{}|{}|{}|{}|{}",
            context,
            r.group,
            r.version,
            r.plural,
            namespace.unwrap_or("*"),
            selector.unwrap_or_default(),
            detail
        );
        self.ensure_watch(&key, context, r, namespace, selector, detail);
        let mut snapshot = self.watches.get(&key).unwrap().snapshot.clone();
        if !scoped
            && r.namespaced
            && let Some(ns) = ns
        {
            if snapshot
                .error
                .as_ref()
                .is_some_and(|e| e.to_lowercase().contains("forbidden"))
            {
                let key = format!("{}|{}|{}|{}|{}||false", context, r.group, r.version, r.plural, ns);
                self.ensure_watch(&key, context, r, Some(ns), None, false);
                snapshot = self.watches[&key].snapshot.clone();
            } else {
                snapshot.rows.retain(|r| r.namespace.as_deref() == Some(ns));
            }
        }
        snapshot
    }
    fn ensure_watch(
        &mut self,
        key: &str,
        context: &str,
        r: &ResourceType,
        namespace: Option<&str>,
        selector: Option<&str>,
        detail: bool,
    ) {
        if let Some(w) = self.watches.get_mut(key) {
            w.last_used = Instant::now();
            return;
        }
        let persistent = namespace.is_none() && selector.is_none() && !detail;
        let warm = persistent && model::WARM.iter().any(|kind| model::resource(kind) == *r);
        let mut snapshot = Snapshot::default();
        if persistent && let Ok(bytes) = std::fs::read(cache_path(&self.cache, key)) {
            snapshot.rows = serde_json::from_slice(&bytes).unwrap_or_default();
        }
        let clusters = self.clusters.clone();
        let c = context.to_owned();
        let r = r.clone();
        let ns = namespace.map(str::to_owned);
        let selector = selector.map(str::to_owned);
        let k = key.to_owned();
        let channel = self.channel(move |v| Message::Watch(k.clone(), v));
        let session = self.sessions.spawn(
            async move {
                match clusters.client(&c).await {
                    Ok(client) => {
                        let api = r.api(client, ns.as_deref());
                        let mut config = watcher::Config::default();
                        if let Some(s) = selector {
                            config = config.fields(&s);
                        }
                        let kind = r.kind;
                        watch::stream_rows(
                            format!("{c}/{}", r.plural),
                            api,
                            config,
                            move |obj| summary::summarize(&kind, obj, detail),
                            channel,
                        )
                        .await;
                    }
                    Err(e) => {
                        let _ = channel.send(WatchEvent::Error { message: e.to_string() });
                    }
                }
            },
            None,
        );
        self.watches.insert(
            key.into(),
            Watch {
                snapshot,
                session,
                last_used: Instant::now(),
                last_saved: None,
                persistent,
                warm,
            },
        );
    }
    pub fn reload(&mut self) {
        self.generation += 1;
        self.sessions.stop_all();
        self.watches.clear();
        self.warmed.clear();
        self.context_data.clear();
        self.clusters = Arc::default();
        self.contexts = crate::clusters::list_contexts().map_err(|e| e.to_string());
    }
    pub fn yaml(&self, token: u64, c: String, r: ResourceType, ns: Option<String>, name: String) {
        let clusters = self.clusters.clone();
        let channel = self.channel(move |v| Message::Yaml(token, v));
        tokio::spawn(async move {
            let r = async { resources::get_yaml(clusters.client(&c).await?, &r, ns.as_deref(), &name).await }
                .await
                .map_err(|e| e.to_string());
            let _ = channel.send(r);
        });
    }
    pub fn save_yaml(&self, token: u64, c: String, r: ResourceType, ns: Option<String>, name: String, yaml: String) {
        let clusters = self.clusters.clone();
        let channel = self.channel(move |v| Message::Saved(token, v));
        tokio::spawn(async move {
            let r = async { resources::replace(clusters.client(&c).await?, &r, ns.as_deref(), &name, &yaml).await }
                .await
                .map_err(|e| e.to_string());
            let _ = channel.send(r);
        });
    }
    pub fn delete(&self, c: String, r: ResourceType, ns: Option<String>, name: String) {
        let clusters = self.clusters.clone();
        let label = format!("{} {}", r.kind.to_lowercase(), name);
        let channel = self.channel(move |v| Message::Deleted(label.clone(), v));
        tokio::spawn(async move {
            let r = async { resources::delete(clusters.client(&c).await?, &r, ns.as_deref(), &name).await }
                .await
                .map_err(|e| e.to_string());
            let _ = channel.send(r);
        });
    }
    pub fn logs(&self, token: u64, context: String, target: LogTarget, tail: i64) -> u64 {
        let clusters = self.clusters.clone();
        let key = format!("{}/{}/{}", target.namespace, target.pod, target.container);
        let channel = self.channel(move |v| Message::Log(token, key.clone(), v));
        self.sessions.spawn(
            async move {
                match clusters.client(&context).await {
                    Ok(client) => {
                        logs::stream_logs(
                            client,
                            vec![target],
                            LogOptions {
                                tail_lines: Some(tail),
                                previous: false,
                                follow: true,
                            },
                            channel,
                        )
                        .await
                    }
                    Err(e) => {
                        let _ = channel.send(LogEvent::Error {
                            source: 0,
                            message: e.to_string(),
                        });
                    }
                }
            },
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn shell(
        &self,
        key: u64,
        context: String,
        namespace: String,
        pod: String,
        container: String,
        cols: u16,
        rows: u16,
    ) -> u64 {
        let clusters = self.clusters.clone();
        let (input, rx) = tokio::sync::mpsc::unbounded_channel();
        let output = self.channel(move |v| Message::ShellBytes(key, v));
        let events = self.channel(move |v| Message::ShellEvent(key, v));
        self.sessions.spawn(
            async move {
                match clusters.client(&context).await {
                    Ok(client) => {
                        exec::run_shell(
                            Api::<Pod>::namespaced(client, &namespace),
                            pod,
                            container,
                            TerminalSize {
                                width: cols,
                                height: rows,
                            },
                            rx,
                            output,
                            events,
                        )
                        .await
                    }
                    Err(e) => {
                        let _ = events.send(ExecEvent::Error { message: e.to_string() });
                    }
                }
            },
            Some(input),
        )
    }
    pub fn shell_input(&self, id: u64, bytes: Vec<u8>) -> crate::error::Result<()> {
        self.sessions
            .input(id)
            .ok_or(crate::error::Error::SessionGone(id))?
            .send(TerminalInput::Data(bytes))
            .map_err(|_| crate::error::Error::SessionGone(id))
    }
    pub fn resize(&self, id: u64, cols: u16, rows: u16) -> crate::error::Result<()> {
        self.sessions
            .input(id)
            .ok_or(crate::error::Error::SessionGone(id))?
            .send(TerminalInput::Resize { cols, rows })
            .map_err(|_| crate::error::Error::SessionGone(id))
    }
}
enum Metrics {
    Nodes(Vec<NodeUsage>),
    Pods(Vec<PodUsage>),
}
fn cache_path(dir: &std::path::Path, key: &str) -> PathBuf {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    dir.join(format!("{:016x}.json", h.finish()))
}
fn write_json(file: &std::path::Path, value: &impl serde::Serialize) {
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec(value) {
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(tmp, file);
        }
    }
}
