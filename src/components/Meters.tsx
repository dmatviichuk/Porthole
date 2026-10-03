import { cx } from "../lib/cx";
import { type Amounts, formatCpu, formatMemory, percent } from "../lib/utilization";

type Kind = keyof Amounts;

const TONE = {
  cpu: "bg-ok",
  memory: "bg-accent",
  neutral: "bg-text/70",
} as const;

export const formatAmount = (kind: Kind, value: number) => (kind === "cpu" ? formatCpu(value) : formatMemory(value));

/** Horizontal meter; at or over 100% it turns red (limits are often overcommitted). */
export function Bar({ value, tone, className }: { value: number; tone: keyof typeof TONE; className?: string }) {
  return (
    <div
      role="meter"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value)}
      className={cx("h-1.5 overflow-hidden rounded-full bg-selected", className)}
    >
      <div
        className={cx("h-full rounded-full", value >= 100 ? "bg-failed" : TONE[tone])}
        style={{ width: `${Math.min(100, Math.max(value > 0 ? 2 : 0, value))}%` }}
      />
    </div>
  );
}

/**
 * Small vertical gauge before a value. `fill` is the share of whatever the value is
 * measured against (a limit, a node); null draws an empty gauge. Near the top it turns red.
 */
export function Gauge({ fill, kind, children }: { fill: number | null; kind: Kind; children: React.ReactNode }) {
  const share = fill ?? 0;
  return (
    <span className="inline-flex items-center gap-2.5">
      <span className="relative inline-block h-6 w-2.5 overflow-hidden rounded-[3px] bg-selected" aria-hidden>
        <span
          className={cx("absolute inset-x-0 bottom-0", share >= 90 ? "bg-failed" : TONE[kind])}
          style={{ height: `${Math.min(100, Math.max(share > 0 ? 6 : 0, share))}%` }}
        />
      </span>
      {children}
    </span>
  );
}

/** One half of a cluster or node utilization card: allocatable, then usage, requests and limits. */
export function CapacityMeter({
  title,
  kind,
  totals,
}: {
  title: string;
  kind: Kind;
  totals: { allocatable: Amounts; usage: Amounts | null; requests: Amounts; limits: Amounts };
}) {
  const whole = totals.allocatable[kind];
  const rows: [string, number | null][] = [
    ["Usage", totals.usage ? totals.usage[kind] : null],
    ["Requests", totals.requests[kind]],
    ["Limits", totals.limits[kind]],
  ];
  return (
    <div className="space-y-3">
      <div className="flex items-baseline justify-between font-semibold">
        <span>{title}</span>
        <span>{formatAmount(kind, whole)}</span>
      </div>
      {rows.map(([label, value]) => (
        <div key={label} className="space-y-1.5">
          <div className="flex justify-between">
            <span className="text-muted">{label}</span>
            <span>{value === null ? "—" : `${formatAmount(kind, value)} · ${Math.round(percent(value, whole))}%`}</span>
          </div>
          <Bar value={value === null ? 0 : percent(value, whole)} tone={kind} />
        </div>
      ))}
    </div>
  );
}

/**
 * A pod's or workload's usage against what it asked for: CPU % and Memory % are usage of
 * requests (of limits when nothing is requested).
 */
export function UsageMeter({
  title,
  kind,
  usage,
  requests,
  limits,
}: {
  title: string;
  kind: Kind;
  usage: number | null;
  requests: number;
  limits: number;
}) {
  const basis = requests || limits;
  const share = usage !== null && basis > 0 ? percent(usage, basis) : null;
  const value = (v: number) => (v > 0 ? formatAmount(kind, v) : "Not set");
  return (
    <div className="space-y-2">
      <div className="flex items-baseline justify-between font-semibold">
        <span>{title} %</span>
        <span>{share === null ? "—" : `${share.toFixed(2)}%`}</span>
      </div>
      <Bar value={share ?? 0} tone={kind} />
      <dl className="grid grid-cols-[1fr_auto] gap-y-1 pt-2">
        <dt className="text-muted">Usage</dt>
        <dd className="text-right">{usage === null ? "—" : formatAmount(kind, usage)}</dd>
        <dt className="text-muted">Requested</dt>
        <dd className="text-right">{value(requests)}</dd>
        <dt className="text-muted">Limit</dt>
        <dd className="text-right">{value(limits)}</dd>
      </dl>
    </div>
  );
}

export function Card({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={cx("rounded-lg bg-raised px-6 py-5", className)}>{children}</div>;
}
