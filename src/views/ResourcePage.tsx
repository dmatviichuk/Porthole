import { RotateCw, Trash2 } from "lucide-react";
import { Fragment, type ReactNode, useEffect, useMemo, useState } from "react";

import { type Column, DataTable } from "../components/DataTable";
import { Bar, CapacityMeter, Card, formatAmount, Gauge, UsageMeter } from "../components/Meters";
import { podLimits, podRequests, PodsTable, RowMenuButton } from "../components/PodsTable";
import { Status } from "../components/Status";
import { YamlEditor } from "../components/YamlEditor";
import { containersOf, openResource, openShell } from "../lib/actions";
import { appKey, buildApplications } from "../lib/apps";
import { Age, columnsFor } from "../lib/columns";
import { cx } from "../lib/cx";
import { getYaml, saveYaml } from "../lib/ipc";
import { focusOnMount } from "../lib/keys";
import { CRONJOB, DAEMONSET, DEPLOYMENT, EVENT, JOB, NODE, POD, REPLICASET, STATEFULSET } from "../lib/kinds";
import { type MenuEntry, popupMenu } from "../lib/menu";
import { podKey, useNodeUsage, usePodUsage } from "../lib/metricsStore";
import type { ContainerRow, Health, LogTarget, ResourceRow, ResourceType } from "../lib/types";
import { type Amounts, percent, type PodUsage, utilization } from "../lib/utilization";
import { useResources } from "../lib/watchStore";
import { useApp, type View } from "../store";
import { Banner, MetricsWarning, Placeholder, SectionTitle } from "./common";
import { LogsPanel } from "./LogsPanel";
import { RelatedSections } from "./Related";

type ResourceView = Extract<View, { name: "resource" }>;

const count = (rows: { ready: boolean }[]) => `${rows.filter((r) => r.ready).length}/${rows.length}`;

const OWNERS: Record<string, ResourceType> = {
  Deployment: DEPLOYMENT,
  StatefulSet: STATEFULSET,
  DaemonSet: DAEMONSET,
  ReplicaSet: REPLICASET,
  Job: JOB,
  CronJob: CRONJOB,
  Node: NODE,
};
/** Kinds whose page lists the pods they run. */
const POD_OWNERS = new Set(["Deployment", "StatefulSet", "DaemonSet", "ReplicaSet", "Job", "CronJob"]);

// A container that has not started has no logs to stream yet; it joins once it starts.
const NOT_STARTED = new Set([
  "Pending",
  "Waiting",
  "ContainerCreating",
  "PodInitializing",
  "ErrImagePull",
  "ImagePullBackOff",
  "CreateContainerConfigError",
  "CreateContainerError",
]);

function usePodsOf(context: string, resource: ResourceType, row: ResourceRow | null, namespace: string | null) {
  const wantsPods = POD_OWNERS.has(resource.kind);
  const pods = useResources(context, wantsPods ? POD : null, namespace);
  const jobs = useResources(context, resource.kind === "CronJob" ? JOB : null, namespace);
  return useMemo(() => {
    if (!row) return [];
    if (resource.kind === "Pod") return [row];
    if (resource.kind === "ReplicaSet") {
      return pods.rows.filter((p) => p.owner?.kind === "ReplicaSet" && p.owner.name === row.name);
    }
    if (!wantsPods) return [];
    const workloads = [{ resource, rows: [row] }, ...(resource.kind === "CronJob" ? [{ resource: JOB, rows: jobs.rows }] : [])];
    const key = appKey(resource.kind, row.namespace, row.name);
    return buildApplications(workloads, pods.rows).find((a) => a.key === key)?.pods ?? [];
  }, [row, resource, wantsPods, pods.rows, jobs.rows]);
}

