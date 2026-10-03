import type { Column } from "../components/DataTable";
import { Status } from "../components/Status";
import { useNow } from "./clock";
import { age, duration } from "./format";
import type { ResourceRow } from "./types";

export function Age({ created }: { created: number | null }) {
  const now = useNow();
  return <>{age(created, now)}</>;
}

const text = (row: ResourceRow, key: string): string => {
  const value = row.fields[key];
  if (value === null || value === undefined) return "";
  return Array.isArray(value) ? value.join(", ") : String(value);
};

const num = (row: ResourceRow, key: string): number | null =>
  typeof row.fields[key] === "number" ? (row.fields[key] as number) : null;

export const nameColumn: Column<ResourceRow> = {
  id: "name",
  header: "Name",
  width: "minmax(220px, 3fr)",
  sortValue: (r) => r.name,
  cell: (r) => <span className="text-text">{r.name}</span>,
};

export const namespaceColumn: Column<ResourceRow> = {
  id: "namespace",
  header: "Namespace",
  width: "minmax(120px, 1.2fr)",
  sortValue: (r) => r.namespace,
  cell: (r) => r.namespace,
};

export const statusColumn: Column<ResourceRow> = {
  id: "status",
  header: "Status",
  width: "minmax(140px, 1.3fr)",
  sortValue: (r) => r.status,
  cell: (r) => <Status health={r.health} text={r.status} />,
};

export const ageColumn: Column<ResourceRow> = {
  id: "age",
  header: "Age",
  width: "72px",
  // Ascending age means newest first.
  sortValue: (r) => (r.created === null ? null : -r.created),
  numeric: true,
  cell: (r) => <Age created={r.created} />,
};

function field(id: string, header: string, width = "minmax(90px, 1fr)", numeric = false): Column<ResourceRow> {
  return {
    id,
    header,
    width,
    numeric,
    sortValue: (r) => (numeric ? num(r, id) : text(r, id)),
    cell: (r) => text(r, id),
  };
}

/** Sorts "3/5" by its ratio. */
function ratio(id: string, header: string): Column<ResourceRow> {
  return {
    id,
    header,
    width: "84px",
    numeric: true,
    sortValue: (r) => {
      const [a, b] = text(r, id).split("/").map(Number);
      return a !== undefined && b ? a / b : null;
    },
    cell: (r) => text(r, id),
  };
}

const timeAgo = (id: string, header: string): Column<ResourceRow> => ({
  id,
  header,
  width: "100px",
  numeric: true,
  sortValue: (r) => {
    const t = num(r, id);
    return t === null ? null : -t;
  },
  cell: (r) => <Age created={num(r, id)} />,
});

const KIND_COLUMNS: Record<string, Column<ResourceRow>[]> = {
  Pod: [
    ratio("ready", "Ready"),
    field("restarts", "Restarts", "84px", true),
    statusColumn,
    field("node", "Node", "minmax(140px, 1.4fr)"),
    field("ip", "IP", "120px"),
  ],
  Deployment: [ratio("ready", "Ready"), field("upToDate", "Up to date", "96px", true), field("available", "Available", "90px", true), statusColumn],
  StatefulSet: [ratio("ready", "Ready"), statusColumn],
  ReplicaSet: [ratio("ready", "Ready"), statusColumn],
  DaemonSet: [
    field("desired", "Desired", "80px", true),
    ratio("ready", "Ready"),
    field("upToDate", "Up to date", "96px", true),
    field("available", "Available", "90px", true),
    statusColumn,
  ],
  Job: [
    ratio("completions", "Completions"),
    {
      id: "duration",
      header: "Duration",
      width: "90px",
      numeric: true,
      sortValue: (r) => num(r, "duration"),
      cell: (r) => {
        const d = num(r, "duration");
        return d === null ? "" : duration(d);
      },
    },
    statusColumn,
  ],
  CronJob: [field("schedule", "Schedule", "minmax(120px, 1fr)"), field("active", "Active", "72px", true), timeAgo("lastSchedule", "Last schedule"), statusColumn],
  Service: [
    field("type", "Type", "110px"),
    field("clusterIP", "Cluster IP", "130px"),
    field("external", "External", "minmax(140px, 1.4fr)"),
    field("ports", "Ports", "minmax(120px, 1.2fr)"),
  ],
  Ingress: [field("class", "Class", "110px"), field("hosts", "Hosts", "minmax(160px, 2fr)"), field("address", "Address", "minmax(140px, 1.4fr)")],
  ConfigMap: [field("keys", "Keys", "72px", true)],
  Secret: [field("type", "Type", "minmax(160px, 1.5fr)"), field("keys", "Keys", "72px", true)],
  Namespace: [statusColumn],
  Node: [statusColumn, field("roles", "Roles", "minmax(110px, 1fr)"), field("version", "Version", "110px"), field("internalIP", "Internal IP", "130px"), field("instanceType", "Instance type", "130px")],
  PersistentVolumeClaim: [statusColumn, field("capacity", "Capacity", "90px"), field("storageClass", "Storage class", "130px"), field("volume", "Volume", "minmax(160px, 1.5fr)")],
  PersistentVolume: [statusColumn, field("capacity", "Capacity", "90px"), field("claim", "Claim", "minmax(160px, 1.5fr)"), field("storageClass", "Storage class", "130px"), field("reclaimPolicy", "Reclaim", "90px")],
};

const eventColumns: Column<ResourceRow>[] = [
  timeAgo("lastSeen", "Last seen"),
  {
    id: "reason",
    header: "Reason",
    width: "minmax(130px, 1fr)",
    sortValue: (r) => r.status,
    cell: (r) => <Status health={r.health === "failed" ? "failed" : null} text={r.status} />,
  },
  field("object", "Object", "minmax(160px, 1.4fr)"),
  { ...field("message", "Message", "minmax(260px, 4fr)"), cell: (r) => <span title={text(r, "message")}>{text(r, "message")}</span> },
  field("count", "Count", "64px", true),
];

/** Table columns for a kind; unknown kinds (custom resources) get name, namespace, status and age. */
export function columnsFor(kind: string, showNamespace: boolean): Column<ResourceRow>[] {
  if (kind === "Event") return showNamespace ? [...eventColumns.slice(0, 3), namespaceColumn, ...eventColumns.slice(3)] : eventColumns;
  const specific = KIND_COLUMNS[kind] ?? [statusColumn];
  return [nameColumn, ...(showNamespace ? [namespaceColumn] : []), ...specific, ageColumn];
}
