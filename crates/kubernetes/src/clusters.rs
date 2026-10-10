use std::{collections::HashMap, sync::Arc};

use kube::{
    Client, Config,
    config::{KubeConfigOptions, Kubeconfig},
};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::{
    error::Result,
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
