import { ChevronLeft, ChevronRight, Search } from "lucide-react";
import { type ReactNode, useRef } from "react";

import { cx } from "../lib/cx";
import { useFind } from "../lib/find";
import { focusPrimary, MOD } from "../lib/keys";
import { type ResourceTab, useApp, useView, type View } from "../store";

const LOG_KINDS = new Set(["Pod", "Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob", "ReplicaSet"]);

export function TopBar() {
  const view = useView();
  const canBack = useApp((s) => s.index > 0);
  const canForward = useApp((s) => s.index < s.history.length - 1);
  const back = useApp((s) => s.back);
  const forward = useApp((s) => s.forward);

  const searchable = view.name === "applications" || view.name === "resources";

  return (
    // "deep": empty space anywhere in the bar drags the window and a double-click zooms it,
    // like a native title bar; the buttons, tabs and search inside stay clickable.
    <header data-tauri-drag-region="deep" className="grid h-[52px] shrink-0 grid-cols-[1fr_auto_1fr] items-center px-4">
      <div className="flex items-center gap-1">
        <IconButton label="Back" shortcut={`${MOD}[`} disabled={!canBack} onClick={back}>
          <ChevronLeft size={17} />
        </IconButton>
        <IconButton label="Forward" shortcut={`${MOD}]`} disabled={!canForward} onClick={forward}>
          <ChevronRight size={17} />
        </IconButton>
      </div>

      <ViewSwitch view={view} />

      <div className="flex justify-end">{searchable && <SearchField />}</div>
    </header>
  );
}

function SearchField() {
  const search = useApp((s) => s.search);
  const setSearch = useApp((s) => s.setSearch);
  const ref = useRef<HTMLInputElement>(null);
  useFind(() => {
    ref.current?.focus();
    ref.current?.select();
  });

  return (
    <label className="flex h-7 w-[220px] items-center gap-1.5 rounded-md border border-line-strong bg-bg px-2 focus-within:border-accent">
      <Search size={13} className="shrink-0 text-muted" aria-hidden />
      <input
        ref={ref}
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape" && search) setSearch("");
          else if (e.key === "Escape" || e.key === "Enter" || e.key === "ArrowDown") {
            e.preventDefault();
            focusPrimary();
          }
        }}
        placeholder="Search"
        aria-label="Search"
        spellCheck={false}
        className="min-w-0 flex-1 bg-transparent outline-none"
      />
    </label>
  );
}

type Tabs = { value: string; options: { value: string; label: string }[]; select: (value: string) => void };

/** The tabs the top bar shows for a view: Applications / All Resources, or a resource's tabs. */
function tabsOf(view: View): Tabs | null {
  const { navigate, replace } = useApp.getState();
  if (view.name === "applications" || view.name === "resources") {
    return {
      value: view.name,
      options: [
        { value: "applications", label: "Applications" },
        { value: "resources", label: "All Resources" },
      ],
      select: (value) =>
        navigate(value === "applications" ? { name: "applications" } : { name: "resources", resource: null }),
    };
  }
  if (view.name === "resource") {
    const options: { value: ResourceTab; label: string }[] = [
      { value: "overview", label: "Overview" },
      ...(LOG_KINDS.has(view.resource.kind) ? [{ value: "logs" as const, label: "Logs" }] : []),
      { value: "events", label: "Events" },
      { value: "yaml", label: "YAML" },
    ];
    return { value: view.tab, options, select: (tab) => replace({ ...view, tab: tab as ResourceTab }) };
  }
  return null;
}

/** Ctrl+Tab and Ctrl+Shift+Tab: the next or previous tab of the current view, wrapping around. */
export function stepTab(delta: 1 | -1) {
  const { history, index } = useApp.getState();
  const view = history[index];
  const tabs = view && tabsOf(view);
  if (!tabs) return;
  const at = tabs.options.findIndex((o) => o.value === tabs.value);
  const next = tabs.options[(at + delta + tabs.options.length) % tabs.options.length];
  if (next) tabs.select(next.value);
}

function ViewSwitch({ view }: { view: View }) {
  const tabs = tabsOf(view);
  return tabs && <Segmented value={tabs.value} options={tabs.options} onChange={tabs.select} />;
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div role="tablist" className="flex rounded-lg border border-line bg-raised p-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="tab"
          aria-selected={value === option.value}
          onClick={() => onChange(option.value)}
          className={cx(
            "rounded-md px-4 py-1 text-[12.5px]",
            value === option.value ? "bg-selected font-medium text-text" : "text-muted hover:text-text",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function IconButton({
  label,
  shortcut,
  disabled,
  onClick,
  children,
}: {
  label: string;
  shortcut: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={`${label} (${shortcut})`}
      disabled={disabled}
      onClick={onClick}
      className="rounded-md p-1 text-muted enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-35"
    >
      {children}
    </button>
  );
}
