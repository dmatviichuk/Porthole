import { POD } from "./kinds";
import type { Health, ResourceRow, ResourceType } from "./types";

/** One row of the Applications view: a workload with its pods, or a pod nobody owns. */
export type AppRow = {
  key: string;
  resource: ResourceType;
  row: ResourceRow;
  pods: ResourceRow[];
  /** e.g. "2/3" ready of desired; Jobs show completions. */
  podsLabel: string;
  podsRatio: number;
  status: string | null;
  health: Health | null;
};

export type WorkloadRows = { resource: ResourceType; rows: ResourceRow[] };

export const appKey = (kind: string, namespace: string | null, name: string) => `${kind}/${namespace ?? ""}/${name}`;

/**
 * The workload a pod belongs to. Deployments own pods through a ReplicaSet named
 * `<deployment>-<pod-template-hash>`, so the deployment name falls out of the pod's own label
 * without watching ReplicaSets.
 */
export function podOwnerKey(pod: ResourceRow, cronJobOfJob: Map<string, string>): string | null {
  const owner = pod.owner;
  if (!owner) return null;
  switch (owner.kind) {
    case "ReplicaSet": {
      const hash = pod.labels["pod-template-hash"];
      if (!hash || !owner.name.endsWith(`-${hash}`)) return null;
      return appKey("Deployment", pod.namespace, owner.name.slice(0, -(hash.length + 1)));
    }
    case "StatefulSet":
    case "DaemonSet":
      return appKey(owner.kind, pod.namespace, owner.name);
    case "Job":
      return cronJobOfJob.get(appKey("Job", pod.namespace, owner.name)) ?? appKey("Job", pod.namespace, owner.name);
    default:
      return null;
  }
}

// For the status column: a failing pod outranks a healthy workload; finished pods rank last.
const RANK: Record<Health, number> = { failed: 4, progress: 3, ok: 2, ending: 1, done: 0 };

export function buildApplications(workloads: WorkloadRows[], pods: ResourceRow[]): AppRow[] {
  const cronJobOfJob = new Map<string, string>();
  for (const { resource, rows } of workloads) {
    if (resource.kind !== "Job") continue;
    for (const job of rows) {
      if (job.owner?.kind === "CronJob") {
        cronJobOfJob.set(appKey("Job", job.namespace, job.name), appKey("CronJob", job.namespace, job.owner.name));
      }
    }
  }

  const apps = new Map<string, AppRow>();
  const add = (resource: ResourceType, row: ResourceRow) => {
    const key = appKey(resource.kind, row.namespace, row.name);
    apps.set(key, { key, resource, row, pods: [], podsLabel: "", podsRatio: 0, status: row.status, health: row.health });
  };
  for (const { resource, rows } of workloads) {
    for (const row of rows) {
      // Jobs a CronJob started are shown through that CronJob.
      if (resource.kind === "Job" && row.owner?.kind === "CronJob") continue;
      add(resource, row);
    }
  }
  for (const pod of pods) {
    const key = podOwnerKey(pod, cronJobOfJob);
    const app = key ? apps.get(key) : undefined;
    if (app) {
      app.pods.push(pod);
    } else {
      add(POD, pod);
      apps.get(appKey("Pod", pod.namespace, pod.name))?.pods.push(pod);
    }
  }
  return Array.from(apps.values(), summarize);
}

function summarize(app: AppRow): AppRow {
  const fields = app.row.fields;
  const num = (key: string) => (typeof fields[key] === "number" ? (fields[key] as number) : 0);
  switch (app.resource.kind) {
    case "Pod":
      app.podsLabel = String(fields.ready ?? "");
      app.podsRatio = num("total") ? num("readyCount") / num("total") : 0;
      break;
    case "CronJob": {
      const running = app.pods.filter((p) => p.health === "ok" || p.health === "progress").length;
      app.podsLabel = String(running);
      app.podsRatio = running;
      break;
    }
    default:
      app.podsLabel = String(fields.completions ?? fields.ready ?? "");
      app.podsRatio = num("desired") ? num("readyCount") / num("desired") : 1;
  }

  // Finished Job pods are history (a retried Job leaves failed pods behind), so Jobs keep their own status.
  if (app.resource.kind !== "Job" && app.resource.kind !== "CronJob") {
    for (const pod of app.pods) {
      if (pod.health && (app.health === null || RANK[pod.health] > RANK[app.health])) {
        app.health = pod.health;
        app.status = pod.status;
      }
    }
  }
  return app;
}
