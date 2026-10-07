import { useEffect, useRef } from "react";

import { SHORTCUTS } from "../lib/keys";
import { useApp } from "../store";

/** "?": every keyboard shortcut. */
export function ShortcutsDialog() {
  const open = useApp((s) => s.overlay === "shortcuts");
  return open ? <Shortcuts /> : null;
}

function Shortcuts() {
  const dialogRef = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    dialogRef.current?.showModal();
  }, []);
  const close = () => dialogRef.current?.close();

  return (
    <dialog
      ref={dialogRef}
      onClose={() => useApp.getState().overlay === "shortcuts" && useApp.getState().setOverlay(null)}
      onClick={(e) => e.target === e.currentTarget && close()}
      aria-labelledby="shortcuts-title"
      className="m-auto w-[640px] max-w-[calc(100vw-32px)] rounded-xl border border-line-strong bg-bg p-0 text-text shadow-2xl"
    >
      <div className="max-h-[80vh] overflow-y-auto p-5">
        <div className="mb-4 flex items-center justify-between">
          <h2 id="shortcuts-title" className="text-[15px] font-semibold">
            Keyboard shortcuts
          </h2>
          <button
            type="button"
            autoFocus
            onClick={close}
            className="rounded-md border border-line-strong px-3 py-1 text-[12.5px] hover:bg-hover"
          >
            Done
          </button>
        </div>
        <div className="grid gap-x-8 gap-y-5 sm:grid-cols-2">
          {SHORTCUTS.map((section) => (
            <section key={section.title}>
              <h3 className="mb-1.5 text-[11px] font-medium text-muted">{section.title}</h3>
              <dl className="space-y-1.5">
                {section.items.map((item) => (
                  <div key={item.label} className="flex items-start justify-between gap-4">
                    <dt className="text-text/90">{item.label}</dt>
                    <dd className="flex shrink-0 gap-1">
                      {item.keys.map((key) => (
                        <kbd
                          key={key}
                          className="rounded border border-line-strong bg-raised px-1.5 font-sans text-[11.5px] leading-5"
                        >
                          {key}
                        </kbd>
                      ))}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </div>
    </dialog>
  );
}
