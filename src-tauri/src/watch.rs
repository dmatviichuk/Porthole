use std::{collections::HashMap, fmt::Debug, time::Duration};

use futures::StreamExt;
use kube::{
    Api, Resource, ResourceExt,
    runtime::{WatchStreamExt, watcher},
};
use serde::{Serialize, de::DeserializeOwned};
use tauri::ipc::Channel;
use tokio::time::MissedTickBehavior;

/// How often buffered changes are flushed to the UI. Busy clusters emit hundreds of
/// events a second; one message per tick keeps the webview responsive.
const FLUSH_EVERY: Duration = Duration::from_millis(150);

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WatchEvent<R> {
    /// The complete current state. Sent after the initial list and after every relist.
    Reset { items: Vec<R> },
    /// Changes since the previous message, keyed by object uid.
    Changes { upserts: Vec<R>, deletes: Vec<String> },
    /// The watch failed; it retries with backoff and the next Reset/Changes means it recovered.
    Error { message: String },
}

/// Watches `api` and streams summarised rows to the frontend until the channel closes
/// or the session is aborted.
pub async fn stream_rows<K, R>(
    label: String,
    api: Api<K>,
    config: watcher::Config,
    summarize: impl Fn(&K) -> R,
    channel: Channel<WatchEvent<R>>,
) where
    K: Resource + Clone + DeserializeOwned + Debug + Send + 'static,
    R: Serialize,
{
    let started = std::time::Instant::now();
    let mut first_list = true;
    let stream = watcher(api, config).default_backoff();
    let mut stream = std::pin::pin!(stream);
    let mut tick = tokio::time::interval(FLUSH_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut initial: Option<Vec<R>> = None;
    // uid -> Some(row) for an upsert, None for a delete; later events overwrite earlier ones.
    let mut pending: HashMap<String, Option<R>> = HashMap::new();

    loop {
        let message = tokio::select! {
            event = stream.next() => match event {
                None => break,
                Some(Ok(watcher::Event::Init)) => {
                    initial = Some(Vec::new());
                    continue;
                }
                Some(Ok(watcher::Event::InitApply(obj))) => {
                    initial.get_or_insert_default().push(summarize(&obj));
                    continue;
                }
                Some(Ok(watcher::Event::InitDone)) => {
                    if first_list {
                        first_list = false;
                        let rows = initial.as_ref().map_or(0, Vec::len);
                        tracing::info!(watch = %label, rows, elapsed_ms = started.elapsed().as_millis() as u64, "listed");
                    }
                    pending.clear();
                    WatchEvent::Reset { items: initial.take().unwrap_or_default() }
                }
                Some(Ok(watcher::Event::Apply(obj))) => {
                    pending.insert(obj.uid().unwrap_or_default(), Some(summarize(&obj)));
                    continue;
                }
                Some(Ok(watcher::Event::Delete(obj))) => {
                    pending.insert(obj.uid().unwrap_or_default(), None);
                    continue;
                }
                Some(Err(err)) => WatchEvent::Error { message: err.to_string() },
            },
            _ = tick.tick() => {
                if pending.is_empty() {
                    continue;
                }
                let mut upserts = Vec::new();
                let mut deletes = Vec::new();
                for (uid, row) in pending.drain() {
                    match row {
                        Some(row) => upserts.push(row),
                        None => deletes.push(uid),
                    }
                }
                WatchEvent::Changes { upserts, deletes }
            }
        };
        if channel.send(message).is_err() {
            break;
        }
    }
}
