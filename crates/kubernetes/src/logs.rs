use std::time::Duration;

use crate::channel::Channel;
use futures::{AsyncBufRead, AsyncBufReadExt, Stream, StreamExt, stream};
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client, api::LogParams};
use serde::{Deserialize, Serialize};
use tokio::time::MissedTickBehavior;

use crate::error::Error;

const FLUSH_EVERY: Duration = Duration::from_millis(100);
/// Flush early when a chatty container fills a batch before the next tick.
const MAX_BATCH: usize = 5_000;

#[derive(Debug, Clone, Deserialize)]
pub struct LogTarget {
    pub namespace: String,
    pub pod: String,
    pub container: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogOptions {
    pub tail_lines: Option<i64>,
    #[serde(default)]
    pub previous: bool,
    pub follow: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct LogLine {
    /// Index into the targets the session was started with.
    pub source: usize,
    /// RFC 3339 timestamp added by the kubelet.
    pub ts: Option<String>,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LogEvent {
    Lines {
        lines: Vec<LogLine>,
    },
    /// The source stopped: the container exited, the pod went away, or `follow` was off.
    Ended {
        source: usize,
    },
    Error {
        source: usize,
        message: String,
    },
}

enum Item {
    Line(usize, String),
    Ended(usize),
    Failed(usize, String),
}

/// Follows every target at once and merges their lines into one ordered-by-arrival stream.
pub async fn stream_logs(client: Client, targets: Vec<LogTarget>, options: LogOptions, channel: Channel<LogEvent>) {
    let sources = targets.into_iter().enumerate().map(|(source, target)| {
        let params = LogParams {
            container: Some(target.container),
            follow: options.follow,
            previous: options.previous,
            tail_lines: options.tail_lines,
            timestamps: true,
            ..Default::default()
        };
        let api = Api::<Pod>::namespaced(client.clone(), &target.namespace);
        source_stream(api, target.pod, params, source).boxed()
    });
    let mut merged = stream::select_all(sources);
    let mut tick = tokio::time::interval(FLUSH_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut batch = Vec::new();

    loop {
        let sent = tokio::select! {
            item = merged.next() => match item {
                None => break,
                Some(Item::Line(source, raw)) => {
                    batch.push(parse_line(source, &raw));
                    batch.len() < MAX_BATCH || flush(&channel, &mut batch)
                }
                Some(Item::Ended(source)) => {
                    flush(&channel, &mut batch) && channel.send(LogEvent::Ended { source }).is_ok()
                }
                Some(Item::Failed(source, message)) => {
                    flush(&channel, &mut batch) && channel.send(LogEvent::Error { source, message }).is_ok()
                }
            },
            _ = tick.tick() => flush(&channel, &mut batch),
        };
        if !sent {
            return;
        }
    }
    flush(&channel, &mut batch);
}

fn flush(channel: &Channel<LogEvent>, batch: &mut Vec<LogLine>) -> bool {
    batch.is_empty()
        || channel
            .send(LogEvent::Lines {
                lines: std::mem::take(batch),
            })
            .is_ok()
}

fn source_stream(api: Api<Pod>, pod: String, params: LogParams, source: usize) -> impl Stream<Item = Item> + Send {
    stream::once(async move { api.log_stream(&pod, &params).await }).flat_map(move |opened| match opened {
        Ok(reader) => read_lines(reader, source).boxed(),
        Err(err) => stream::iter([Item::Failed(source, Error::from(err).to_string())]).boxed(),
    })
}

/// Line reader that tolerates invalid UTF-8; `AsyncBufReadExt::lines` would end the stream on it.
fn read_lines<R>(reader: R, source: usize) -> impl Stream<Item = Item> + Send
where
    R: AsyncBufRead + Send + 'static,
{
    stream::unfold(Some(Box::pin(reader)), move |reader| async move {
        let mut reader = reader?;
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) => Some((Item::Ended(source), None)),
            Ok(_) => {
                while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                    buf.pop();
                }
                Some((
                    Item::Line(source, String::from_utf8_lossy(&buf).into_owned()),
                    Some(reader),
                ))
            }
            Err(err) => Some((Item::Failed(source, err.to_string()), None)),
        }
    })
}

fn parse_line(source: usize, raw: &str) -> LogLine {
    let (ts, text) = match raw.split_once(' ') {
        Some((ts, text)) if is_timestamp(ts) => (Some(ts), text),
        None if is_timestamp(raw) => (Some(raw), ""),
        _ => (None, raw),
    };
    LogLine {
        source,
        ts: ts.map(str::to_owned),
        text: text.to_owned(),
    }
}

/// The kubelet prefixes lines with RFC 3339 UTC timestamps, e.g. `2026-10-03T20:41:00.123456789Z`.
fn is_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 20 && b[4] == b'-' && b[10] == b'T' && s.ends_with('Z')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_kubelet_timestamp() {
        assert_eq!(
            parse_line(2, "2026-10-03T20:41:00.123456789Z GET /healthz 200"),
            LogLine {
                source: 2,
                ts: Some("2026-10-03T20:41:00.123456789Z".into()),
                text: "GET /healthz 200".into()
            }
        );
    }

    #[test]
    fn keeps_lines_without_timestamp_and_empty_messages() {
        assert_eq!(parse_line(0, "no timestamp here").ts, None);
        assert_eq!(parse_line(0, "no timestamp here").text, "no timestamp here");
        let empty = parse_line(0, "2026-10-03T20:41:00Z");
        assert_eq!(
            (empty.ts.as_deref(), empty.text.as_str()),
            (Some("2026-10-03T20:41:00Z"), "")
        );
    }

    #[tokio::test]
    async fn reads_lossy_lines_until_eof() {
        let input: &[u8] = b"one\r\ntw\xffo\nthree";
        let items: Vec<Item> = read_lines(futures::io::Cursor::new(input), 7).collect().await;
        let shown: Vec<String> = items
            .into_iter()
            .map(|item| match item {
                Item::Line(s, text) => format!("{s}:{text}"),
                Item::Ended(s) => format!("{s}:end"),
                Item::Failed(s, e) => format!("{s}:err {e}"),
            })
            .collect();
        assert_eq!(shown, ["7:one", "7:tw\u{fffd}o", "7:three", "7:end"]);
    }
}
