import { useMemo } from "react";

import { type Column, DataTable } from "../components/DataTable";
import { formatAmount, Gauge } from "../components/Meters";
import { podLimits, podRequests } from "../components/PodsTable";
import { Status } from "../components/Status";
import { openResource, resourceMenu } from "../lib/actions";
import { type AppRow, buildApplications } from "../lib/apps";
import { Age } from "../lib/columns";
import { matches } from "../lib/format";
import { CRONJOB, DAEMONSET, DEPLOYMENT, JOB, POD, STATEFULSET } from "../lib/kinds";
import { podKey, usePodUsage } from "../lib/metricsStore";
import { percent } from "../lib/utilization";
import { useResources } from "../lib/watchStore";
import { useApp } from "../store";
import { Banner, MetricsWarning, Placeholder } from "./common";

type Usage = { cpu: number; memory: number } | null;

/** CPU and memory of each application's pods, gauged against their limits (or requests). */
function appColumns(usageOf: (app: AppRow) => Usage): Column<AppRow>[] {
  const metric = (kind: "cpu" | "memory"): Column<AppRow> => {
    const reference = (a: AppRow) => {
      const active = a.pods.filter((p) => p.fields.active === true);
      const total = (pick: (p: (typeof active)[number]) => { cpu: number; memory: number }) =>
        active.reduce((sum, p) => sum + pick(p)[kind], 0);
      return total(podLimits) || total(podRequests);
    };
    return {
      id: kind,
      header: kind === "cpu" ? "CPU Usage" : "Mem Usage",
      width: "120px",
      numeric: true,
      sortValue: (a) => usageOf(a)?.[kind] ?? null,
      cell: (a) => {
        const used = usageOf(a)?.[kind];
        if (used === undefined) return <span className="text-faint">—</span>;
        const ref = reference(a);
        return (
          <Gauge fill={ref > 0 ? percent(used, ref) : null} kind={kind}>
            {formatAmount(kind, used)}
          </Gauge>
        );
      },
    };
  };
  return [
    {
      id: "name",
      header: "Name",
      width: "minmax(220px, 3fr)",
      sortValue: (a) => a.row.name,
      cell: (a) => a.row.name,
    },
    { id: "kind", header: "Kind", width: "minmax(100px, 1fr)", sortValue: (a) => a.resource.kind, cell: (a) => a.resource.kind },
    {
      id: "namespace",
      header: "Namespace",
      width: "minmax(120px, 1.2fr)",
      sortValue: (a) => a.row.namespace,
      cell: (a) => a.row.namespace,
    },
    { id: "pods", header: "Pods", width: "84px", numeric: true, sortValue: (a) => a.podsRatio, cell: (a) => a.podsLabel },
    metric("cpu"),
    metric("memory"),
    {
      id: "status",
      header: "Status",
      width: "minmax(160px, 1.6fr)",
      sortValue: (a) => a.status,
      cell: (a) => <Status health={a.health} text={a.status} />,
    },
    {
      id: "age",
      header: "Age",
      width: "72px",
      numeric: true,
      sortValue: (a) => (a.row.created === null ? null : -a.row.created),
      cell: (a) => <Age created={a.row.created} />,
    },
  ];
}

export function ApplicationsView() {
  const context = useApp((s) => s.context);
  const namespace = useApp((s) => s.namespace);
  const search = useApp((s) => s.search);

  const pods = useResources(context, POD, namespace);
  const deployments = useResources(context, DEPLOYMENT, namespace);
  const statefulSets = useResources(context, STATEFULSET, namespace);
  const daemonSets = useResources(context, DAEMONSET, namespace);
  const cronJobs = useResources(context, CRONJOB, namespace);
  const jobs = useResources(context, JOB, namespace);

  const apps = useMemo(
    () =>
      buildApplications(
        [
          { resource: DEPLOYMENT, rows: deployments.rows },
          { resource: STATEFULSET, rows: statefulSets.rows },
          { resource: DAEMONSET, rows: daemonSets.rows },
          { resource: CRONJOB, rows: cronJobs.rows },
          { resource: JOB, rows: jobs.rows },
        ],
        pods.rows,
      ),
    [deployments.rows, statefulSets.rows, daemonSets.rows, cronJobs.rows, jobs.rows, pods.rows],
  );

  const visible = useMemo(
    () => apps.filter((a) => matches(search, a.row.name, a.resource.kind, a.row.namespace, a.status)),
    [apps, search],
  );
  const usage = usePodUsage(context);
  const shown = useMemo(() => {
    const usageOf = (app: AppRow): Usage => {
      if (!usage.data) return null;
      const samples = app.pods.map((p) => usage.data?.get(podKey(p.namespace, p.name))).filter((u) => u !== undefined);
      if (samples.length === 0) return null;
      return samples.reduce((sum, u) => ({ cpu: sum.cpu + u.cpu, memory: sum.memory + u.memory }), { cpu: 0, memory: 0 });
    };
    const columns = appColumns(usageOf);
    return namespace ? columns.filter((c) => c.id !== "namespace") : columns;
  }, [namespace, usage.data]);

  const watches = [pods, deployments, statefulSets, daemonSets, cronJobs, jobs];
  const error = watches.find((w) => w.error)?.error ?? null;
  const loading = !watches.every((w) => w.synced);

  if (!context) return <Placeholder>Choose a cluster to browse.</Placeholder>;

  return (
    <div className="flex h-full flex-col">
      {error ? <Banner>{error}</Banner> : <MetricsWarning error={usage.error} />}
      <div className="min-h-0 flex-1">
        <DataTable
          id="applications"
          columns={shown}
          rows={visible}
          getRowId={(a) => a.key}
          initialSort={{ id: "name", desc: false }}
          onOpen={(a) => openResource(a.resource, a.row)}
          menu={(a) => resourceMenu(context, a.resource, a.row)}
          primary
          search={search}
          empty={
            loading
              ? "Loading applications…"
              : search
                ? `No applications match "${search}".`
                : namespace
                  ? `No applications in ${namespace}.`
                  : "No applications in this cluster."
          }
        />
      </div>
    </div>
  );
}
