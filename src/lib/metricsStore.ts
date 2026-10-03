import { useSyncExternalStore } from "react";

import { nodeUsage, podUsage } from "./ipc";
import type { NodeUsage, PodUsage } from "./utilization";

export type MetricsSnapshot<T> = {
  data: T | null;
  /** Set when metrics-server is missing or failing. */
  error: string | null;
};

const EMPTY: MetricsSnapshot<never> = { data: null, error: null };

/** metrics-server samples every ~15 s; polling faster only repeats the same numbers. */
const EVERY_MS = 15_000;
const KEEP_ALIVE_MS = 10 * 60_000;

/** One poll loop per (context, query), shared by every component that shows it. */
class Poller<T> {
  snapshot: MetricsSnapshot<T> = EMPTY;
  private listeners = new Set<() => void>();
  private timer: number | undefined;
  private idle: number | undefined;

  constructor(
    private readonly key: string,
    private readonly load: () => Promise<T>,
  ) {}

  subscribe = (listener: () => void) => {
    window.clearTimeout(this.idle);
    this.listeners.add(listener);
    if (this.timer === undefined) {
      void this.poll();
      this.timer = window.setInterval(() => void this.poll(), EVERY_MS);
    }
    return () => {
      this.listeners.delete(listener);
      if (this.listeners.size === 0) this.idle = window.setTimeout(this.close, KEEP_ALIVE_MS);
    };
  };

  getSnapshot = () => this.snapshot;

  private async poll() {
    try {
      this.publish({ data: await this.load(), error: null });
    } catch (err) {
      this.publish({ data: null, error: String(err) });
    }
  }

  private publish(snapshot: MetricsSnapshot<T>) {
    this.snapshot = snapshot;
    for (const listener of this.listeners) listener();
  }

  private close = () => {
    window.clearInterval(this.timer);
    this.timer = undefined;
    if (pollers.get(this.key) === this) pollers.delete(this.key);
  };
}

const pollers = new Map<string, Poller<unknown>>();

function poller<T>(key: string, load: () => Promise<T>): Poller<T> {
  let found = pollers.get(key) as Poller<T> | undefined;
  if (!found) {
    found = new Poller(key, load);
    pollers.set(key, found as Poller<unknown>);
  }
  return found;
}

const noSubscribe = () => () => undefined;
const emptySnapshot = () => EMPTY;

function usePoller<T>(source: Poller<T> | null): MetricsSnapshot<T> {
  return useSyncExternalStore(source?.subscribe ?? noSubscribe, source?.getSnapshot ?? emptySnapshot);
}

const nodePoller = (context: string) => poller(`nodes|${context}`, () => nodeUsage(context));

export const podKey = (namespace: string | null, name: string) => `${namespace ?? ""}/${name}`;

/** Pod usage keyed by `namespace/name`, polled once for the whole cluster and shared by every view. */
const podPoller = (context: string) =>
  poller(`pods|${context}`, async () => {
    const list = await podUsage(context, null);
    return new Map(list.map((p) => [podKey(p.namespace, p.name), p]));
  });

export function useNodeUsage(context: string | null): MetricsSnapshot<NodeUsage[]> {
  return usePoller(context ? nodePoller(context) : null);
}

export function usePodUsage(context: string | null): MetricsSnapshot<Map<string, PodUsage>> {
  return usePoller(context ? podPoller(context) : null);
}

/** Keeps node and pod metrics polling for a context until the returned function is called. */
export function holdMetrics(context: string): () => void {
  const stops = [nodePoller(context).subscribe(() => undefined), podPoller(context).subscribe(() => undefined)];
  return () => {
    for (const stop of stops) stop();
  };
}

export type MetricsProblem = { kind: "missing" | "forbidden" | "unavailable"; message: string };

/** Turns a metrics.k8s.io failure into what the user can do about it. */
export function describeMetricsError(error: string): MetricsProblem {
  if (/could not find the requested resource|\bNotFound\b|\b404\b/.test(error)) {
    return {
      kind: "missing",
      message:
        "metrics-server isn't installed on this cluster, so CPU and memory usage can't be shown. Requests and limits are still shown.",
    };
  }
  if (/forbidden|cannot (list|get)/i.test(error)) {
    return {
      kind: "forbidden",
      message:
        "Your account can't read metrics (metrics.k8s.io) on this cluster, so CPU and memory usage can't be shown. Requests and limits are still shown.",
    };
  }
  return {
    kind: "unavailable",
    message: `metrics-server isn't responding, so CPU and memory usage can't be shown right now: ${error}`,
  };
}
