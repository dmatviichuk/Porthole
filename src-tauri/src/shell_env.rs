use std::{
    env,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

const MARKER: &str = "__PORTHOLE_ENV__";
/// Variables that describe the probe shell itself rather than the user's environment.
const SKIP: [&str; 5] = ["PWD", "OLDPWD", "SHLVL", "_", "TERM_SESSION_ID"];

/// Apps opened from Finder or the Dock get launchd's minimal environment, so kubeconfig
/// exec plugins (`aws`, `kubelogin`, `gke-gcloud-auth-plugin`) are not on PATH and
/// `KUBECONFIG` is unset. Borrow both from the user's login shell, as kubectl would see them.
///
/// Must run before any other thread starts: it calls `env::set_var`.
pub fn import_login_shell_env() {
    // A terminal launch (e.g. `pnpm tauri dev`) already has the full environment.
    if env::var_os("SHLVL").is_some() {
        return;
    }
    let Some(vars) = read_login_shell_env() else { return };
    for (key, value) in vars {
        if SKIP.contains(&key.as_str()) {
            continue;
        }
        if key == "PATH" || env::var_os(&key).is_none() {
            // SAFETY: called from `main` before Tauri, tokio or any other thread starts.
            unsafe { env::set_var(key, value) };
        }
    }
}

fn read_login_shell_env() -> Option<Vec<(String, String)>> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let (tx, rx) = mpsc::channel();
    // A slow rc file must not block startup forever; give up after a few seconds.
    thread::spawn(move || {
        let output = Command::new(shell)
            .args(["-ilc", &format!("printf '{MARKER}'; env -0")])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        let _ = tx.send(output);
    });
    let output = rx.recv_timeout(Duration::from_secs(5)).ok()?.ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(parse_env(&stdout))
}

/// Parses `env -0` output that follows the marker; anything rc files printed before it is ignored.
fn parse_env(stdout: &str) -> Vec<(String, String)> {
    let Some((_, vars)) = stdout.split_once(MARKER) else {
        return Vec::new();
    };
    vars.split('\0')
        .filter_map(|entry| entry.split_once('='))
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_env_after_marker() {
        let out = format!("Welcome banner\n{MARKER}PATH=/opt/homebrew/bin:/usr/bin\0KUBECONFIG=/a:/b\0EQ=a=b\0\0");
        assert_eq!(
            parse_env(&out),
            [
                ("PATH".to_owned(), "/opt/homebrew/bin:/usr/bin".to_owned()),
                ("KUBECONFIG".to_owned(), "/a:/b".to_owned()),
                ("EQ".to_owned(), "a=b".to_owned()),
            ]
        );
        assert!(parse_env("no marker").is_empty());
    }
}