export function ResourcePage({ view }: { view: ResourceView }) {
  const context = useApp((s) => s.context);
  const requestDelete = useApp((s) => s.requestDelete);
  const { resource, namespace, object, tab } = view;
  // The shared list already holds the object, so the page renders at once; the detail watch
  // adds annotations a moment later.
  const list = useResources(context, resource, namespace);
  const detail = useResources(context, resource, namespace, `metadata.name=${object}`, true);
  const listed = useMemo(() => list.rows.find((r) => r.name === object) ?? null, [list.rows, object]);
  const row = detail.rows[0] ?? listed;
  const pods = usePodsOf(context ?? "", resource, row, namespace);

  if (!context) return <Placeholder>Choose a cluster to browse.</Placeholder>;
  if (detail.synced && !row) {
    return (
      <Placeholder>
        {resource.kind} {object} no longer exists{namespace ? ` in ${namespace}` : ""}.
      </Placeholder>
    );
  }

  const subtitle =
    resource.kind === "Pod" && row
      ? `${count(containersOf(row).filter((c) => !c.init))} Containers`
      : POD_OWNERS.has(resource.kind)
        ? `${pods.filter((p) => p.health === "ok").length}/${pods.length} Pods`
        : null;

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-start justify-between gap-6 px-8 pt-1 pb-5">
        <div className="min-w-0">
          <h1 className="selectable truncate text-[22px] font-bold">{object}</h1>
          <p className="text-muted">{resource.kind}</p>
        </div>
        <div className="flex shrink-0 items-start gap-4">
          <div className="text-right">
            {row?.status && <Status health={row.health} text={row.status} className="font-medium" />}
            {subtitle && <p className="text-muted">{subtitle}</p>}
          </div>
          <button
            type="button"
            onClick={() => requestDelete({ context, resource, namespace, name: object })}
            className="flex items-center gap-1.5 rounded-md border border-line-strong px-2.5 py-1 text-[12.5px] text-failed hover:bg-failed/10"
          >
            <Trash2 size={13} aria-hidden />
            Delete
          </button>
        </div>
      </header>
      {detail.error && <Banner>{detail.error}</Banner>}
      <div className="min-h-0 flex-1">
        {!row ? (
          <Placeholder>Loading…</Placeholder>
        ) : tab === "logs" ? (
          <LogsPanel context={context} targets={logTargets(pods, resource.kind === "Pod")} />
        ) : tab === "events" ? (
          <EventsTab context={context} resource={resource} row={row} />
        ) : tab === "yaml" ? (
          <YamlTab context={context} resource={resource} namespace={namespace} name={object} />
        ) : (
          <div
            // The page takes focus so the arrow keys scroll it; Tab goes on to its tables.
            ref={focusOnMount}
            tabIndex={-1}
            data-primary
            className="h-full overflow-y-auto px-8 pb-10 outline-none"
          >
            <OverviewTab
              context={context}
              resource={resource}
              row={row}
              pods={pods}
              connected={detail.error === null}
            />
          </div>
        )}
      </div>
    </div>
  );
}

function logTargets(pods: ResourceRow[], includeInit: boolean): LogTarget[] {
  return pods.flatMap((pod) =>
    containersOf(pod)
      .filter((c) => (includeInit || !c.init) && !NOT_STARTED.has(c.state))
      .map((c) => ({ namespace: pod.namespace ?? "default", pod: pod.name, container: c.name })),
  );
}

function OverviewTab({
  context,
  resource,
  row,
  pods,
  connected,
}: {
  context: string;
  resource: ResourceType;
  row: ResourceRow;
  pods: ResourceRow[];
  /** False while the page shows a connection error; metrics fail with it and need no second banner. */
  connected: boolean;
}) {
  const podMetrics = usePodUsage(context);
  const nodeMetrics = useNodeUsage(context);
  if (resource.kind === "Node") {
    return (
      <>
        {connected && <MetricsWarning error={nodeMetrics.error ?? podMetrics.error} className="mb-5" />}
        <NodeOverview context={context} node={row} />
        <Details row={row} />
      </>
    );
  }
  const runsPods = resource.kind === "Pod" || POD_OWNERS.has(resource.kind);
  return (
    <>
      {connected && runsPods && <MetricsWarning error={podMetrics.error} className="mb-5" />}
      <div className={cx("mb-8 grid gap-5", runsPods && "xl:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]")}>
        <section className="flex flex-col">
          <SectionTitle>Overview</SectionTitle>
          <OverviewCard resource={resource} row={row} />
        </section>
        {runsPods && (
          <section className="flex flex-col">
            <SectionTitle>Utilization</SectionTitle>
            <PodsUtilization context={context} pods={pods} />
          </section>
        )}
      </div>
      {resource.kind === "Pod" ? (
        <ContainersTable context={context} pod={row} />
      ) : (
        runsPods && (
          <PodsTable
            context={context}
            id="resource.pods"
            title="Pods"
            pods={pods}
            empty="No pods"
          />
        )
      )}
      <Details row={row} />
      {runsPods && <RelatedSections context={context} namespace={row.namespace} pods={pods} />}
    </>
  );
}

