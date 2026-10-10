use kube::{
    Api, Client,
    api::{DeleteParams, DynamicObject, PostParams},
    core::ApiResource,
    discovery::{Discovery, Scope},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// Identifies one API resource type, e.g. `apps/v1 deployments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceType {
    pub group: String,
    pub version: String,
    pub kind: String,
    pub plural: String,
    pub namespaced: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    #[serde(flatten)]
    pub resource: ResourceType,
    pub verbs: Vec<String>,
}

impl ResourceType {
    fn api_resource(&self) -> ApiResource {
        let api_version = if self.group.is_empty() {
            self.version.clone()
        } else {
            format!("{}/{}", self.group, self.version)
        };
        ApiResource {
            group: self.group.clone(),
            version: self.version.clone(),
            api_version,
            kind: self.kind.clone(),
            plural: self.plural.clone(),
        }
    }

    /// Namespaced types without a namespace address all namespaces.
    pub fn api(&self, client: Client, namespace: Option<&str>) -> Api<DynamicObject> {
        let ar = self.api_resource();
        match namespace {
            Some(ns) if self.namespaced => Api::namespaced_with(client, ns, &ar),
            _ => Api::all_with(client, &ar),
        }
    }
}

/// Every listable, watchable resource type the cluster serves, preferred versions only.
pub async fn discover(client: Client) -> Result<Vec<ResourceInfo>> {
    // Aggregated discovery is two requests on Kubernetes 1.30+; older servers need one per group.
    let started = std::time::Instant::now();
    let discovery = match Discovery::new(client.clone()).run_aggregated().await {
        Ok(discovery) => discovery,
        Err(err) => {
            tracing::warn!(error = %err, "aggregated discovery failed, querying each API group");
            Discovery::new(client).run().await?
        }
    };
    tracing::info!(elapsed_ms = started.elapsed().as_millis() as u64, "discovery");
    let mut resources: Vec<ResourceInfo> = discovery
        .groups()
        .flat_map(|group| group.recommended_resources())
        .filter(|(_, caps)| caps.supports_operation("list") && caps.supports_operation("watch"))
        .map(|(ar, caps)| ResourceInfo {
            resource: ResourceType {
                group: ar.group,
                version: ar.version,
                kind: ar.kind,
                plural: ar.plural,
                namespaced: caps.scope == Scope::Namespaced,
            },
            verbs: caps.operations,
        })
        .collect();
    resources.sort_by(|a, b| {
        a.resource
            .kind
            .cmp(&b.resource.kind)
            .then(a.resource.group.cmp(&b.resource.group))
    });
    Ok(resources)
}

/// The object as YAML, without managedFields (bookkeeping nobody edits by hand).
pub async fn get_yaml(client: Client, resource: &ResourceType, namespace: Option<&str>, name: &str) -> Result<String> {
    let mut obj = resource.api(client, namespace).get(name).await?;
    obj.metadata.managed_fields = None;
    // The original editor used serde_json's sorted maps. Keep the same field
    // order even though log parsing now preserves JSON insertion order.
    obj.data.sort_all_objects();
    if obj.types.is_none() {
        obj.types = Some(kube::core::TypeMeta {
            api_version: resource.api_resource().api_version,
            kind: resource.kind.clone(),
        });
    }
    serde_saphyr::to_string(&obj).map_err(|err| Error::Yaml(err.to_string()))
}

pub async fn delete(client: Client, resource: &ResourceType, namespace: Option<&str>, name: &str) -> Result<()> {
    resource
        .api(client, namespace)
        .delete(name, &DeleteParams::background())
        .await?;
    Ok(())
}

