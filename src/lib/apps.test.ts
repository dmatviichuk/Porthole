import { describe, expect, it } from "vitest";

import { buildApplications } from "./apps";
import { CRONJOB, DEPLOYMENT, JOB, STATEFULSET } from "./kinds";
import type { ResourceRow } from "./types";

function row(name: string, extra: Partial<ResourceRow> = {}): ResourceRow {
  return {
    uid: `uid-${name}`,
    name,
    namespace: "default",
    created: 0,
    owner: null,
    labels: {},
    status: "Running",
    health: "ok",
    fields: {},
    ...extra,
  };
}

const deployment = row("web", { fields: { ready: "1/2", readyCount: 1, desired: 2 } });
const webPod = (name: string, extra: Partial<ResourceRow> = {}) =>
  row(name, {
    owner: { kind: "ReplicaSet", name: "web-7d9f8b" },
    labels: { "pod-template-hash": "7d9f8b" },
    ...extra,
  });

describe("buildApplications", () => {
  it("folds pods into their deployment through the ReplicaSet name", () => {
    const apps = buildApplications(
      [{ resource: DEPLOYMENT, rows: [deployment] }],
      [webPod("web-7d9f8b-aaaaa"), webPod("web-7d9f8b-bbbbb", { status: "CrashLoopBackOff", health: "failed" })],
    );
    expect(apps).toHaveLength(1);
    const [web] = apps;
    expect(web?.resource.kind).toBe("Deployment");
    expect(web?.pods.map((p) => p.name)).toEqual(["web-7d9f8b-aaaaa", "web-7d9f8b-bbbbb"]);
    expect(web?.podsLabel).toBe("1/2");
    // A crashing pod is what the user needs to see, not the deployment's "Updating".
    expect([web?.status, web?.health]).toEqual(["CrashLoopBackOff", "failed"]);
  });

  it("shows CronJob pods under the CronJob, not the Job", () => {
    const cron = row("backup", { status: "Scheduled" });
    const job = row("backup-2931", { owner: { kind: "CronJob", name: "backup" }, status: "Completed", health: "done" });
    const pod = row("backup-2931-x", { owner: { kind: "Job", name: "backup-2931" }, status: "Completed", health: "done" });
    const apps = buildApplications(
      [
        { resource: CRONJOB, rows: [cron] },
        { resource: JOB, rows: [job] },
      ],
      [pod],
    );
    expect(apps.map((a) => [a.resource.kind, a.row.name, a.pods.length])).toEqual([["CronJob", "backup", 1]]);
    expect(apps[0]?.status).toBe("Scheduled");
  });

  it("lists pods without a known workload on their own", () => {
    const bare = row("debug", { fields: { ready: "1/1", readyCount: 1, total: 1 } });
    const orphanRs = webPod("legacy-abc-1", { owner: { kind: "ReplicaSet", name: "legacy-abc" } });
    const sts = row("db", { fields: { ready: "1/1", readyCount: 1, desired: 1 } });
    const stsPod = row("db-0", { owner: { kind: "StatefulSet", name: "db" } });
    const apps = buildApplications([{ resource: STATEFULSET, rows: [sts] }], [bare, orphanRs, stsPod]);
    expect(apps.map((a) => `${a.resource.kind}/${a.row.name}`).toSorted()).toEqual([
      "Pod/debug",
      "Pod/legacy-abc-1",
      "StatefulSet/db",
    ]);
    expect(apps.find((a) => a.row.name === "debug")?.podsLabel).toBe("1/1");
  });

  it("keeps a finished Job's own status even when retried pods failed", () => {
    const job = row("migrate", { status: "Completed", health: "done", fields: { completions: "1/1" } });
    const failedTry = row("migrate-a", { owner: { kind: "Job", name: "migrate" }, status: "Error", health: "failed" });
    const [app] = buildApplications([{ resource: JOB, rows: [job] }], [failedTry]);
    expect([app?.status, app?.podsLabel]).toEqual(["Completed", "1/1"]);
  });

  it("does not match a namesake workload in another namespace", () => {
    const other = webPod("web-7d9f8b-ccccc", { namespace: "staging" });
    const apps = buildApplications([{ resource: DEPLOYMENT, rows: [deployment] }], [other]);
    expect(apps.map((a) => `${a.resource.kind}/${a.row.namespace}`).toSorted()).toEqual([
      "Deployment/default",
      "Pod/staging",
    ]);
  });
});
