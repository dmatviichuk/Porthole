import { useApp } from "../store";
import { POD } from "./kinds";
import type { MenuEntry } from "./menu";
import type { ContainerRow, ResourceInfo, ResourceRow, ResourceType } from "./types";

export const containersOf = (pod: ResourceRow): ContainerRow[] =>
  Array.isArray(pod.fields.containers) ? (pod.fields.containers as ContainerRow[]) : [];

export function openResource(resource: ResourceType, row: Pick<ResourceRow, "name" | "namespace">, tab: "overview" | "logs" | "yaml" = "overview") {
  useApp.getState().navigate({ name: "resource", resource, namespace: row.namespace, object: row.name, tab });
}

export function openShell(context: string, pod: ResourceRow, container: string) {
  useApp.getState().openShell({ context, namespace: pod.namespace ?? "default", pod: pod.name, container });
}

export function copyName(name: string) {
  navigator.clipboard.writeText(name).then(
    () => useApp.getState().notify("ok", `Copied ${name}`),
    () => useApp.getState().notify("error", `Could not copy ${name}`),
  );
}

const kindLabel = (kind: string) => kind.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();

/** Right-click menu for any resource row; the keyed entries also run from a focused row. */
export function resourceMenu(context: string, resource: ResourceType | ResourceInfo, row: ResourceRow): MenuEntry[] {
  const canDelete = !("verbs" in resource) || resource.verbs.includes("delete");
  const hasLogs = ["Pod", "Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob", "ReplicaSet"].includes(resource.kind);
  const entries: MenuEntry[] = [{ label: "Open", action: () => openResource(resource, row) }];
  if (hasLogs) entries.push({ label: "View logs", key: "l", action: () => openResource(resource, row, "logs") });
  if (resource.kind === POD.kind) {
    const running = containersOf(row).filter((c) => c.state === "Running");
    if (running.length === 1 && running[0]) {
      const only = running[0].name;
      entries.push({ label: "Open shell", key: "s", action: () => openShell(context, row, only) });
    } else if (running.length > 1) {
      entries.push({
        label: "Open shell",
        key: "s",
        items: running.map((c) => ({ label: c.name, action: () => openShell(context, row, c.name) })),
      });
    }
  }
  entries.push(
    { label: "Edit YAML", key: "y", action: () => openResource(resource, row, "yaml") },
    "separator",
    { label: "Copy name", key: "c", action: () => copyName(row.name) },
    "separator",
    {
      label: `Delete ${kindLabel(resource.kind)}…`,
      enabled: canDelete,
      action: () =>
        useApp.getState().requestDelete({ context, resource, namespace: row.namespace, name: row.name }),
    },
  );
  return entries;
}
