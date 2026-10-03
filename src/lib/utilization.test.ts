import { describe, expect, it } from "vitest";

import type { ResourceRow } from "./types";
import { formatCpu, formatMemory, percent, utilization } from "./utilization";

const GI = 1024 ** 3;

function row(name: string, fields: Record<string, unknown>): ResourceRow {
  return { uid: name, name, namespace: null, created: 0, owner: null, labels: {}, status: null, health: null, fields };
}

const nodes = [
  row("node-a", { cpu: 4000, memory: 16 * GI, podCapacity: 110 }),
  row("node-b", { cpu: 2000, memory: 8 * GI, podCapacity: 110 }),
];

const pod = (node: string | null, active: boolean, cpu: number, mem: number) =>
  row(`p-${node}-${cpu}`, { node, active, cpuRequest: cpu, cpuLimit: cpu * 2, memRequest: mem, memLimit: 0 });

describe("utilization", () => {
  it("sums requests and limits of pods that still hold resources", () => {
    const pods = [
      pod("node-a", true, 500, GI),
      pod("node-a", true, 250, GI),
      pod("node-a", false, 9000, 9 * GI), // Succeeded: frees its resources
      pod("node-b", true, 100, GI / 2),
      pod(null, true, 300, GI), // Pending, unscheduled: counted as a pod, on no node
    ];
    const u = utilization(nodes, pods, null);
    const [a, b] = u.nodes;
    expect([a?.pods, a?.requests.cpu, a?.limits.cpu, a?.requests.memory]).toEqual([2, 750, 1500, 2 * GI]);
    expect([b?.pods, b?.requests.cpu]).toEqual([1, 100]);
    expect(u.allocatable).toEqual({ cpu: 6000, memory: 24 * GI });
    expect([u.pods, u.podCapacity]).toEqual([4, 220]);
    expect(u.requests.cpu).toBe(850);
    expect(u.usage).toBeNull();
  });

  it("adds metrics-server usage when available", () => {
    const u = utilization(nodes, [], [
      { name: "node-a", cpu: 350, memory: 3 * GI },
      { name: "gone-node", cpu: 999, memory: GI },
    ]);
    expect(u.nodes[0]?.usage).toEqual({ cpu: 350, memory: 3 * GI });
    expect(u.nodes[1]?.usage).toBeNull();
    // Only nodes still in the cluster count.
    expect(u.usage).toEqual({ cpu: 350, memory: 3 * GI });
  });
});

describe("formatting", () => {
  it("uses millicores and binary memory units", () => {
    expect(formatCpu(2158.4)).toBe("2158m");
    expect(formatCpu(624.4)).toBe("624.40m");
    expect(formatCpu(2.336)).toBe("2.34m");
    expect(formatMemory(118.11 * GI)).toBe("118.11Gi");
    expect(formatMemory(67.14 * 1024 ** 2)).toBe("67.14Mi");
    expect(percent(1, 4)).toBe(25);
    expect(percent(5, 0)).toBe(0);
  });
});
