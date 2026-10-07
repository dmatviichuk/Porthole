import type { ResourceInfo, ResourceType } from "./types";

const core = (kind: string, plural: string, namespaced = true): ResourceType => ({
  group: "",
  version: "v1",
  kind,
  plural,
  namespaced,
});
const typed = (group: string, version: string, kind: string, plural: string, namespaced = true): ResourceType => ({
  group,
  version,
  kind,
  plural,
  namespaced,
});

export const POD = core("Pod", "pods");
export const NAMESPACE = core("Namespace", "namespaces", false);
export const EVENT = core("Event", "events");
export const NODE = core("Node", "nodes", false);
export const SERVICE = core("Service", "services");
export const CONFIGMAP = core("ConfigMap", "configmaps");
export const PVC = core("PersistentVolumeClaim", "persistentvolumeclaims");
export const INGRESS = typed("networking.k8s.io", "v1", "Ingress", "ingresses");
export const DEPLOYMENT = typed("apps", "v1", "Deployment", "deployments");
export const STATEFULSET = typed("apps", "v1", "StatefulSet", "statefulsets");
export const DAEMONSET = typed("apps", "v1", "DaemonSet", "daemonsets");
export const REPLICASET = typed("apps", "v1", "ReplicaSet", "replicasets");
export const JOB = typed("batch", "v1", "Job", "jobs");
export const CRONJOB = typed("batch", "v1", "CronJob", "cronjobs");

/** Kinds that show up in the Applications view. */
export const WORKLOADS = [DEPLOYMENT, STATEFULSET, DAEMONSET, CRONJOB, JOB] as const;

export const sameType = (a: ResourceType, b: ResourceType) =>
  a.group === b.group && a.plural === b.plural;

export const typeKey = (r: ResourceType) => (r.group ? `${r.plural}.${r.group}` : r.plural);

export const apiVersion = (r: ResourceType) => (r.group ? `${r.group}/${r.version}` : r.version);

/** Sidebar sections of the All Resources view, in kubectl/Lens order. Anything else is a custom resource. */
const SECTIONS: { title: string; kinds: string[] }[] = [
  {
    title: "Workloads",
    kinds: ["pods", "deployments.apps", "statefulsets.apps", "daemonsets.apps", "replicasets.apps", "jobs.batch", "cronjobs.batch"],
  },
  {
    title: "Network",
    kinds: [
      "services",
      "ingresses.networking.k8s.io",
      "ingressclasses.networking.k8s.io",
      "networkpolicies.networking.k8s.io",
      "endpointslices.discovery.k8s.io",
    ],
  },
  {
    title: "Config",
    kinds: [
      "configmaps",
      "secrets",
      "horizontalpodautoscalers.autoscaling",
      "poddisruptionbudgets.policy",
      "resourcequotas",
      "limitranges",
      "priorityclasses.scheduling.k8s.io",
    ],
  },
  {
    title: "Storage",
    kinds: ["persistentvolumeclaims", "persistentvolumes", "storageclasses.storage.k8s.io"],
  },
  {
    title: "Access control",
    kinds: [
      "serviceaccounts",
      "roles.rbac.authorization.k8s.io",
      "rolebindings.rbac.authorization.k8s.io",
      "clusterroles.rbac.authorization.k8s.io",
      "clusterrolebindings.rbac.authorization.k8s.io",
    ],
  },
  {
    title: "Cluster",
    kinds: ["namespaces", "nodes", "events", "customresourcedefinitions.apiextensions.k8s.io"],
  },
];

