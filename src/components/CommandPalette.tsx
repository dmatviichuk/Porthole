import { Search } from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import { openResource } from "../lib/actions";
import { cx } from "../lib/cx";
import { useDiscovery } from "../lib/discovery";
import { CTRL, mod, MOD } from "../lib/keys";
import {
  apiVersion,
  CRONJOB,
  DAEMONSET,
  DEPLOYMENT,
  INGRESS,
  JOB,
  NAMESPACE,
  NODE,
  POD,
  pluralTitle,
  searchText,
  SERVICE,
  STATEFULSET,
} from "../lib/kinds";
import { type PaletteItem, rank } from "../lib/palette";
import type { ClusterLabel } from "../lib/clusterLabels";
import type { ThemePref } from "../lib/theme";
import type { ContextInfo, ResourceInfo, ResourceRow, ResourceType } from "../lib/types";
import { useResources } from "../lib/watchStore";
import { type ShellTab, useApp } from "../store";
import { focusShell, go } from "./Shortcuts";

type Props = { contexts: ContextInfo[]; onReload: () => void };

/** Cmd/Ctrl+K: one list of every place and command, filtered as you type. */
export function CommandPalette(props: Props) {
  const open = useApp((s) => s.overlay === "palette");
  return open ? <Palette {...props} /> : null;
}

/** Results per group while typing; objects get more room. Without a query nothing is cut. */
const limitFor = (group: string) => (group === "Open" ? 12 : 8);

