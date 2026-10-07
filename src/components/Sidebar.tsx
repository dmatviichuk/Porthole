import {
  ChartNoAxesColumn,
  ChevronsUpDown,
  Folder,
  Keyboard,
  Layers,
  LayoutGrid,
  Monitor,
  Moon,
  RotateCw,
  Sun,
} from "lucide-react";
import { type MouseEvent, type PointerEvent as ReactPointerEvent, type ReactNode, useMemo, useState } from "react";

import { LABEL_COLORS } from "../lib/clusterLabels";
import { cx } from "../lib/cx";
import { MOD } from "../lib/keys";
import { NAMESPACE } from "../lib/kinds";
import { popupMenu } from "../lib/menu";
import type { ThemePref } from "../lib/theme";
import type { ContextInfo } from "../lib/types";
import { useResources } from "../lib/watchStore";
import { useApp, useView } from "../store";

type Props = {
  contexts: ContextInfo[];
  onReload: () => void;
};

const DEFAULT_WIDTH = 236;
const MIN_WIDTH = 200;
const MAX_WIDTH = 560;
const WIDTH_KEY = "porthole.sidebarWidth";

const clampWidth = (width: number) => Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width)));

/** Sidebar width, remembered across launches. */
function useSidebarWidth() {
  const [width, setWidth] = useState(() => {
    try {
      const saved = Number(localStorage.getItem(WIDTH_KEY));
      return saved ? clampWidth(saved) : DEFAULT_WIDTH;
    } catch {
      return DEFAULT_WIDTH;
    }
  });
  const update = (next: number) => {
    const clamped = clampWidth(next);
    setWidth(clamped);
    try {
      localStorage.setItem(WIDTH_KEY, String(clamped));
    } catch {
      // Storage blocked: the width lasts for this session.
    }
  };
  return [width, update] as const;
}

/** Drag the sidebar's right edge to resize; double-click resets; arrow keys nudge it. */
function ResizeHandle({ width, onChange }: { width: number; onChange: (width: number) => void }) {
  const [dragging, setDragging] = useState(false);
  const startDrag = (e: ReactPointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = width;
    setDragging(true);
    document.body.style.cursor = "col-resize";
    const onMove = (ev: PointerEvent) => onChange(startWidth + ev.clientX - startX);
    const onUp = () => {
      setDragging(false);
      document.body.style.cursor = "";
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  // Invisible: the cursor says it can be dragged. While dragging, the sidebar's own border
  // brightens a little so you can see what moves.
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar (double-click to reset)"
      aria-valuemin={MIN_WIDTH}
      aria-valuemax={MAX_WIDTH}
      aria-valuenow={width}
      tabIndex={0}
      onPointerDown={startDrag}
      onDoubleClick={() => onChange(DEFAULT_WIDTH)}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft") onChange(width - 16);
        if (e.key === "ArrowRight") onChange(width + 16);
      }}
      className="group absolute inset-y-0 -right-1 z-20 flex w-2 cursor-col-resize justify-center outline-none"
    >
      <span className={cx("w-px", dragging ? "bg-line-strong" : "bg-transparent group-focus-visible:bg-line-strong")} />
    </div>
  );
}

export function Sidebar({ contexts, onReload }: Props) {
  const [width, setWidth] = useSidebarWidth();
  const context = useApp((s) => s.context);
  const namespace = useApp((s) => s.namespace);
  const browseNamespace = useApp((s) => s.browseNamespace);
  const browseAll = useApp((s) => s.browseAll);
  const navigate = useApp((s) => s.navigate);
  const view = useView();
  const namespaces = useResources(context, NAMESPACE, null);
  const names = useMemo(() => namespaces.rows.map((r) => r.name).toSorted(), [namespaces.rows]);

  const browsing = view.name === "applications" || view.name === "resources" || view.name === "resource";

  return (
    <aside className="relative flex shrink-0 flex-col border-r border-line bg-sidebar" style={{ width }}>
      <ResizeHandle width={width} onChange={setWidth} />
      {/* Room for the traffic lights; dragging here moves the window. */}
      <div data-tauri-drag-region className="h-12 shrink-0" />
      <ClusterSwitcher contexts={contexts} onReload={onReload} status={namespaces} />

      <nav className="space-y-0.5 px-2.5 pt-4" aria-label="Main">
        <NavItem icon={<LayoutGrid size={15} />} active={browsing} onClick={browseAll}>
          Browse
        </NavItem>
        <NavItem
          icon={<ChartNoAxesColumn size={15} />}
          active={view.name === "overview"}
          onClick={() => navigate({ name: "overview" })}
        >
          Overview
        </NavItem>
      </nav>

      <h2 className="px-5 pt-5 pb-1.5 text-[11px] font-medium text-muted">Namespaces</h2>
      {/* Pinned above the list, so it stays one click away however far the list scrolls. */}
      <div className="px-2.5 pb-0.5">
        <NavItem icon={<Layers size={14} />} active={namespace === null} onClick={() => browseNamespace(null)}>
          All namespaces
        </NavItem>
      </div>
      <ul className="min-h-0 flex-1 overflow-y-auto px-2.5 pb-3" aria-label="Namespaces">
        {names.map((name) => (
          <li key={name}>
            <NavItem
              icon={<Folder size={14} />}
              active={namespace === name}
              onClick={() => browseNamespace(name)}
              onContextMenu={(e) => {
                e.preventDefault();
                if (!context) return;
                void popupMenu([
                  { label: "Show only this namespace", action: () => browseNamespace(name) },
                  { label: "Copy name", action: () => void navigator.clipboard.writeText(name) },
                  "separator",
                  {
                    label: "Delete namespace…",
                    action: () => useApp.getState().requestDelete({ context, resource: NAMESPACE, namespace: null, name }),
                  },
                ]);
              }}
            >
              {name}
            </NavItem>
          </li>
        ))}
      </ul>

      <footer className="flex items-center justify-between border-t border-line px-3 py-2">
        <ThemeSwitch />
        <div className="flex">
          <button
            type="button"
            onClick={() => useApp.getState().setOverlay("shortcuts")}
            className="rounded-md p-1.5 text-muted hover:bg-selected hover:text-text"
            title={`Keyboard shortcuts (?) · Command palette (${MOD}K)`}
            aria-label="Keyboard shortcuts"
          >
            <Keyboard size={14} />
          </button>
          <button
            type="button"
            onClick={onReload}
            className="rounded-md p-1.5 text-muted hover:bg-selected hover:text-text"
            title="Reload kubeconfig"
            aria-label="Reload kubeconfig"
          >
            <RotateCw size={14} />
          </button>
        </div>
      </footer>
    </aside>
  );
}

