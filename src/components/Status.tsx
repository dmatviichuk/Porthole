import { Check, CircleDashed, Clock, Clock3, Minus, Pause, X } from "lucide-react";

import { cx } from "../lib/cx";
import type { Health } from "../lib/types";

const ICONS = {
  ok: { Icon: Check, className: "text-ok" },
  progress: { Icon: Clock3, className: "text-progress" },
  failed: { Icon: X, className: "text-failed" },
  done: { Icon: Check, className: "text-done" },
  ending: { Icon: CircleDashed, className: "text-ending" },
} as const;

// A check mark reads as "succeeded"; a CronJob waiting for its next run, or a suspended one,
// has done neither.
const BY_TEXT = new Map<string, { Icon: typeof Check; className: string }>([
  ["ok:Scheduled", { Icon: Clock, className: "text-ok" }],
  ["done:Suspended", { Icon: Pause, className: "text-done" }],
]);
const UNKNOWN = { Icon: Minus, className: "text-faint" };

/** Status cell: a coloured mark and the plain status text. */
export function Status({ health, text, className }: { health: Health | null; text: string | null; className?: string }) {
  if (!text) return null;
  const { Icon, className: tone } = health ? (BY_TEXT.get(`${health}:${text}`) ?? ICONS[health]) : UNKNOWN;
  return (
    <span className={cx("inline-flex min-w-0 items-center gap-1.5", className)}>
      <Icon size={13} strokeWidth={2.5} className={cx("shrink-0", tone)} aria-hidden />
      <span className="truncate">{text}</span>
    </span>
  );
}
