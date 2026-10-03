import { type ReactNode, useMemo, useState } from "react";

import { Status } from "../components/Status";
import { openResource } from "../lib/actions";
import { Age } from "../lib/columns";
import { cx } from "../lib/cx";
import { CONFIGMAP, INGRESS, PVC, SERVICE } from "../lib/kinds";
import type { ResourceRow, ResourceType } from "../lib/types";
import { useResources } from "../lib/watchStore";
import { SectionTitle } from "./common";

type PortSpec = { name: string | null; port: number; targetPort: number | string | null; nodePort: number | null; protocol: string };

const list = (row: ResourceRow, key: string): string[] =>
  Array.isArray(row.fields[key]) ? (row.fields[key] as string[]) : [];

/** A Service relates to the pods its selector picks; an empty selector picks nothing. */
export function selects(selector: Record<string, string>, labels: Record<string, string>): boolean {
  const entries = Object.entries(selector);
  return entries.length > 0 && entries.every(([k, v]) => labels[k] === v);
}

/** Services, Ingresses, Config Maps and claims that belong to a set of pods. */
export function RelatedSections({
  context,
  namespace,
  pods,
}: {
  context: string;
  namespace: string | null;
  pods: ResourceRow[];
}) {
  const services = useResources(context, SERVICE, namespace);
  const ingresses = useResources(context, INGRESS, namespace);
  const claims = useResources(context, PVC, namespace);

  const related = useMemo(() => {
    const svc = services.rows.filter((s) => {
      const selector = (s.fields.selector ?? {}) as Record<string, string>;
      return pods.some((p) => selects(selector, p.labels));
    });
    const names = new Set(svc.map((s) => s.name));
    const ing = ingresses.rows.filter((i) => list(i, "backends").some((b) => names.has(b)));
    const configMaps = [...new Set(pods.flatMap((p) => list(p, "configMaps")))].toSorted();
    const pvcNames = new Set(pods.flatMap((p) => list(p, "pvcs")));
    const pvcs = claims.rows.filter((c) => pvcNames.has(c.name));
    return { svc, ing, configMaps, pvcs };
  }, [services.rows, ingresses.rows, claims.rows, pods]);

  return (
    <>
      <Related title="Services" empty="No Services" resource={SERVICE} rows={related.svc} columns={serviceColumns} />
      <Related title="Ingresses" empty="No Ingresses" resource={INGRESS} rows={related.ing} columns={ingressColumns} />
      <section className="mb-8">
        <SectionTitle>Config Maps</SectionTitle>
        {related.configMaps.length === 0 ? (
          <p className="text-muted">No Config Maps</p>
        ) : (
          <div>
            <Header columns={[["Name", "220px"], ["Data", "1fr"]]} />
            {related.configMaps.map((name) => (
              <ConfigMapRow key={name} context={context} namespace={namespace} name={name} />
            ))}
          </div>
        )}
      </section>
      <Related
        title="Persistent Volume Claims"
        empty="No Persistent Volume Claims"
        resource={PVC}
        rows={related.pvcs}
        columns={pvcColumns}
      />
    </>
  );
}

type MiniColumn = { header: string; width: string; cell: (row: ResourceRow) => ReactNode };

function Header({ columns }: { columns: [string, string][] }) {
  return (
    <div
      className="grid border-b border-line pb-2 text-xs text-muted"
      style={{ gridTemplateColumns: columns.map(([, w]) => w).join(" ") }}
    >
      {columns.map(([title]) => (
        <span key={title}>{title}</span>
      ))}
    </div>
  );
}