function ClusterSwitcher({
  contexts,
  onReload,
  status,
}: Props & { status: { synced: boolean; error: string | null } }) {
  const context = useApp((s) => s.context);
  const setContext = useApp((s) => s.setContext);
  const labels = useApp((s) => s.labels);
  const editLabel = useApp((s) => s.editLabel);
  const label = context ? labels[context] : undefined;

  const [state, tone] = status.error
    ? /forbidden/i.test(status.error)
      ? ["Connected", "bg-ok"]
      : ["Can't connect", "bg-failed"]
    : status.synced
      ? ["Connected", "bg-ok"]
      : ["Connecting…", "bg-faint"];

  const openMenu = (anchor?: HTMLElement) =>
    void popupMenu(
      [
        ...contexts.map((c) => {
          const tag = labels[c.name]?.tag;
          return {
            label: tag ? `${c.name}  ·  ${tag}` : c.name,
            checked: c.name === context,
            action: () => setContext(c.name),
          };
        }),
        "separator",
        ...(context ? [{ label: label ? "Edit label…" : "Label this cluster…", action: () => editLabel(context) }] : []),
        { label: "Reload kubeconfig", action: onReload },
      ],
      anchor,
    );

  return (
    <div className="px-2.5">
      <button
        type="button"
        className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left hover:bg-selected"
        title={status.error ?? context ?? undefined}
        onClick={(e) => openMenu(e.currentTarget)}
        onContextMenu={(e) => {
          e.preventDefault();
          openMenu();
        }}
      >
        <span className="min-w-0 flex-1">
          <span className="block truncate font-semibold">{context ?? "No cluster"}</span>
          <span className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[11px] text-muted">
            <span className={cx("size-1.5 shrink-0 rounded-full", tone)} />
            {state}
            {label?.tag && (
              <>
                <span className="text-faint">·</span>
                <span className="truncate font-medium" style={{ color: LABEL_COLORS[label.color] }}>
                  {label.tag}
                </span>
              </>
            )}
          </span>
        </span>
        <ChevronsUpDown size={14} className="shrink-0 text-muted" aria-hidden />
      </button>
    </div>
  );
}

function NavItem({
  icon,
  active,
  onClick,
  onContextMenu,
  children,
}: {
  icon: ReactNode;
  active: boolean;
  onClick: () => void;
  onContextMenu?: (e: MouseEvent) => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      onContextMenu={onContextMenu}
      aria-current={active ? "page" : undefined}
      className={cx(
        "flex w-full items-center gap-2.5 rounded-md px-2.5 py-[5px] text-left",
        active ? "bg-selected font-medium text-text" : "text-text/90 hover:bg-hover",
      )}
    >
      <span className={cx("shrink-0", active ? "text-text" : "text-muted")}>{icon}</span>
      <span className="truncate">{children}</span>
    </button>
  );
}

const THEMES: { value: ThemePref; label: string; Icon: typeof Sun }[] = [
  { value: "light", label: "Light", Icon: Sun },
  { value: "dark", label: "Dark", Icon: Moon },
  { value: "auto", label: "Auto", Icon: Monitor },
];

function ThemeSwitch() {
  const theme = useApp((s) => s.theme);
  const setTheme = useApp((s) => s.setTheme);
  return (
    <div role="radiogroup" aria-label="Appearance" className="flex rounded-md bg-selected/60 p-0.5">
      {THEMES.map(({ value, label, Icon }) => (
        <button
          key={value}
          type="button"
          role="radio"
          aria-checked={theme === value}
          title={label}
          onClick={() => setTheme(value)}
          className={cx(
            "flex items-center gap-1 rounded px-2 py-1 text-[11px]",
            theme === value ? "bg-bg text-text shadow-sm" : "text-muted hover:text-text",
          )}
        >
          <Icon size={12} aria-hidden />
          {label}
        </button>
      ))}
    </div>
  );
}
