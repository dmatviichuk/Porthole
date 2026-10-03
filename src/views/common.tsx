import { TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";

import { cx } from "../lib/cx";
import { describeMetricsError } from "../lib/metricsStore";

export function Placeholder({ children }: { children: ReactNode }) {
  return <div className="flex h-full items-center justify-center px-8 text-center text-muted">{children}</div>;
}

const TONES = {
  failed: { role: "alert", box: "border-failed/30 bg-failed/8", icon: "text-failed" },
  warn: { role: "status", box: "border-progress/35 bg-progress/10", icon: "text-progress" },
} as const;

/** A failed watch keeps retrying and metrics keep polling; the banner disappears once they recover. */
export function Banner({
  children,
  tone = "failed",
  className = "mx-6 mb-2",
}: {
  children: ReactNode;
  tone?: keyof typeof TONES;
  className?: string;
}) {
  const { role, box, icon } = TONES[tone];
  return (
    <div role={role} className={cx("flex items-start gap-2 rounded-md border px-3 py-2", box, className)}>
      <TriangleAlert size={14} className={cx("mt-0.5 shrink-0", icon)} aria-hidden />
      <p className="selectable min-w-0 break-words">{children}</p>
    </div>
  );
}

/** Why usage is missing, wherever usage would be shown. */
export function MetricsWarning({ error, className }: { error: string | null; className?: string }) {
  if (!error) return null;
  return (
    <Banner tone="warn" className={className}>
      {describeMetricsError(error).message}
    </Banner>
  );
}

export function SectionTitle({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <div className="mb-3 flex items-baseline justify-between">
      <h2 className="text-[15px] font-semibold">{children}</h2>
      {aside}
    </div>
  );
}