function Related({
  title,
  empty,
  resource,
  rows,
  columns,
}: {
  title: string;
  empty: string;
  resource: ResourceType;
  rows: ResourceRow[];
  columns: MiniColumn[];
}) {
  const template = columns.map((c) => c.width).join(" ");
  return (
    <section className="mb-8">
      <SectionTitle>{title}</SectionTitle>
      {rows.length === 0 ? (
        <p className="text-muted">{empty}</p>
      ) : (
        <div>
          <Header columns={columns.map((c) => [c.header, c.width])} />
          {rows.map((row) => (
            <div
              key={row.uid}
              role="button"
              tabIndex={0}
              onClick={() => openResource(resource, row)}
              onKeyDown={(e) => e.key === "Enter" && openResource(resource, row)}
              className="grid min-h-11 items-center border-b border-line py-2 hover:bg-hover"
              style={{ gridTemplateColumns: template }}
            >
              {columns.map((c) => (
                <div key={c.header} className="min-w-0 pr-4">
                  {c.cell(row)}
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

const part = (label: string, value: ReactNode) => (
  <span className="mr-6 whitespace-nowrap">
    <span className="text-muted">{label}</span> <span className="font-mono">{value}</span>
  </span>
);

function PortLine({ port }: { port: PortSpec }) {
  return (
    <div>
      {part("Port", `${port.port}${port.protocol !== "TCP" ? `/${port.protocol}` : ""}`)}
      {port.targetPort !== null && part("Target Port", port.targetPort)}
      {port.nodePort !== null && part("Node Port", port.nodePort)}
    </div>
  );
}

const serviceColumns: MiniColumn[] = [
  { header: "Name", width: "minmax(160px, 1fr)", cell: (r) => r.name },
  { header: "Type", width: "120px", cell: (r) => String(r.fields.type ?? "") },
  {
    header: "Ports",
    width: "minmax(240px, 3fr)",
    cell: (r) =>
      (Array.isArray(r.fields.portList) ? (r.fields.portList as PortSpec[]) : []).map((p) => (
        <PortLine key={`${p.port}/${p.protocol}`} port={p} />
      )),
  },
];

const ingressColumns: MiniColumn[] = [
  { header: "Name", width: "minmax(160px, 1fr)", cell: (r) => r.name },
  { header: "Class", width: "120px", cell: (r) => String(r.fields.class ?? "") },
  { header: "Hosts", width: "minmax(200px, 2fr)", cell: (r) => String(r.fields.hosts ?? "") },
  { header: "Address", width: "minmax(160px, 1.5fr)", cell: (r) => String(r.fields.address ?? "") },
];

const pvcColumns: MiniColumn[] = [
  { header: "Name", width: "minmax(180px, 2fr)", cell: (r) => r.name },
  { header: "Status", width: "120px", cell: (r) => <Status health={r.health} text={r.status} /> },
  { header: "Capacity", width: "100px", cell: (r) => String(r.fields.capacity ?? "") },
  { header: "Storage class", width: "140px", cell: (r) => String(r.fields.storageClass ?? "") },
  { header: "Age", width: "72px", cell: (r) => <Age created={r.created} /> },
];

/** One referenced ConfigMap with its data; watched by name so other ConfigMaps' data stays out. */
function ConfigMapRow({ context, namespace, name }: { context: string; namespace: string | null; name: string }) {
  const watch = useResources(context, CONFIGMAP, namespace, `metadata.name=${name}`, true);
  const row = watch.rows[0];
  const data = (row?.fields.data ?? {}) as Record<string, string>;
  const binary = row && Array.isArray(row.fields.binaryKeys) ? (row.fields.binaryKeys as string[]) : [];
  const entries = Object.entries(data).toSorted(([a], [b]) => a.localeCompare(b));
  return (
    <div className="grid grid-cols-[220px_1fr] border-b border-line py-3">
      <button
        type="button"
        onClick={() => row && openResource(CONFIGMAP, row)}
        className={cx("self-start truncate pr-4 text-left", row ? "hover:underline" : "text-muted")}
      >
        {name}
      </button>
      <div className="min-w-0 space-y-3">
        {!watch.synced ? null : !row ? (
          <span className="text-failed">Not found in this namespace</span>
        ) : entries.length === 0 && binary.length === 0 ? (
          <span className="text-muted">No data</span>
        ) : (
          <>
            {entries.map(([key, value]) => (
              <DataEntry key={key} name={key} value={value} />
            ))}
            {binary.map((key) => (
              <DataEntry key={key} name={key} value="(binary data)" />
            ))}
          </>
        )}
      </div>
    </div>
  );
}

/** A key and its value; long values start clamped. */
function DataEntry({ name, value }: { name: string; value: string }) {
  const [open, setOpen] = useState(false);
  const long = value.split("\n").length > 12 || value.length > 1200;
  return (
    <div className="grid grid-cols-[minmax(120px,240px)_minmax(0,1fr)] gap-4 font-mono text-[12px] leading-5">
      <span className="selectable truncate font-semibold">{name}</span>
      <div className="min-w-0">
        <pre className={cx("selectable font-mono break-all whitespace-pre-wrap", long && !open && "line-clamp-12")}>
          {value}
        </pre>
        {long && (
          <button type="button" onClick={() => setOpen((o) => !o)} className="font-mono text-accent hover:underline">
            {open ? "Show less" : "Show more"}
          </button>
        )}
      </div>
    </div>
  );
}
