import { useEffect, useMemo, useState } from "react";

import { type Column, DataTable } from "../components/DataTable";
import { Bar, CapacityMeter, Card, Gauge } from "../components/Meters";
import { type Basis, BasisSelect } from "../components/PodsTable";
import { Status } from "../components/Status";
import { openResource } from "../lib/actions";
import { Age } from "../lib/columns";
import { clusterVersion } from "../lib/ipc";
import { focusOnMount } from "../lib/keys";
import { NODE, POD } from "../lib/kinds";
import { useNodeUsage } from "../lib/metricsStore";
import { type Amounts, type NodeUtilization, percent, utilization } from "../lib/utilization";
import { useResources } from "../lib/watchStore";
import { useApp } from "../store";
import { Banner, MetricsWarning, Placeholder, SectionTitle } from "./common";

function useClusterVersion(context: string | null) {
  const [version, setVersion] = useState<{ context: string; value: string } | null>(null);
  useEffect(() => {
    if (!context) return;
    let live = true;
    clusterVersion(context).then(
      (value) => live && setVersion({ context, value: value.replace(/^v/, "") }),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [context]);
  return version?.context === context ? version.value : null;
}

/** The cluster at a glance: version, node and pod counts, capacity, and every node's load. */
export function ClusterOverview() {
  const context = useApp((s) => s.context);
  const nodes = useResources(context, NODE, null);
  const pods = useResources(context, POD, null);
  const metrics = useNodeUsage(context);
  const version = useClusterVersion(context);
  const [chosen, setBasis] = useState<Basis>("usage");
  const u = useMemo(() => utilization(nodes.rows, pods.rows, metrics.data), [nodes.rows, pods.rows, metrics.data]);
  const metricsMissing = metrics.error !== null;
  const basis: Basis = metricsMissing && chosen === "usage" ? "requests" : chosen;
  const columns = useMemo(() => nodeColumns(basis), [basis]);

  if (!context) return <Placeholder>Choose a cluster to see its overview.</Placeholder>;

  return (
    <div
      // The page takes focus so the arrow keys scroll it; Tab goes on to its tables.
      ref={focusOnMount}
      tabIndex={-1}
      data-primary
      className="h-full overflow-y-auto px-8 pt-2 pb-10 outline-none"
    >
      {nodes.error || pods.error ? (
        <Banner>{nodes.error ?? pods.error}</Banner>
      ) : (
        <MetricsWarning error={metrics.error} className="mb-5" />
      )}
      <div className="grid gap-5 xl:grid-cols-2">
        <section className="flex flex-col">
          <SectionTitle>Cluster</SectionTitle>
          <Card className="flex-1">
            <dl className="grid grid-cols-[110px_1fr] items-center gap-x-6 gap-y-2.5">
              <dt className="text-muted">Name</dt>
              <dd className="selectable truncate">{context}</dd>
              <dt className="text-muted">Version</dt>
              <dd className="selectable">{version ?? "…"}</dd>
              <dt className="text-muted">Nodes</dt>
              <dd>{nodes.synced ? u.nodes.length : "…"}</dd>
              <dt className="text-muted">Pods</dt>
              <dd className="flex items-center gap-4">
                <span>
                  {u.pods}/{u.podCapacity} · {Math.round(percent(u.pods, u.podCapacity))}%
                </span>
                <Bar value={percent(u.pods, u.podCapacity)} tone="neutral" className="w-44" />
              </dd>
            </dl>
          </Card>
        </section>
        <section className="flex flex-col">
          <SectionTitle>Cluster Utilization</SectionTitle>
          <Card className="grid flex-1 grid-cols-2 gap-8">
            <CapacityMeter title="CPU" kind="cpu" totals={u} />
            <CapacityMeter title="Memory" kind="memory" totals={u} />
          </Card>
        </section>
      </div>

      <section className="mt-10">
        <SectionTitle aside={<BasisSelect value={basis} onChange={setBasis} usageAvailable={!metricsMissing} />}>
          Nodes
        </SectionTitle>
        <div className="-mx-6">
          <DataTable
            id="utilization.nodes"
            columns={columns}
            rows={u.nodes}
            getRowId={(n) => n.row.uid}
            rowHeight={44}
            fit
            initialSort={{ id: "name", desc: false }}
            onOpen={(n) => openResource(NODE, n.row)}
            empty={nodes.synced ? "No nodes." : "Loading nodes…"}
          />
        </div>
      </section>
    </div>
  );
}

const BASIS_LABEL: Record<Basis, string> = { usage: "Usage", requests: "Requests", limits: "Limits" };

function nodeColumns(basis: Basis): Column<NodeUtilization>[] {
  const share = (n: NodeUtilization, kind: keyof Amounts) => {
    const amount = basis === "usage" ? n.usage?.[kind] : n[basis][kind];
    return amount === undefined ? null : percent(amount, n.allocatable[kind]);
  };
  const metric = (kind: keyof Amounts): Column<NodeUtilization> => ({
    id: kind,
    header: `${kind === "cpu" ? "CPU" : "Mem"} ${BASIS_LABEL[basis]}`,
    width: "130px",
    numeric: true,
    sortValue: (n) => share(n, kind),
    cell: (n) => {
      const value = share(n, kind);
      return value === null ? (
        <span className="text-faint">—</span>
      ) : (
        <Gauge fill={value} kind={kind}>
          {value.toFixed(2)}%
        </Gauge>
      );
    },
  });
  return [
    { id: "name", header: "Name", width: "minmax(260px, 4fr)", sortValue: (n) => n.row.name, cell: (n) => n.row.name },
    {
      id: "age",
      header: "Age",
      width: "80px",
      numeric: true,
      sortValue: (n) => (n.row.created === null ? null : -n.row.created),
      cell: (n) => <Age created={n.row.created} />,
    },
    metric("cpu"),
    metric("memory"),
    {
      id: "pods",
      header: "Pods",
      width: "90px",
      numeric: true,
      sortValue: (n) => percent(n.pods, n.podCapacity),
      cell: (n) => `${n.pods}/${n.podCapacity}`,
    },
    {
      id: "status",
      header: "Status",
      width: "minmax(130px, 1.2fr)",
      sortValue: (n) => n.row.status,
      cell: (n) => <Status health={n.row.health} text={n.row.status} />,
    },
  ];
}
