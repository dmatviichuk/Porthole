import type { ResourceRow } from "./types";

/** CPU in millicores, memory in bytes. */
export type Amounts = { cpu: number; memory: number };

export type NodeUsage = { name: string } & Amounts;

export type PodUsage = { namespace: string; name: string; containers: ({ name: string } & Amounts)[] } & Amounts;

export type NodeUtilization = {
  row: ResourceRow;
  allocatable: Amounts;
  podCapacity: number;
  pods: number;
  /** null when metrics-server is not installed or has no sample for the node yet. */
  usage: Amounts | null;
  requests: Amounts;
  limits: Amounts;
};

export type ClusterUtilization = Omit<NodeUtilization, "row"> & { nodes: NodeUtilization[] };

const field = (row: ResourceRow, key: string): number => {
  const value = row.fields[key];
  return typeof value === "number" ? value : 0;
};

const add = (a: Amounts, b: Amounts): Amounts => ({ cpu: a.cpu + b.cpu, memory: a.memory + b.memory });
const ZERO: Amounts = { cpu: 0, memory: 0 };

/**
 * Per-node and cluster totals the way `kubectl describe node` counts them: requests and
 * limits of pods that still hold resources (not Succeeded/Failed), against allocatable.
 */
export function utilization(nodes: ResourceRow[], pods: ResourceRow[], usage: NodeUsage[] | null): ClusterUtilization {
  const byName = new Map<string, NodeUtilization>(
    nodes.map((row) => [
      row.name,
      {
        row,
        allocatable: { cpu: field(row, "cpu"), memory: field(row, "memory") },
        podCapacity: field(row, "podCapacity"),
        pods: 0,
        usage: null,
        requests: ZERO,
        limits: ZERO,
      },
    ]),
  );
  let activePods = 0;
  for (const pod of pods) {
    if (pod.fields.active !== true) continue;
    activePods += 1;
    const node = byName.get(String(pod.fields.node ?? ""));
    if (!node) continue; // not scheduled yet
    node.pods += 1;
    node.requests = add(node.requests, { cpu: field(pod, "cpuRequest"), memory: field(pod, "memRequest") });
    node.limits = add(node.limits, { cpu: field(pod, "cpuLimit"), memory: field(pod, "memLimit") });
  }
  for (const sample of usage ?? []) {
    const node = byName.get(sample.name);
    if (node) node.usage = { cpu: sample.cpu, memory: sample.memory };
  }

  const list = [...byName.values()];
  const sum = (pick: (n: NodeUtilization) => Amounts) => list.reduce((acc, n) => add(acc, pick(n)), ZERO);
  const sampled = list.filter((n) => n.usage !== null);
  return {
    nodes: list,
    allocatable: sum((n) => n.allocatable),
    podCapacity: list.reduce((acc, n) => acc + n.podCapacity, 0),
    pods: activePods,
    usage: usage && sampled.length > 0 ? sum((n) => n.usage ?? ZERO) : null,
    requests: sum((n) => n.requests),
    limits: sum((n) => n.limits),
  };
}

export const percent = (part: number, whole: number) => (whole > 0 ? (part / whole) * 100 : 0);

/** 2158m, 624.40m, 2.34m: decimals only where they still mean something. */
export const formatCpu = (millicores: number) =>
  millicores >= 1000 ? `${Math.round(millicores)}m` : `${millicores.toFixed(2)}m`;

/** 1.22Gi, 67.14Mi. */
export function formatMemory(bytes: number): string {
  const gi = bytes / 1024 ** 3;
  if (gi >= 1) return `${gi.toFixed(2)}Gi`;
  return `${(bytes / 1024 ** 2).toFixed(2)}Mi`;
}
