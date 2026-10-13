use std::{collections::HashMap, fmt::Debug, time::Duration};

use crate::channel::Channel;
use futures::StreamExt;
use kube::{
    Api, Resource, ResourceExt,
    runtime::{utils::Backoff, watcher},
};
use serde::{Serialize, de::DeserializeOwned};
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
    let mut stream = Box::pin(watcher(api.clone(), config.clone()));
    let mut backoff = watcher::DefaultBackoff::default();
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
                    backoff.reset();
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
                Some(Err(err)) => {
                    tracing::warn!(watch = %label, error = ?err, "watch failed; retrying");
                    if channel.send(WatchEvent::Error { message: error_message(&err) }).is_err() {
                        break;
                    }
                    // A resumed watch may reconnect without emitting any objects. Relist after
                    // errors so even an unchanged or empty collection clears its error banner.
                    // Discard buffered changes: the next complete list is authoritative.
                    initial = None;
                    pending.clear();
                    tokio::time::sleep(backoff.next().unwrap_or(Duration::from_secs(30))).await;
                    stream = Box::pin(watcher(api.clone(), config.clone()));
                    continue;
                }
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

fn error_message(error: &watcher::Error) -> String {
    let connection_error = matches!(
        error,
        watcher::Error::InitialListFailed(kube::Error::Service(_) | kube::Error::HyperError(_))
            | watcher::Error::WatchStartFailed(kube::Error::Service(_) | kube::Error::HyperError(_))
            | watcher::Error::WatchFailed(kube::Error::Service(_) | kube::Error::HyperError(_))
    );
    if connection_error {
        let mut cause: &dyn std::error::Error = error;
        while let Some(source) = cause.source() {
            cause = source;
        }
        format!("Could not connect to the Kubernetes API. Retrying automatically. {cause}")
    } else {
        format!("{error}. Retrying automatically.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::Pod;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    #[test]
    fn connection_errors_show_the_cause_and_automatic_retry() {
        let error = watcher::Error::WatchStartFailed(kube::Error::Service(Box::new(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Connection refused",
        ))));
        let message = error_message(&error);
        assert!(message.contains("Could not connect to the Kubernetes API"), "{message}");
        assert!(message.contains("Retrying automatically"), "{message}");
        assert!(message.contains("Connection refused"), "{message}");
        assert!(!message.contains("ServiceError"), "{message}");
    }

    #[tokio::test]
    async fn failed_watch_relists_and_recovers_without_any_object_changes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for failed in [false, true, false] {
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "timed out waiting for watch retry"
                            );
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buffer[..n]);
                }
                requests.push(String::from_utf8(request).unwrap());
                let (status, body) = if failed {
                    (
                        "503 Service Unavailable",
                        r#"{"kind":"Status","apiVersion":"v1","status":"Failure","reason":"ServiceUnavailable","message":"temporary outage","code":503}"#,
                    )
                } else {
                    (
                        "200 OK",
                        r#"{"kind":"PodList","apiVersion":"v1","metadata":{"resourceVersion":"7"},"items":[]}"#,
                    )
                };
                write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        let mut client_config = kube::Config::new(format!("http://{address}").parse().unwrap());
        // Exercise watch recovery after the HTTP client's own retries are exhausted.
        client_config.default_retry = false;
        let client = kube::Client::try_from(client_config).unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let resets = std::sync::atomic::AtomicUsize::new(0);
        let task = tokio::spawn(stream_rows(
            "test/pods".into(),
            Api::<Pod>::all(client),
            watcher::Config::default().fields("metadata.name=unchanged"),
            |pod| pod.name_any(),
            Channel::new(move |event| {
                let done = matches!(&event, WatchEvent::Reset { .. })
                    && resets.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 1;
                tx.send(event).map_err(|_| ())?;
                if done { Err(()) } else { Ok(()) }
            }),
        ));
        tokio::time::timeout(Duration::from_secs(15), async {
            assert!(matches!(rx.recv().await.unwrap(), WatchEvent::Reset { items } if items.is_empty()));
            let event = rx.recv().await.unwrap();
            assert!(
                matches!(&event, WatchEvent::Error { message } if message.contains("temporary outage")),
                "{event:?}"
            );
            assert!(matches!(rx.recv().await.unwrap(), WatchEvent::Reset { items } if items.is_empty()));
            task.await.unwrap();
        })
        .await
        .unwrap();
        let requests = server.join().unwrap();
        assert!(!requests[0].lines().next().unwrap().contains("watch=true"));
        assert!(requests[1].lines().next().unwrap().contains("watch=true"));
        assert!(!requests[2].lines().next().unwrap().contains("watch=true"));
        assert!(
            requests
                .iter()
                .all(|r| r.lines().next().unwrap().contains("fieldSelector="))
        );
    }
}
