import { SquareTerminal, X } from "lucide-react";
import { lazy, type PointerEvent as ReactPointerEvent, Suspense } from "react";

import { cx } from "../lib/cx";
import { useApp } from "../store";

// xterm loads with the first shell; most sessions never open one.
const ShellView = lazy(() => import("./ShellView"));

/** Shell sessions, docked under the current view so they survive navigation. */
export function Dock() {
  const shells = useApp((s) => s.shells);
  const active = useApp((s) => s.activeShell);
  const height = useApp((s) => s.dockHeight);
  const setHeight = useApp((s) => s.setDockHeight);
  const setActive = useApp((s) => s.setActiveShell);
  const close = useApp((s) => s.closeShell);

  if (shells.length === 0) return null;

  const startResize = (e: ReactPointerEvent) => {
    const startY = e.clientY;
    const startHeight = height;
    const max = window.innerHeight * 0.8;
    const onMove = (ev: PointerEvent) => setHeight(Math.min(max, Math.max(140, startHeight + startY - ev.clientY)));
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  return (
    <section className="relative flex shrink-0 flex-col border-t border-line bg-bg" style={{ height }} aria-label="Shells">
      <div
        role="separator"
        aria-orientation="horizontal"
        aria-label="Resize shells"
        onPointerDown={startResize}
        className="absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize"
      />
      <div role="tablist" className="flex h-8 shrink-0 items-stretch overflow-x-auto border-b border-line bg-sidebar">
        {shells.map((shell) => (
          <div
            key={shell.key}
            className={cx(
              "group flex max-w-[260px] items-center gap-1.5 border-r border-line pr-1 pl-3",
              shell.key === active ? "bg-bg text-text" : "text-muted hover:text-text",
            )}
          >
            <button
              type="button"
              role="tab"
              aria-selected={shell.key === active}
              onClick={() => setActive(shell.key)}
              className="flex min-w-0 items-center gap-1.5 text-xs"
              title={`${shell.context} / ${shell.namespace} / ${shell.pod} / ${shell.container}`}
            >
              <SquareTerminal size={13} className="shrink-0" aria-hidden />
              <span className="truncate">
                {shell.pod}
                <span className="text-faint"> · {shell.container}</span>
              </span>
            </button>
            <button
              type="button"
              aria-label="Close shell"
              onClick={() => close(shell.key)}
              className="rounded p-0.5 opacity-60 hover:bg-selected hover:opacity-100"
            >
              <X size={12} />
            </button>
          </div>
        ))}
      </div>
      <div className="relative min-h-0 flex-1">
        <Suspense fallback={null}>
          {shells.map((shell) => (
            <ShellView key={shell.key} shell={shell} visible={shell.key === active} />
          ))}
        </Suspense>
      </div>
    </section>
  );
}
