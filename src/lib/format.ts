import type { LogTarget } from "./types";

/** kubectl-style age: 45s, 12m, 3h20m, 5d, 2y. */
export function age(createdSeconds: number | null, nowMs: number): string {
  if (createdSeconds === null) return "";
  const s = Math.max(0, Math.floor(nowMs / 1000 - createdSeconds));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86_400) {
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    return h < 10 && m > 0 ? `${h}h${m}m` : `${h}h`;
  }
  if (s < 365 * 86_400) return `${Math.floor(s / 86_400)}d`;
  return `${Math.floor(s / (365 * 86_400))}y`;
}

/** 90 -> "1m30s", 7300 -> "2h1m". */
export function duration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m${seconds % 60 ? `${seconds % 60}s` : ""}`;
  const m = Math.floor((seconds % 3600) / 60);
  return `${Math.floor(seconds / 3600)}h${m ? `${m}m` : ""}`;
}

const pad = (n: number, width = 2) => String(n).padStart(width, "0");

/** `2026-10-03T20:41:00.123456789Z` -> `20:41:00.123` in local time. */
export function logTime(ts: string | null): string {
  if (!ts) return "";
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return ts;
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
}

export function dateTime(seconds: number | null): string {
  if (seconds === null) return "";
  return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "medium" });
}

/** Case-insensitive AND match of every space-separated term against any of the haystacks. */
export function matches(query: string, ...haystacks: (string | null | undefined)[]): boolean {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return true;
  const text = haystacks.filter(Boolean).join(" ").toLowerCase();
  return terms.every((term) => text.includes(term));
}

/**
 * Short source labels: pods of one workload share a long prefix (`storefront-79d4d85698-`),
 * so only the part that tells them apart is shown, plus the container when there are several.
 */
export function sourceLabels(targets: LogTarget[]): Map<string, string> {
  const pods = [...new Set(targets.map((t) => t.pod))];
  let prefix = "";
  if (pods.length > 1) {
    const [first = "", ...rest] = pods;
    let length = first.length;
    for (const pod of rest) {
      let i = 0;
      while (i < length && pod[i] === first[i]) i++;
      length = i;
    }
    // Cut at a dash so "web-7d9f-abc" and "web-7d9f-abd" keep "abc"/"abd", not "c"/"d".
    prefix = first.slice(0, first.lastIndexOf("-", length - 1) + 1);
  }
  const containers = new Set(targets.map((t) => t.container)).size > 1;
  return new Map(
    targets.map((t) => {
      const pod = t.pod.slice(prefix.length);
      const label = pods.length > 1 && containers ? `${pod}/${t.container}` : pods.length > 1 ? pod : t.container;
      return [`${t.namespace}/${t.pod}/${t.container}`, label];
    }),
  );
}
