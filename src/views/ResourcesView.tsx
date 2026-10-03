import { RotateCw, Search } from "lucide-react";
import { useMemo, useState } from "react";

import { DataTable } from "../components/DataTable";
import { openResource, resourceMenu } from "../lib/actions";
import { columnsFor } from "../lib/columns";
import { cx } from "../lib/cx";
import { matches } from "../lib/format";
import { filterSections, POD, pluralTitle, sameType, sections, typeKey } from "../lib/kinds";
import { popupMenu } from "../lib/menu";
import { useDiscovery } from "../lib/discovery";
import type { ResourceInfo, ResourceType } from "../lib/types";
import { useResources } from "../lib/watchStore";
import { useApp } from "../store";
import { Banner, Placeholder } from "./common";

export function ResourcesView({ resource }: { resource: ResourceType | null }) {
  const context = useApp((s) => s.context);
  const namespace = useApp((s) => s.namespace);
  const search = useApp((s) => s.search);
  const replace = useApp((s) => s.replace);
  const discovery = useDiscovery(context);

  const selected: ResourceInfo | ResourceType =
    (resource && discovery.types.find((t) => sameType(t, resource))) ?? resource ?? POD;
  const groups = useMemo(() => sections(discovery.types), [discovery.types]);
  const [typeQuery, setTypeQuery] = useState("");
  const shown = useMemo(() => filterSections(groups, typeQuery), [groups, typeQuery]);

  const watch = useResources(context, selected, namespace);
  const showNamespace = selected.namespaced && namespace === null;
  const columns = useMemo(() => columnsFor(selected.kind, showNamespace), [selected.kind, showNamespace]);
  const rows = useMemo(
    () =>
      watch.rows.filter((r) =>
        matches(search, r.name, r.namespace, r.status, typeof r.fields.message === "string" ? r.fields.message : null),
      ),
    [watch.rows, search],
  );

  if (!context) return <Placeholder>Choose a cluster to browse.</Placeholder>;
  const title = pluralTitle(selected.kind);

  return (
    <div className="flex h-full">
      <nav className="w-[210px] shrink-0 overflow-y-auto border-r border-line px-2 pb-4" aria-label="Resource types">
        <div className="sticky top-0 z-10 bg-bg pb-2">
          <div className="flex items-center justify-between px-2 pt-1 pb-2">
            <span className="text-[11px] font-medium text-muted">Resource types</span>
            <button
              type="button"
              onClick={discovery.refresh}
              className="rounded p-1 text-muted hover:bg-hover hover:text-text"
              title="Rediscover resource types (new CRDs)"
              aria-label="Rediscover resource types"
            >
              <RotateCw size={12} />
            </button>
          </div>
          <label className="mx-1 flex h-7 items-center gap-1.5 rounded-md border border-line-strong px-2 focus-within:border-accent">
            <Search size={12} className="shrink-0 text-muted" aria-hidden />
            <input
              value={typeQuery}
              onChange={(e) => setTypeQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") setTypeQuery("");
                const first = shown[0]?.items[0];
                if (e.key === "Enter" && first) replace({ name: "resources", resource: first });
              }}
              placeholder="Find a type"
              aria-label="Find a resource type"
              spellCheck={false}
              className="min-w-0 flex-1 bg-transparent text-[12.5px] outline-none"
            />
          </label>
        </div>
        {discovery.error && <p className="selectable px-2 text-failed">{discovery.error}</p>}
        {typeQuery && shown.length === 0 && <p className="px-2 text-muted">No types match "{typeQuery}".</p>}
        {shown.map((section) => (
          <div key={section.title} className="mb-3">
            <h3 className="px-2 pb-1 text-[11px] text-faint">{section.title}</h3>
            <ul>
              {section.items.map((t) => (
                <li key={`${typeKey(t)}`}>
                  <button
                    type="button"
                    onClick={() => replace({ name: "resources", resource: t })}
                    aria-current={sameType(t, selected) ? "page" : undefined}
                    title={t.group ? `${t.kind} (${t.group}/${t.version})` : `${t.kind} (${t.version})`}
                    className={cx(
                      "w-full truncate rounded-md px-2 py-[3px] text-left",
                      sameType(t, selected) ? "bg-selected font-medium" : "text-text/90 hover:bg-hover",
                    )}
                  >
                    {pluralTitle(t.kind)}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </nav>

      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center justify-between px-6 pb-2">
          <h1 className="text-[15px] font-semibold">
            {title}
            <span className="ml-2 font-normal text-muted">{watch.synced ? rows.length : ""}</span>
          </h1>
        </div>
        {watch.error && <Banner>{watch.error}</Banner>}
        <div className="min-h-0 flex-1">
          <DataTable
            id={`resources.${typeKey(selected)}`}
            columns={columns}
            rows={rows}
            getRowId={(r) => r.uid}
            initialSort={selected.kind === "Event" ? { id: "lastSeen", desc: false } : { id: "name", desc: false }}
            onOpen={(r) => openResource(selected, r)}
            onMenu={(r) => void popupMenu(resourceMenu(context, selected, r))}
            empty={
              !watch.synced
                ? `Loading ${title.toLowerCase()}…`
                : search
                  ? `No ${title.toLowerCase()} match "${search}".`
                  : `No ${title.toLowerCase()}${selected.namespaced && namespace ? ` in ${namespace}` : ""}.`
            }
          />
        </div>
      </section>
    </div>
  );
}