function Palette({ contexts, onReload }: Props) {
  const context = useApp((s) => s.context);
  const namespace = useApp((s) => s.namespace);
  const labels = useApp((s) => s.labels);
  const theme = useApp((s) => s.theme);
  const shells = useApp((s) => s.shells);
  const activeShell = useApp((s) => s.activeShell);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const listId = useId();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  const discovery = useDiscovery(context);
  const namespaces = useResources(context, NAMESPACE, null);
  // The objects people jump to, from the watches the app keeps warm for the whole cluster.
  const deployments = useResources(context, DEPLOYMENT, null);
  const statefulSets = useResources(context, STATEFULSET, null);
  const daemonSets = useResources(context, DAEMONSET, null);
  const cronJobs = useResources(context, CRONJOB, null);
  const jobs = useResources(context, JOB, null);
  const pods = useResources(context, POD, null);
  const services = useResources(context, SERVICE, null);
  const ingresses = useResources(context, INGRESS, null);
  const nodes = useResources(context, NODE, null);

  useEffect(() => {
    dialogRef.current?.showModal();
  }, []);

  const typed = query.trim() !== "";

  const items = useMemo(() => {
    const objects: [ResourceType, ResourceRow[]][] = [
      [DEPLOYMENT, deployments.rows],
      [STATEFULSET, statefulSets.rows],
      [DAEMONSET, daemonSets.rows],
      [CRONJOB, cronJobs.rows],
      [JOB, jobs.rows],
      [POD, pods.rows],
      [SERVICE, services.rows],
      [INGRESS, ingresses.rows],
      [NODE, nodes.rows],
    ];
    return paletteItems({
      typed,
      contexts,
      onReload,
      context,
      namespace,
      labels,
      theme,
      shells,
      activeShell,
      types: discovery.types,
      rediscover: discovery.refresh,
      namespaces: namespaces.rows.map((r) => r.name).toSorted(),
      objects,
    });
  }, [
    typed,
    contexts,
    onReload,
    context,
    namespace,
    labels,
    theme,
    shells,
    activeShell,
    discovery,
    namespaces.rows,
    deployments.rows,
    statefulSets.rows,
    daemonSets.rows,
    cronJobs.rows,
    jobs.rows,
    pods.rows,
    services.rows,
    ingresses.rows,
    nodes.rows,
  ]);

  const shown = useMemo(() => rank(items, query, typed ? limitFor : () => Infinity), [items, query, typed]);

  const selected = Math.min(active, shown.length - 1);
  const optionId = (index: number) => `${listId}-${index}`;

  useEffect(() => {
    document.getElementById(optionId(selected))?.scrollIntoView({ block: "nearest" });
  });

  const close = () => dialogRef.current?.close();
  const run = (item: PaletteItem | undefined) => {
    if (!item) return;
    // Closing first hands focus back to where it was; a view the item opens then takes it.
    close();
    item.run();
  };

  return (
    <dialog
      ref={dialogRef}
      // A command may have opened the other overlay (Keyboard shortcuts); leave that one open.
      onClose={() => useApp.getState().overlay === "palette" && useApp.getState().setOverlay(null)}
      onClick={(e) => e.target === e.currentTarget && close()}
      aria-label="Command palette"
      className="mx-auto mt-[12vh] w-[560px] max-w-[calc(100vw-32px)] overflow-hidden rounded-xl border border-line-strong bg-bg p-0 text-text shadow-2xl"
    >
      <label className="flex items-center gap-2 border-b border-line px-4">
        <Search size={15} className="shrink-0 text-muted" aria-hidden />
        <input
          autoFocus
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={(e) => {
            const down = e.key === "ArrowDown" || (e.ctrlKey && e.key === "n");
            const up = e.key === "ArrowUp" || (e.ctrlKey && e.key === "p");
            if (down || up) {
              e.preventDefault();
              setActive((selected + (down ? 1 : -1) + shown.length) % Math.max(1, shown.length));
            } else if (e.key === "Enter") {
              e.preventDefault();
              run(shown[selected]);
            } else if (mod(e) && e.key.toLowerCase() === "k") {
              e.preventDefault();
              close();
            }
          }}
          role="combobox"
          aria-expanded
          aria-controls={listId}
          aria-activedescendant={shown.length > 0 ? optionId(selected) : undefined}
          placeholder="Go to a view, type, namespace, object or cluster…"
          spellCheck={false}
          className="h-12 min-w-0 flex-1 bg-transparent text-[14px] outline-none"
        />
      </label>
      <ul id={listId} role="listbox" className="max-h-[min(60vh,480px)] overflow-y-auto py-1.5">
        {shown.length === 0 && <li className="px-4 py-3 text-muted">Nothing matches "{query}".</li>}
        {shown.map((item, index) => (
          <li key={item.id} role="presentation">
            {item.group !== shown[index - 1]?.group && (
              <div className="px-4 pt-2 pb-1 text-[11px] font-medium text-muted">{item.group}</div>
            )}
            <div
              id={optionId(index)}
              role="option"
              aria-selected={index === selected}
              onMouseMove={() => index !== selected && setActive(index)}
              onClick={() => run(item)}
              className={cx("mx-1.5 flex items-center gap-3 rounded-md px-2.5 py-1.5", index === selected && "bg-selected")}
            >
              <span className="min-w-0 truncate">{item.label}</span>
              {item.detail && <span className="min-w-0 shrink truncate text-muted">{item.detail}</span>}
              {item.hint && <kbd className="ml-auto shrink-0 font-sans text-xs text-muted">{item.hint}</kbd>}
            </div>
          </li>
        ))}
      </ul>
    </dialog>
  );
}

type Sources = {
  /** Types, namespaces and objects are listed only once something is typed. */
  typed: boolean;
  contexts: ContextInfo[];
  onReload: () => void;
  context: string | null;
  namespace: string | null;
  labels: Record<string, ClusterLabel>;
  theme: ThemePref;
  shells: ShellTab[];
  activeShell: string | null;
  types: ResourceInfo[];
  rediscover: () => void;
  namespaces: string[];
  objects: [ResourceType, ResourceRow[]][];
};