/** API groups served by Kubernetes itself; any other group comes from a CRD or an aggregated API. */
const BUILT_IN_GROUPS = new Set([
  "",
  "admissionregistration.k8s.io",
  "apiextensions.k8s.io",
  "apiregistration.k8s.io",
  "apps",
  "authentication.k8s.io",
  "authorization.k8s.io",
  "autoscaling",
  "batch",
  "certificates.k8s.io",
  "coordination.k8s.io",
  "discovery.k8s.io",
  "events.k8s.io",
  "flowcontrol.apiserver.k8s.io",
  "internal.apiserver.k8s.io",
  "metrics.k8s.io",
  "networking.k8s.io",
  "node.k8s.io",
  "policy",
  "rbac.authorization.k8s.io",
  "resource.k8s.io",
  "scheduling.k8s.io",
  "storage.k8s.io",
  "storagemigration.k8s.io",
]);

const byGroupThenKind = (a: ResourceInfo, b: ResourceInfo) =>
  a.group.localeCompare(b.group) || a.kind.localeCompare(b.kind);

/** Groups discovered types for the All Resources list, so every type is reachable somewhere. */
export function sections(resources: ResourceInfo[]): { title: string; items: ResourceInfo[] }[] {
  const byKey = new Map(resources.map((r) => [typeKey(r), r]));
  const known = new Set(SECTIONS.flatMap((s) => s.kinds));
  const listed = SECTIONS.map((s) => ({
    title: s.title,
    items: s.kinds.map((k) => byKey.get(k)).filter((r): r is ResourceInfo => r !== undefined),
  }));
  const rest = resources.filter((r) => !known.has(typeKey(r)));
  const custom = rest.filter((r) => !BUILT_IN_GROUPS.has(r.group)).toSorted(byGroupThenKind);
  // events.k8s.io serves the same objects as core events, already listed under Cluster.
  const other = rest
    .filter((r) => BUILT_IN_GROUPS.has(r.group) && r.group !== "events.k8s.io")
    .toSorted((a, b) => a.kind.localeCompare(b.kind));
  return [...listed, { title: "Custom resources", items: custom }, { title: "Other", items: other }].filter(
    (s) => s.items.length > 0,
  );
}

/** "Deployment" -> "Deployments", "Ingress" -> "Ingresses", "NetworkPolicy" -> "NetworkPolicies". */
export function pluralTitle(kind: string): string {
  if (/[^aeiou]y$/.test(kind)) return `${kind.slice(0, -1)}ies`;
  if (/(s|x|ch|sh)$/.test(kind)) return `${kind}es`;
  return `${kind}s`;
}

/** kubectl's short names for built-in types (discovery does not report them). */
const SHORT_NAMES: Record<string, string> = {
  pods: "po",
  deployments: "deploy",
  statefulsets: "sts",
  daemonsets: "ds",
  replicasets: "rs",
  cronjobs: "cj",
  services: "svc",
  ingresses: "ing",
  networkpolicies: "netpol",
  endpointslices: "eps",
  configmaps: "cm",
  horizontalpodautoscalers: "hpa",
  poddisruptionbudgets: "pdb",
  resourcequotas: "quota",
  limitranges: "limits",
  priorityclasses: "pc",
  persistentvolumeclaims: "pvc",
  persistentvolumes: "pv",
  storageclasses: "sc",
  serviceaccounts: "sa",
  namespaces: "ns",
  nodes: "no",
  events: "ev",
  customresourcedefinitions: "crd crds",
};

/** What a type is found by: kind, plural, kubectl short name and API group. */
export const searchText = (t: ResourceInfo) =>
  [t.kind, pluralTitle(t.kind), t.plural, t.group, SHORT_NAMES[t.plural] ?? ""].join(" ").toLowerCase();

/**
 * Narrows the resource-type list to types matching every word of `query` in their kind,
 * plural, kubectl short name or API group; empty sections disappear.
 */
export function filterSections(
  list: { title: string; items: ResourceInfo[] }[],
  query: string,
): { title: string; items: ResourceInfo[] }[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return list;
  return list
    .map((section) => ({ ...section, items: section.items.filter((t) => terms.every((term) => searchText(t).includes(term))) }))
    .filter((section) => section.items.length > 0);
}

