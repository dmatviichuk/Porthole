use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use kube::{
    Client, Config,
    config::{KubeConfigOptions, Kubeconfig},
};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::{
    error::{Error, Result},
    resources::{self, ResourceInfo},
};

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContextInfo {
    pub name: String,
    pub cluster: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Contexts {
    pub current: Option<String>,
    pub contexts: Vec<ContextInfo>,
}

/// Reads the merged kubeconfig (`$KUBECONFIG` or `~/.kube/config`), the same files kubectl uses.
pub fn list_contexts() -> Result<Contexts> {
    let kubeconfig = Kubeconfig::read()?;
    let mut contexts: Vec<ContextInfo> = kubeconfig
        .contexts
        .into_iter()
        .map(|named| ContextInfo {
            cluster: named.context.as_ref().map(|c| c.cluster.clone()),
            namespace: named.context.and_then(|c| c.namespace),
            name: named.name,
        })
        .collect();
    contexts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Contexts {
        current: kubeconfig.current_context,
        contexts,
    })
}

/// Switches the context used by kubectl, without requiring kubectl to be installed.
/// With multiple KUBECONFIG files, only the first existing file is updated.
pub fn set_current_context(context: &str) -> Result<()> {
    let mut paths: Vec<_> = std::env::var_os("KUBECONFIG")
        .map(|value| {
            std::env::split_paths(&value)
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default();
    if paths.is_empty() {
        let home =
            directories::BaseDirs::new().ok_or_else(|| Error::Invalid("Could not find your home directory".into()))?;
        paths.push(home.home_dir().join(".kube/config"));
    }
    set_current_context_in(&paths, context)
}

fn set_current_context_in(paths: &[PathBuf], context: &str) -> Result<()> {
    let mut merged = Kubeconfig::default();
    let mut destination = None;
    for path in paths {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(config_io_error(path, e)),
        };
        merged = merged.merge(Kubeconfig::from_yaml(&text)?)?;
        if destination.is_none() {
            destination = Some((path, text));
        }
    }
    if context.is_empty() || !merged.contexts.iter().any(|c| c.name == context) {
        return Err(Error::Invalid(format!(
            "Context {context:?} does not exist in your kubeconfig"
        )));
    }
    if merged.current_context.as_deref() == Some(context) {
        return Ok(());
    }
    let (path, text) = destination.ok_or_else(|| Error::Invalid("No kubeconfig file to update".into()))?;
    // Parse the original text, rather than read_from: relative credential paths must stay relative.
    let mut config = Kubeconfig::from_yaml(&text)?;
    config.current_context = Some(context.to_owned());
    let yaml = serde_saphyr::to_string(&config).map_err(|e| Error::Yaml(e.to_string()))?;
    // Follow symlinks and atomically replace the target, preserving its permissions.
    let path = fs::canonicalize(path).map_err(|e| config_io_error(path, e))?;
    let metadata = fs::metadata(&path).map_err(|e| config_io_error(&path, e))?;
    if metadata.permissions().readonly() {
        return Err(Error::Invalid(format!("Kubeconfig {} is read-only", path.display())));
    }
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))
        .map_err(|e| config_io_error(&path, e))?;
    file.as_file()
        .set_permissions(metadata.permissions())
        .map_err(|e| config_io_error(&path, e))?;
    file.write_all(yaml.as_bytes()).map_err(|e| config_io_error(&path, e))?;
    file.as_file().sync_all().map_err(|e| config_io_error(&path, e))?;
    file.persist(&path).map_err(|e| config_io_error(&path, e.error))?;
    Ok(())
}

fn config_io_error(path: &Path, error: std::io::Error) -> Error {
    Error::Invalid(format!("Could not update kubeconfig {}: {error}", path.display()))
}

/// One API client per kubeconfig context, built on first use and reused by every session,
/// plus each cluster's discovered resource types.
#[derive(Default)]
pub struct Clusters {
    clients: Mutex<HashMap<String, Client>>,
    resources: Mutex<HashMap<String, Arc<Vec<ResourceInfo>>>>,
}

