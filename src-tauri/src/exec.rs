use std::time::Duration;

use futures::SinkExt;
use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::Status};
use kube::api::{Api, AttachParams, TerminalSize};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc::UnboundedReceiver,
};

use crate::error::Error;

/// Prefer bash, fall back to sh. TERM matches xterm.js so colours and line editing work.
const SHELL: [&str; 3] = [
    "/bin/sh",
    "-c",
    "export TERM=xterm-256color; if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi",
];

pub enum TerminalInput {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExecEvent {
    Connected,
    Exited { code: Option<i32>, message: Option<String> },
    Error { message: String },
}

/// Runs an interactive shell. Terminal output goes to `output` as raw bytes, lifecycle to `events`.
/// Dropping this future (session stop) drops the websocket, which hangs up the remote shell.
pub async fn run_shell(
    api: Api<Pod>,
    pod: String,
    container: String,
    size: TerminalSize,
    mut input: UnboundedReceiver<TerminalInput>,
    output: Channel<InvokeResponseBody>,
    events: Channel<ExecEvent>,
) {
    let params = AttachParams::interactive_tty().container(container);
    let mut process = match api.exec(&pod, SHELL, &params).await {
        Ok(process) => process,
        Err(err) => {
            let _ = events.send(ExecEvent::Error {
                message: Error::from(err).to_string(),
            });
            return;
        }
    };
    let (Some(mut stdin), Some(mut stdout), Some(mut resize), Some(status)) = (
        process.stdin(),
        process.stdout(),
        process.terminal_size(),
        process.take_status(),
    ) else {
        let _ = events.send(ExecEvent::Error {
            message: "exec session is missing a stream".into(),
        });
        return;
    };
    let _ = events.send(ExecEvent::Connected);
    let _ = resize.send(size).await;

    let mut buf = vec![0u8; 32 * 1024];
    loop {
        tokio::select! {
            read = stdout.read(&mut buf) => match read {
                Ok(0) => break,
                Ok(n) => {
                    if output.send(InvokeResponseBody::Raw(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
                Err(err) => {
                    let _ = events.send(ExecEvent::Error { message: err.to_string() });
                    break;
                }
            },
            message = input.recv() => match message {
                Some(TerminalInput::Data(bytes)) => {
                    if stdin.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Some(TerminalInput::Resize { cols, rows }) => {
                    let _ = resize.send(TerminalSize { width: cols, height: rows }).await;
                }
                None => break,
            },
        }
    }

    let status = tokio::time::timeout(Duration::from_secs(2), status)
        .await
        .ok()
        .flatten();
    let (code, message) = exit_status(status);
    let _ = events.send(ExecEvent::Exited { code, message });
}

/// Exec reports the exit code inside a Status cause; other failures (no shell in a
/// distroless image, for example) only carry a message.
fn exit_status(status: Option<Status>) -> (Option<i32>, Option<String>) {
    let Some(status) = status else { return (None, None) };
    if status.status.as_deref() == Some("Success") {
        return (Some(0), None);
    }
    let code = status
        .details
        .as_ref()
        .and_then(|d| d.causes.as_deref())
        .and_then(|causes| causes.iter().find(|c| c.reason.as_deref() == Some("ExitCode")))
        .and_then(|cause| cause.message.as_deref()?.parse().ok());
    match code {
        Some(code) => (Some(code), None),
        None => (None, status.message),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_exit_codes_from_status() {
        let ok: Status = serde_json::from_value(json!({ "status": "Success" })).unwrap();
        let failed: Status = serde_json::from_value(json!({
            "status": "Failure", "reason": "NonZeroExitCode",
            "message": "command terminated with non-zero exit code: exit status 130",
            "details": { "causes": [{ "reason": "ExitCode", "message": "130" }] }
        }))
        .unwrap();
        let no_shell: Status = serde_json::from_value(json!({
            "status": "Failure", "reason": "InternalError",
            "message": "exec: \"/bin/sh\": stat /bin/sh: no such file or directory"
        }))
        .unwrap();
        assert_eq!(exit_status(Some(ok)), (Some(0), None));
        assert_eq!(exit_status(Some(failed)), (Some(130), None));
        assert_eq!(
            exit_status(Some(no_shell)),
            (
                None,
                Some("exec: \"/bin/sh\": stat /bin/sh: no such file or directory".into())
            )
        );
        assert_eq!(exit_status(None), (None, None));
    }
}