type Image = { name: string | null; image: string | null };

function OverviewCard({ resource, row }: { resource: ResourceType; row: ResourceRow }) {
  const images: Image[] =
    resource.kind === "Pod"
      ? containersOf(row).map((c) => ({ name: c.name, image: c.image }))
      : Array.isArray(row.fields.images)
        ? (row.fields.images as Image[])
        : [];
  const owner = row.owner;
  const ownerType = owner ? OWNERS[owner.kind] : undefined;
  const node = typeof row.fields.node === "string" ? row.fields.node : null;
  const details = columnsFor(resource.kind, false).filter(
    (c) => !["name", "age", "status", "node", "ready", "restarts"].includes(c.id),
  );

  return (
    <dl className="grid flex-1 grid-cols-[120px_minmax(0,1fr)] content-start gap-x-6 gap-y-2 rounded-lg bg-raised px-6 py-5">
      <Field label="Kind">{resource.kind}</Field>
      {row.namespace && <Field label="Namespace">{row.namespace}</Field>}
      <Field label="Age">
        <Age created={row.created} />
      </Field>
      {images.length > 0 && (
        <Field label="Images" wrap>
          <span className="block font-mono text-[12px] leading-5">
            {images.map((i) => (
              <span key={`${i.name}/${i.image}`} className="block break-all">
                <span className="font-semibold">{i.name}</span> {i.image}
              </span>
            ))}
          </span>
        </Field>
      )}
      {node && (
        <Field label="Node">
          <LinkButton onClick={() => openResource(NODE, { name: node, namespace: null })}>{node}</LinkButton>
        </Field>
      )}
      {owner && (
        <Field label="Owner">
          {ownerType ? (
            <LinkButton onClick={() => openResource(ownerType, { name: owner.name, namespace: row.namespace })}>
              {owner.name}
            </LinkButton>
          ) : (
            owner.name
          )}{" "}
          <span className="text-muted">({owner.kind})</span>
        </Field>
      )}
      {details.map((c) => (
        <Field key={c.id} label={c.header}>
          {c.cell(row)}
        </Field>
      ))}
    </dl>
  );
}

function LinkButton({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" onClick={onClick} className="text-accent hover:underline">
      {children}
    </button>
  );
}

function Field({ label, wrap, children }: { label: string; wrap?: boolean; children: ReactNode }) {
  return (
    <>
      <dt className="text-muted">{label}</dt>
      <dd className={cx("selectable min-w-0", !wrap && "truncate")}>
        {children ?? <span className="text-faint">—</span>}
      </dd>
    </>
  );
}

/** Usage of what the pods requested; requests and limits summed over pods still running. */
function PodsUtilization({ context, pods }: { context: string; pods: ResourceRow[] }) {
  const usage = usePodUsage(context);
  const active = pods.filter((p) => p.fields.active === true);
  const sum = (pick: (p: ResourceRow) => Amounts) =>
    active.reduce((acc, p) => ({ cpu: acc.cpu + pick(p).cpu, memory: acc.memory + pick(p).memory }), { cpu: 0, memory: 0 });
  const requests = sum(podRequests);
  const limits = sum(podLimits);
  const samples = active.map((p) => usage.data?.get(podKey(p.namespace, p.name))).filter((u) => u !== undefined);
  const used =
    samples.length > 0
      ? samples.reduce((acc, u) => ({ cpu: acc.cpu + u.cpu, memory: acc.memory + u.memory }), { cpu: 0, memory: 0 })
      : null;
  return (
    <Card className="grid flex-1 grid-cols-2 content-start gap-8">
      <UsageMeter title="CPU" kind="cpu" usage={used?.cpu ?? null} requests={requests.cpu} limits={limits.cpu} />
      <UsageMeter
        title="Memory"
        kind="memory"
        usage={used?.memory ?? null}
        requests={requests.memory}
        limits={limits.memory}
      />
    </Card>
  );
}