impl Clusters {
    pub async fn client(&self, context: &str) -> Result<Client> {
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get(context) {
            return Ok(client.clone());
        }
        let options = KubeConfigOptions {
            context: Some(context.to_owned()),
            ..Default::default()
        };
        let config = Config::from_custom_kubeconfig(Kubeconfig::read()?, &options).await?;
        let client = Client::try_from(config)?;
        clients.insert(context.to_owned(), client.clone());
        Ok(client)
    }

    /// Resource types served by the cluster; `refresh` re-runs discovery (new CRDs).
    pub async fn resources(&self, context: &str, refresh: bool) -> Result<Arc<Vec<ResourceInfo>>> {
        if !refresh && let Some(cached) = self.resources.lock().await.get(context) {
            return Ok(cached.clone());
        }
        let discovered = Arc::new(resources::discover(self.client(context).await?).await?);
        self.resources
            .lock()
            .await
            .insert(context.to_owned(), discovered.clone());
        Ok(discovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(context: &str) -> serde_json::Value {
        json!({
            "apiVersion": "v1", "kind": "Config", "current-context": context,
            "clusters": [{"name": "cluster", "cluster": {
                "server": "https://localhost:6443", "certificate-authority": "relative/ca.pem",
                "custom-cluster-field": {"keep": true}
            }}],
            "users": [{"name": "user", "user": {
                "token": "test-token", "client-key": "relative/key.pem", "custom-user-field": "keep"
            }}],
            "contexts": [
                {"name": "old", "context": {"cluster": "cluster", "user": "user", "namespace": "old-ns"}},
                {"name": "new", "context": {"cluster": "cluster", "user": "user", "namespace": "new-ns"}}
            ],
            "extensions": [{"name": "custom", "extension": {"nested": [1, 2]}}],
            "custom-field": "keep"
        })
    }

    #[test]
    fn switching_preserves_credentials_relative_paths_and_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        let mut expected = config("old");
        fs::write(&path, serde_json::to_string(&expected).unwrap()).unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();
        set_current_context_in(std::slice::from_ref(&path), "new").unwrap();
        expected["current-context"] = "new".into();
        let actual: serde_json::Value = serde_saphyr::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(fs::metadata(&path).unwrap().permissions(), permissions);
    }

    #[test]
    fn merged_configs_update_only_the_first_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = ["missing", "first", "second"].map(|name| dir.path().join(name)).into();
        fs::write(
            &paths[1],
            "apiVersion: v1\nkind: Config\ncurrent-context: old\ncontexts: []\n",
        )
        .unwrap();
        let second = serde_json::to_string(&config("old")).unwrap();
        fs::write(&paths[2], &second).unwrap();
        set_current_context_in(&paths, "new").unwrap();
        assert!(!paths[0].exists());
        assert_eq!(fs::read_to_string(&paths[2]).unwrap(), second);
        let first = Kubeconfig::read_from(&paths[1]).unwrap();
        assert!(first.contexts.is_empty());
        assert_eq!(first.current_context.as_deref(), Some("new"));
        assert_eq!(
            first
                .merge(Kubeconfig::read_from(&paths[2]).unwrap())
                .unwrap()
                .current_context
                .as_deref(),
            Some("new")
        );
    }

    #[test]
    fn invalid_contexts_and_unchanged_selections_leave_the_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        let original = format!(
            "# preserve this comment\n{}",
            serde_saphyr::to_string(&config("old")).unwrap()
        );
        fs::write(&path, &original).unwrap();
        let paths = std::slice::from_ref(&path);
        assert!(set_current_context_in(paths, "unknown").is_err());
        assert!(set_current_context_in(paths, "").is_err());
        set_current_context_in(paths, "old").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn read_only_config_reports_failure_without_changing_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        let original = serde_json::to_string(&config("old")).unwrap();
        fs::write(&path, &original).unwrap();
        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        assert!(
            set_current_context_in(std::slice::from_ref(&path), "new")
                .unwrap_err()
                .to_string()
                .contains("read-only")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::set_permissions(&path, original_permissions).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn switching_follows_symlinks_and_preserves_private_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("config");
        fs::write(&target, serde_json::to_string(&config("old")).unwrap()).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, &link).unwrap();
        set_current_context_in(std::slice::from_ref(&link), "new").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(
            Kubeconfig::read_from(&target).unwrap().current_context.as_deref(),
            Some("new")
        );
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
