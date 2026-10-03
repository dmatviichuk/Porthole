import { MoreHorizontal } from "lucide-react";
import { useMemo, useState } from "react";

import { openResource, resourceMenu } from "../lib/actions";
import { Age } from "../lib/columns";
import { NODE, POD } from "../lib/kinds";
import { popupMenu } from "../lib/menu";
import { podKey, usePodUsage } from "../lib/metricsStore";
import type { ResourceRow } from "../lib/types";
import { type Amounts, percent, type PodUsage } from "../lib/utilization";
import { useResources } from "../lib/watchStore";
import { type Column, DataTable } from "./DataTable";
import { formatAmount, Gauge } from "./Meters";
import { Status } from "./Status";

export type Basis = "usage" | "requests" | "limits";

const BASIS_LABEL: Record<Basis, string> = { usage: "Usage", requests: "Requests", limits: "Limits" };

export function BasisSelect({
  value,
  onChange,
  usageAvailable,
}: {
  value: Basis;
  onChange: (basis: Basis) => void;
  usageAvailable: boolean;
}) {
  return (
    <label className="flex items-center gap-3 text-muted">
      Show CPU & Memory
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as Basis)}
        className="rounded-md border border-line-strong bg-raised px-2 py-1 text-text"
      >
        <option value="usage" disabled={!usageAvailable} title={usageAvailable ? undefined : "Needs metrics-server"}>
          Usage
        </option>
        <option value="requests">Requests</option>
        <option value="limits">Limits</option>
      </select>
    </label>
  );
}

const num = (row: ResourceRow, key: string) => (typeof row.fields[key] === "number" ? (row.fields[key] as number) : 0);

export const podRequests = (pod: ResourceRow): Amounts => ({ cpu: num(pod, "cpuRequest"), memory: num(pod, "memRequest") });
export const podLimits = (pod: ResourceRow): Amounts => ({ cpu: num(pod, "cpuLimit"), memory: num(pod, "memLimit") });

/**
 * The value a cell shows and how full its gauge is: usage against the pod's limit (or
 * request), requests and limits against the node's allocatable.
 */
function measure(
  pod: ResourceRow,
  kind: keyof Amounts,
  basis: Basis,
  usage: PodUsage | undefined,
  node: Amounts | undefined,
): { value: number | null; fill: number | null } {
  if (basis === "usage") {
    const value = usage ? usage[kind] : null;
    const reference = podLimits(pod)[kind] || podRequests(pod)[kind];
    return { value, fill: value !== null && reference > 0 ? percent(value, reference) : null };
  }
  const value = (basis === "requests" ? podRequests(pod) : podLimits(pod))[kind];
  return { value, fill: node && node[kind] > 0 ? percent(value, node[kind]) : null };
}

type ColumnInputs = {
  context: string;
  basis: Basis;
  usage: Map<string, PodUsage> | null;
  allocatable: Map<string, Amounts>;
  showNamespace: boolean;
};

function podColumns({ context, basis, usage, allocatable, showNamespace }: ColumnInputs): Column<ResourceRow>[] {
  const metric = (kind: keyof Amounts): Column<ResourceRow> => {
    const get = (p: ResourceRow) =>
      measure(p, kind, basis, usage?.get(podKey(p.namespace, p.name)), allocatable.get(String(p.fields.node ?? "")));
    return {
      id: kind,
      header: `${kind === "cpu" ? "CPU" : "Mem"} ${BASIS_LABEL[basis]}`,
      width: "130px",
      numeric: true,
      sortValue: (p) => get(p).value,
      cell: (p) => {
        const { value, fill } = get(p);
        if (value === null) return <span className="text-faint">—</span>;
        return (
          <Gauge fill={fill} kind={kind}>
            {basis !== "usage" && value === 0 ? <span className="text-muted">Not set</span> : formatAmount(kind, value)}
          </Gauge>
        );
      },
    };
  };
  return [
    { id: "name", header: "Name", width: "minmax(220px, 3fr)", sortValue: (p) => p.name, cell: (p) => p.name },
    ...(showNamespace
      ? [{ id: "namespace", header: "Namespace", width: "minmax(120px, 1.2fr)", sortValue: (p: ResourceRow) => p.namespace, cell: (p: ResourceRow) => p.namespace }]
      : []),
    {
      id: "age",
      header: "Age",
      width: "72px",
      numeric: true,
      sortValue: (p) => (p.created === null ? null : -p.created),
      cell: (p) => <Age created={p.created} />,
    },
    {
      id: "containers",
      header: "Containers",
      width: "96px",
      numeric: true,
      sortValue: (p) => num(p, "readyCount") / Math.max(1, num(p, "total")),
      cell: (p) => String(p.fields.ready ?? ""),
    },
    { id: "restarts", header: "Restarts", width: "84px", numeric: true, sortValue: (p) => num(p, "restarts"), cell: (p) => num(p, "restarts") },
    metric("cpu"),
    metric("memory"),
    {
      id: "status",
      header: "Status",
      width: "minmax(140px, 1.3fr)",
      sortValue: (p) => p.status,
      cell: (p) => <Status health={p.health} text={p.status} />,
    },
    {
      id: "actions",
      header: "",
      width: "40px",
      sortValue: () => null,
      cell: (p) => <RowMenuButton onOpen={(anchor) => void popupMenu(resourceMenu(context, POD, p), anchor)} />,
    },
  ];
}

type Props = {
  context: string;
  id: string;
  title: string;
  pods: ResourceRow[];
  showNamespace?: boolean;
  empty: string;
};

/** Pods with live CPU and memory, switchable between usage, requests and limits. */
export function PodsTable({ context, id, title, pods, showNamespace = false, empty }: Props) {
  const usage = usePodUsage(context);
  const nodes = useResources(context, NODE, null);
  const [chosen, setBasis] = useState<Basis>("usage");
  const basis: Basis = usage.error && chosen === "usage" ? "requests" : chosen;

  const allocatable = useMemo(
    () => new Map(nodes.rows.map((n) => [n.name, { cpu: num(n, "cpu"), memory: num(n, "memory") }])),
    [nodes.rows],
  );

  const columns = useMemo(
    () => podColumns({ context, basis, usage: usage.data, allocatable, showNamespace }),
    [basis, usage.data, allocatable, showNamespace, context],
  );


  return (
    <section className="mb-8">
      <div className="mb-3 flex items-center justify-between">
        <h2 className="text-[15px] font-semibold">{title}</h2>
        <BasisSelect value={basis} onChange={setBasis} usageAvailable={!usage.error} />
      </div>
      <div className="-mx-6">
        <DataTable
          id={id}
          columns={columns}
          rows={pods}
          getRowId={(p) => p.uid}
          rowHeight={44}
          fit
          initialSort={{ id: "name", desc: false }}
          onOpen={(p) => openResource(POD, p)}
          onMenu={(p) => void popupMenu(resourceMenu(context, POD, p))}
          empty={empty}
        />
      </div>
    </section>
  );
}

/** The "…" button at the end of a row; it opens the same menu as a right click. */
export function RowMenuButton({ onOpen }: { onOpen: (anchor: HTMLElement) => void }) {
  return (
    <button
      type="button"
      aria-label="Actions"
      title="Actions"
      onClick={(e) => {
        e.stopPropagation();
        onOpen(e.currentTarget);
      }}
      className="rounded p-1 text-muted hover:bg-selected hover:text-text"
    >
      <MoreHorizontal size={16} />
    </button>
  );
}

