import { Check, X } from "lucide-react";

import { cx } from "../lib/cx";
import { useApp } from "../store";

export function Notices() {
  const notices = useApp((s) => s.notices);
  const dismiss = useApp((s) => s.dismiss);
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex w-[380px] flex-col gap-2" aria-live="polite">
      {notices.map((n) => (
        <div
          key={n.id}
          role={n.tone === "error" ? "alert" : "status"}
          className="pointer-events-auto flex items-start gap-2 rounded-lg border border-line-strong bg-raised px-3 py-2.5 shadow-lg"
        >
          {n.tone === "ok" ? (
            <Check size={14} className="mt-0.5 shrink-0 text-ok" aria-hidden />
          ) : (
            <X size={14} className="mt-0.5 shrink-0 text-failed" aria-hidden />
          )}
          <p className={cx("selectable min-w-0 flex-1 break-words")}>{n.text}</p>
          <button type="button" aria-label="Dismiss" onClick={() => dismiss(n.id)} className="text-muted hover:text-text">
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
