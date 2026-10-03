import { ChevronLeft, ChevronRight, Search } from "lucide-react";
import { type ReactNode, useEffect, useRef } from "react";

import { cx } from "../lib/cx";
import { type ResourceTab, useApp, useView, type View } from "../store";

const LOG_KINDS = new Set(["Pod", "Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob", "ReplicaSet"]);

export function TopBar() {
  const view = useView();
  const canBack = useApp((s) => s.index > 0);
  const canForward = useApp((s) => s.index < s.history.length - 1);
  const back = useApp((s) => s.back);
  const forward = useApp((s) => s.forward);
  const search = useApp((s) => s.search);
  const setSearch = useApp((s) => s.setSearch);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.metaKey && !e.ctrlKey) return;
      if (e.key === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      } else if (e.key === "[") {
        e.preventDefault();
        useApp.getState().back();
      } else if (e.key === "]") {
        e.preventDefault();
        useApp.getState().forward();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const searchable = view.name === "applications" || view.name === "resources";

  return (
    // "deep": empty space anywhere in the bar drags the window and a double-click zooms it,
    // like a native title bar; the buttons, tabs and search inside stay clickable.
    <header data-tauri-drag-region="deep" className="grid h-[52px] shrink-0 grid-cols-[1fr_auto_1fr] items-center px-4">
      <div className="flex items-center gap-1">
        <IconButton label="Back" disabled={!canBack} onClick={back}>
          <ChevronLeft size={17} />
        </IconButton>
        <IconButton label="Forward" disabled={!canForward} onClick={forward}>
          <ChevronRight size={17} />
        </IconButton>
      </div>

      <ViewSwitch view={view} />

      <div className="flex justify-end">
        {searchable && (
          <label className="flex h-7 w-[220px] items-center gap-1.5 rounded-md border border-line-strong bg-bg px-2 focus-within:border-accent">
            <Search size={13} className="shrink-0 text-muted" aria-hidden />
            <input
              ref={searchRef}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") setSearch("");
              }}
              placeholder="Search"
              aria-label="Search"
              spellCheck={false}
              className="min-w-0 flex-1 bg-transparent outline-none"
            />
          </label>
        )}
      </div>
    </header>
  );
}

function ViewSwitch({ view }: { view: View }) {
  const navigate = useApp((s) => s.navigate);
  const replace = useApp((s) => s.replace);

  if (view.name === "applications" || view.name === "resources") {
    return (
      <Segmented
        value={view.name}
        options={[
          { value: "applications", label: "Applications" },
          { value: "resources", label: "All Resources" },
        ]}
        onChange={(value) =>
          navigate(value === "applications" ? { name: "applications" } : { name: "resources", resource: null })
        }
      />
    );
  }
  if (view.name === "resource") {
    const tabs: { value: ResourceTab; label: string }[] = [
      { value: "overview", label: "Overview" },
      ...(LOG_KINDS.has(view.resource.kind) ? [{ value: "logs" as const, label: "Logs" }] : []),
      { value: "events", label: "Events" },
      { value: "yaml", label: "YAML" },
    ];
    return <Segmented value={view.tab} options={tabs} onChange={(tab) => replace({ ...view, tab })} />;
  }
  return null;
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
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="rounded-md p-1 text-muted enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-35"
    >
      {children}
    </button>
  );
}