/// Checks that edited YAML is still the object the editor was opened for: one document, same
/// apiVersion, kind, name and namespace, and a resourceVersion. A replace with a resourceVersion
/// can only update an existing object, never create one, and fails if someone changed it since.
pub fn parse_edit(yaml: &str, resource: &ResourceType, namespace: Option<&str>, name: &str) -> Result<DynamicObject> {
    let docs: Vec<Value> = serde_saphyr::from_multiple(yaml).map_err(|err| Error::Yaml(err.to_string()))?;
    let mut docs = docs.into_iter().filter(|doc| !doc.is_null());
    let (Some(doc), None) = (docs.next(), docs.next()) else {
        return Err(Error::Invalid("the YAML must contain exactly one object".into()));
    };
    let object: DynamicObject = serde_json::from_value(doc).map_err(|err| Error::Invalid(err.to_string()))?;
    let expected = resource.api_resource().api_version;
    let types = object.types.as_ref();
    if types.map(|t| (t.api_version.as_str(), t.kind.as_str())) != Some((expected.as_str(), resource.kind.as_str())) {
        return Err(Error::Invalid(format!(
            "apiVersion and kind must stay {expected} {}",
            resource.kind
        )));
    }
    if object.metadata.name.as_deref() != Some(name) {
        return Err(Error::Invalid(format!("metadata.name must stay {name}")));
    }
    if resource.namespaced && object.metadata.namespace.as_deref() != namespace {
        return Err(Error::Invalid(format!(
            "metadata.namespace must stay {}",
            namespace.unwrap_or_default()
        )));
    }
    if object
        .metadata
        .resource_version
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        return Err(Error::Invalid(
            "metadata.resourceVersion is required, so a save never overwrites changes made since you loaded it".into(),
        ));
    }
    Ok(object)
}

/// Saves an edited object with a full replace, like `kubectl edit`.
pub async fn replace(
    client: Client,
    resource: &ResourceType,
    namespace: Option<&str>,
    name: &str,
    yaml: &str,
) -> Result<()> {
    let object = parse_edit(yaml, resource, namespace, name)?;
    resource
        .api(client, namespace)
        .replace(name, &PostParams::default(), &object)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployments() -> ResourceType {
        ResourceType {
            group: "apps".into(),
            version: "v1".into(),
            kind: "Deployment".into(),
            plural: "deployments".into(),
            namespaced: true,
        }
    }

    const WEB: &str = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n  namespace: shop\n  resourceVersion: \"42\"\nspec:\n  replicas: 2\n";

    #[test]
    fn accepts_the_object_that_was_opened() {
        let object = parse_edit(WEB, &deployments(), Some("shop"), "web").unwrap();
        assert_eq!(object.data["spec"]["replicas"], 2);
    }

    #[test]
    fn refuses_anything_that_could_create_or_touch_another_object() {
        let err = |yaml: &str| {
            parse_edit(yaml, &deployments(), Some("shop"), "web")
                .expect_err("should be refused")
                .to_string()
        };
        assert!(err(&WEB.replace("name: web", "name: web-copy")).contains("metadata.name must stay web"));
        assert!(err(&WEB.replace("namespace: shop", "namespace: prod")).contains("metadata.namespace must stay shop"));
        assert!(err(&WEB.replace("kind: Deployment", "kind: StatefulSet")).contains("must stay apps/v1 Deployment"));
        assert!(err(&WEB.replace("  resourceVersion: \"42\"\n", "")).contains("resourceVersion is required"));
        assert!(err(&format!("{WEB}---\n{WEB}")).contains("exactly one object"));
        assert!(err("a: [unclosed").contains("invalid YAML"));
    }

    #[test]
    fn api_resource_joins_group_and_version() {
        let core = ResourceType {
            group: String::new(),
            version: "v1".into(),
            kind: "Pod".into(),
            plural: "pods".into(),
            namespaced: true,
        };
        let apps = ResourceType {
            group: "apps".into(),
            kind: "Deployment".into(),
            plural: "deployments".into(),
            ..core.clone()
        };
        assert_eq!(core.api_resource().api_version, "v1");
        assert_eq!(apps.api_resource().api_version, "apps/v1");
    }
}