function paletteItems({
  typed,
  contexts,
  onReload,
  context,
  namespace,
  labels,
  theme,
  shells,
  activeShell,
  types,
  rediscover,
  namespaces,
  objects,
}: Sources): PaletteItem[] {
  const app = useApp.getState();
  const list: PaletteItem[] = [
    { id: "go/applications", group: "Go to", label: "Applications", hint: `${MOD}1`, run: () => go({ name: "applications" }) },
    {
      id: "go/resources",
      group: "Go to",
      label: "All Resources",
      hint: `${MOD}2`,
      run: () => go({ name: "resources", resource: null }),
    },
    { id: "go/overview", group: "Go to", label: "Overview", hint: `${MOD}3`, run: () => go({ name: "overview" }) },
  ];
  if (typed) {
    for (const t of types) {
      list.push({
        id: `type/${t.plural}.${t.group}`,
        group: "Resource types",
        label: pluralTitle(t.kind),
        detail: apiVersion(t),
        keywords: searchText(t),
        run: () => app.navigate({ name: "resources", resource: t }),
      });
    }
    list.push({
      id: "ns/*",
      group: "Namespaces",
      label: "All namespaces",
      detail: namespace === null ? "Current" : undefined,
      keywords: "namespace ns",
      run: () => app.browseNamespace(null),
    });
    for (const ns of namespaces) {
      list.push({
        id: `ns/${ns}`,
        group: "Namespaces",
        label: ns,
        detail: ns === namespace ? "Namespace, current" : "Namespace",
        keywords: "ns",
        run: () => app.browseNamespace(ns),
      });
    }
    for (const [type, rows] of objects) {
      for (const row of rows) {
        list.push({
          id: `open/${type.kind}/${row.uid}`,
          group: "Open",
          label: row.name,
          detail: row.namespace ? `${type.kind} · ${row.namespace}` : type.kind,
          run: () => openResource(type, row),
        });
      }
    }
  }
  for (const c of contexts) {
    const tag = labels[c.name]?.tag;
    list.push({
      id: `cluster/${c.name}`,
      group: "Clusters",
      label: c.name,
      detail: [tag, c.name === context ? "Current" : null].filter(Boolean).join(", ") || undefined,
      keywords: "cluster context",
      run: () => app.setContext(c.name),
    });
  }
  for (const shell of shells) {
    list.push({
      id: `shell/${shell.key}`,
      group: "Shells",
      label: `${shell.pod} · ${shell.container}`,
      detail: shell.key === activeShell ? "Shell, current" : "Shell",
      keywords: `shell terminal ${shell.namespace}`,
      hint: shell.key === activeShell ? `${CTRL}\`` : undefined,
      run: () => focusShell(shell.key),
    });
  }
  const current = shells.find((s) => s.key === activeShell);
  list.push(
    { id: "cmd/shortcuts", group: "Commands", label: "Keyboard shortcuts", hint: "?", run: () => app.setOverlay("shortcuts") },
    ...(current
      ? [
          {
            id: "cmd/close-shell",
            group: "Commands",
            label: `Close shell ${current.pod} · ${current.container}`,
            keywords: "terminal",
            run: () => app.closeShell(current.key),
          },
        ]
      : []),
    ...(context
      ? [
          {
            id: "cmd/label",
            group: "Commands",
            label: labels[context] ? "Edit cluster label…" : "Label this cluster…",
            run: () => app.editLabel(context),
          },
        ]
      : []),
    { id: "cmd/reload", group: "Commands", label: "Reload kubeconfig", run: onReload },
    {
      id: "cmd/rediscover",
      group: "Commands",
      label: "Rediscover resource types",
      keywords: "crd custom refresh",
      run: rediscover,
    },
    ...(
      [
        ["light", "Light"],
        ["dark", "Dark"],
        ["auto", "Auto"],
      ] as const
    ).map(([value, name]) => ({
      id: `cmd/theme/${value}`,
      group: "Commands",
      label: `Theme: ${name}`,
      detail: value === theme ? "Current" : undefined,
      keywords: "appearance",
      run: () => app.setTheme(value),
    })),
  );
  return list;
}