function containerHealth(c: ContainerRow): Health {
  if (c.state === "Running") return c.ready ? "ok" : "progress";
  if (c.state === "Completed") return "done";
  return NOT_STARTED.has(c.state) ? "progress" : "failed";
}

/** A container's menu; L and S run its entries from the focused row. */
function containerMenu(context: string, pod: ResourceRow, c: ContainerRow): MenuEntry[] {
  return [
    { label: "View logs", key: "l", action: () => openResource(POD, pod, "logs") },
    { label: "Open shell", key: "s", enabled: c.state === "Running", action: () => openShell(context, pod, c.name) },
  ];
}

function containerColumns(context: string, pod: ResourceRow, sample: PodUsage | undefined): Column<ContainerRow>[] {
  const metric = (kind: keyof Amounts): Column<ContainerRow> => {
    const get = (c: ContainerRow) => {
      const value = sample?.containers.find((s) => s.name === c.name)?.[kind] ?? (sample ? 0 : null);
      const reference = c.limits[kind] || c.requests[kind];
      return { value, fill: value !== null && reference > 0 ? percent(value, reference) : null };
    };
    return {
      id: kind,
      header: kind === "cpu" ? "CPU Usage" : "Mem Usage",
      width: "130px",
      numeric: true,
      sortValue: (c) => get(c).value,
      cell: (c) => {
        const { value, fill } = get(c);
        return value === null ? (
          <span className="text-faint">—</span>
        ) : (
          <Gauge fill={fill} kind={kind}>
            {formatAmount(kind, value)}
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
      sortValue: (c) => c.name,
      cell: (c) => (
        <>
          {c.name}
          {c.init && <span className="ml-2 text-xs text-muted">init</span>}
        </>
      ),
    },
    { id: "restarts", header: "Restarts", width: "90px", numeric: true, sortValue: (c) => c.restarts, cell: (c) => c.restarts },
    metric("cpu"),
    metric("memory"),
    {
      id: "status",
      header: "Status",
      width: "minmax(140px, 1.4fr)",
      sortValue: (c) => c.state,
      cell: (c) => <Status health={containerHealth(c)} text={c.state} />,
    },
    {
      id: "actions",
      header: "",
      width: "40px",
      sortValue: () => null,
      cell: (c) => <RowMenuButton onOpen={(anchor) => void popupMenu(containerMenu(context, pod, c), anchor)} />,
    },
  ];
}

/** A pod's containers with live usage against each container's own limit (or request). */
function ContainersTable({ context, pod }: { context: string; pod: ResourceRow }) {
  const usage = usePodUsage(context);
  const sample = usage.data?.get(podKey(pod.namespace, pod.name));
  const containers = containersOf(pod);

  const columns = useMemo(() => containerColumns(context, pod, sample), [sample, context, pod]);

  return (
    <section className="mb-8">
      <SectionTitle>Containers</SectionTitle>
      <div className="-mx-6">
        <DataTable
          id="pod.containers"
          columns={columns}
          rows={containers}
          getRowId={(c) => c.name}
          rowHeight={44}
          fit
          menu={(c) => containerMenu(context, pod, c)}
          empty="No containers"
        />
      </div>
    </section>
  );
}

/** Labels and annotations side by side, one entry per line. */
function Details({ row }: { row: ResourceRow }) {
  return (
    <section className="mb-8">
      <SectionTitle>Details</SectionTitle>
      <div className="grid gap-x-10 gap-y-6 xl:grid-cols-2">
        <KeyValues label="Labels" values={row.labels} />
        {row.annotations && <KeyValues label="Annotations" values={row.annotations} />}
      </div>
    </section>
  );
}

function KeyValues({ label, values }: { label: string; values: Record<string, string> }) {
  const entries = Object.entries(values).toSorted(([a], [b]) => a.localeCompare(b));
  return (
    <div className="grid grid-cols-[110px_minmax(0,1fr)] gap-4">
      <h3 className="pt-1 text-muted">{label}</h3>
      {entries.length === 0 ? (
        <p className="pt-1 text-faint">None</p>
      ) : (
        <ul className="flex min-w-0 flex-col items-start gap-1.5">
          {entries.map(([key, value]) => (
            <KeyValue key={key} name={key} value={value} />
          ))}
        </ul>
      )}
    </div>
  );
}

/** Long values (last-applied-configuration, JSON blobs) start clamped to two lines. */
function KeyValue({ name, value }: { name: string; value: string }) {
  const [open, setOpen] = useState(false);
  const long = name.length + value.length > 120 || value.includes("\n");
  return (
    <li className="max-w-full rounded-md bg-raised px-2.5 py-1 font-mono text-[12px] leading-5">
      <span className={cx("selectable break-all", long && !open ? "line-clamp-2" : "whitespace-pre-wrap")}>
        {name}: {value}
      </span>
      {long && (
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          className="block font-mono text-[12px] font-semibold text-text hover:underline"
        >
          {open ? "Show less" : "Show more"}
        </button>
      )}
    </li>
  );
}

type Address = { type: string; address: string };

/** Node overview and utilization side by side, then the pods scheduled on it. */
function NodeOverview({ context, node }: { context: string; node: ResourceRow }) {
  const allPods = useResources(context, POD, null);
  const metrics = useNodeUsage(context);
  const pods = useMemo(() => allPods.rows.filter((p) => p.fields.node === node.name), [allPods.rows, node.name]);
  const totals = useMemo(() => utilization([node], pods, metrics.data), [node, pods, metrics.data]);
  const running = useMemo(() => pods.filter((p) => p.fields.active === true), [pods]);
  const f = node.fields;
  const addresses = Array.isArray(f.addresses) ? (f.addresses as Address[]) : [];
  const osInfo = [f.os, f.arch].filter(Boolean).join(" · ");
  const podShare = percent(running.length, totals.podCapacity);

  return (
    <>
      <div className="mb-8 grid gap-5 xl:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]">
        <section className="flex flex-col">
          <SectionTitle>Overview</SectionTitle>
          <dl className="grid flex-1 grid-cols-[120px_minmax(0,1fr)] content-start gap-x-6 gap-y-2 rounded-lg bg-raised px-6 py-5">
            <Field label="Kind">Node</Field>
            <Field label="Age">
              <Age created={node.created} />
            </Field>
            {f.roles ? <Field label="Roles">{String(f.roles)}</Field> : null}
            <Field label="Version">{String(f.version ?? "")}</Field>
            {f.instanceType ? <Field label="Instance type">{String(f.instanceType)}</Field> : null}
            <Field label="OS Image">{String(f.osImage ?? "")}</Field>
            <Field label="OS Info">{osInfo}</Field>
            <dt className="text-muted">Pods</dt>
            <dd className="flex items-center gap-4">
              <span>
                {running.length}/{totals.podCapacity} · {Math.round(podShare)}%
              </span>
              <Bar value={podShare} tone="neutral" className="w-40" />
            </dd>
            <dt className="text-muted">IP Addresses</dt>
            <dd className="selectable grid grid-cols-[max-content_minmax(0,1fr)] gap-x-6 font-mono text-[12px] leading-5">
              {addresses.map((a) => (
                <Fragment key={`${a.type}/${a.address}`}>
                  <span className="font-semibold">{a.type}</span>
                  <span className="truncate">{a.address}</span>
                </Fragment>
              ))}
            </dd>
          </dl>
        </section>
        <section className="flex flex-col">
          <SectionTitle>Utilization</SectionTitle>
          <Card className="grid flex-1 grid-cols-2 content-start gap-8">
            <CapacityMeter title="CPU" kind="cpu" totals={totals} />
            <CapacityMeter title="Memory" kind="memory" totals={totals} />
          </Card>
        </section>
      </div>
      <PodsTable
        context={context}
        id="node.pods"
        title="Scheduled Pods"
        pods={running}
        showNamespace
        empty={allPods.synced ? "No pods on this node." : "Loading pods…"}
      />
    </>
  );
}

function EventsTab({ context, resource, row }: { context: string; resource: ResourceType; row: ResourceRow }) {
  const events = useResources(context, EVENT, resource.namespaced ? row.namespace : null, `involvedObject.uid=${row.uid}`);
  const columns = useMemo(() => columnsFor("Event", false).filter((c) => c.id !== "object"), []);
  return (
    <div className="flex h-full flex-col">
      {events.error && <Banner>{events.error}</Banner>}
      <div className="min-h-0 flex-1">
        <DataTable
          id="resource.events"
          columns={columns}
          rows={events.rows}
          getRowId={(r) => r.uid}
          initialSort={{ id: "lastSeen", desc: false }}
          primary
          empty={events.synced ? "No events. Kubernetes keeps events for about an hour." : "Loading events…"}
        />
      </div>
    </div>
  );
}

type YamlProps = { context: string; resource: ResourceType; namespace: string | null; name: string };

/** Reloading remounts the editor with a fresh copy of the live object. */
function YamlTab(props: YamlProps) {
  const [generation, setGeneration] = useState(0);
  return <YamlDocument key={generation} {...props} reload={() => setGeneration((g) => g + 1)} />;
}

function YamlDocument({ context, resource, namespace, name, reload }: YamlProps & { reload: () => void }) {
  const notify = useApp((s) => s.notify);
  const [loaded, setLoaded] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let live = true;
    getYaml(context, resource, namespace, name).then(
      (yaml) => {
        if (!live) return;
        setLoaded(yaml);
        setDraft(yaml);
        setError(null);
      },
      (err: unknown) => live && setError(String(err)),
    );
    return () => {
      live = false;
    };
  }, [context, resource, namespace, name]);

  const dirty = loaded !== null && draft !== loaded;

  const save = async () => {
    if (!dirty || saving) return;
    setSaving(true);
    setError(null);
    try {
      await saveYaml(context, resource, namespace, name, draft);
      notify("ok", `Saved ${resource.kind.toLowerCase()} ${name}`);
      reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 px-8 pb-2">
        <button
          type="button"
          disabled={!dirty || saving}
          onClick={() => void save()}
          className="rounded-md bg-accent px-3 py-1 text-[12.5px] font-medium text-white disabled:opacity-40"
          title="Cmd+S"
        >
          {saving ? "Saving…" : "Save changes"}
        </button>
        <button
          type="button"
          disabled={!dirty}
          onClick={() => loaded !== null && setDraft(loaded)}
          className="rounded-md border border-line-strong px-3 py-1 text-[12.5px] enabled:hover:bg-hover disabled:opacity-40"
        >
          Discard
        </button>
        <button
          type="button"
          onClick={reload}
          className="flex items-center gap-1 rounded-md px-2 py-1 text-[12.5px] text-muted hover:bg-hover hover:text-text"
        >
          <RotateCw size={12} aria-hidden />
          Reload
        </button>
        <span className={cx("ml-auto text-xs", dirty ? "text-progress" : "text-muted")}>
          {dirty
            ? "Unsaved changes. Saving replaces the live object and fails if it changed since you loaded it."
            : "Edit and save to update the live object."}
        </span>
      </div>
      {error && <Banner>{error}</Banner>}
      <div className="min-h-0 flex-1 border-t border-line">
        {loaded === null && !error ? (
          <Placeholder>Loading YAML…</Placeholder>
        ) : (
          <YamlEditor value={draft} onChange={setDraft} onSave={() => void save()} />
        )}
      </div>
    </div>
  );
}
