import { useMemo, useSyncExternalStore } from "react";

import { type Stop, type WatchOptions, watchResources } from "./ipc";
import { loadSnapshot, saveSnapshot } from "./snapshotCache";
import type { ResourceRow, ResourceType } from "./types";

export type WatchSnapshot = {
  rows: ResourceRow[];
  /** The live list has arrived; until then `rows` may be the snapshot saved last time. */
  synced: boolean;
  /** Last watch error; cleared by the next successful update. */
  error: string | null;
};

const EMPTY: WatchSnapshot = { rows: [], synced: false, error: null };

/**
 * Unused watches stay open this long, so moving between views, namespaces and back never
 * re-lists. The prewarmed core types never go idle while their context is selected.
 */
const KEEP_ALIVE_MS = 10 * 60_000;

/**
 * One backend watch shared by every component that asks for the same
 * (context, type, namespace, field selector, detail).
 */
class Watch {
  snapshot = EMPTY;
  private listeners = new Set<() => void>();
  private stop: Stop | null = null;
  private idle: number | undefined;

  constructor(
    private readonly key: string,
    private readonly start: (onEvent: Parameters<typeof watchResources>[3]) => Stop,
    /** Cluster-wide list watches are saved to disk and restored on the next open. */
    private readonly persist: boolean,
  ) {}

  subscribe = (listener: () => void) => {
    window.clearTimeout(this.idle);
    this.listeners.add(listener);
    if (!this.stop) this.open();
    return () => {
      this.listeners.delete(listener);
      if (this.listeners.size === 0) this.idle = window.setTimeout(this.close, KEEP_ALIVE_MS);
    };
  };

  getSnapshot = () => this.snapshot;

  private open() {
    const rows = new Map<string, ResourceRow>();
    let synced = false;
    if (this.persist) {
      void loadSnapshot(this.key).then((cached) => {
        // Only until the live list lands; never over fresher data.
        if (cached && !synced && this.stop) this.publish({ rows: cached, synced: false, error: this.snapshot.error });
      });
    }
    this.stop = this.start((event) => {
      switch (event.type) {
        case "reset":
          rows.clear();
          for (const row of event.items) rows.set(row.uid, row);
          synced = true;
          break;
        case "changes":
          for (const uid of event.deletes) rows.delete(uid);
          for (const row of event.upserts) rows.set(row.uid, row);
          break;
        case "error":
          this.publish({ ...this.snapshot, error: event.message });
          return;
      }
      const list = Array.from(rows.values());
      this.publish({ rows: list, synced, error: null });
      if (this.persist && synced) saveSnapshot(this.key, list);
    });
  }

  private publish(snapshot: WatchSnapshot) {
    this.snapshot = snapshot;
    for (const listener of this.listeners) listener();
  }

  close = () => {
    this.stop?.();
    this.stop = null;
    this.snapshot = EMPTY;
    // A reload may already have put a newer watch under the same key.
    if (watches.get(this.key) === this) watches.delete(this.key);
  };
}

const watches = new Map<string, Watch>();

function watchFor(context: string, resource: ResourceType, options: WatchOptions) {
  const namespace = resource.namespaced ? options.namespace : null;
  const { fieldSelector, detail } = options;
  const key = [context, resource.group, resource.version, resource.plural, namespace ?? "*", fieldSelector ?? "", detail].join("|");
  let watch = watches.get(key);
  if (!watch) {
    const persist = namespace === null && fieldSelector === null && !detail;
    watch = new Watch(key, (onEvent) => watchResources(context, resource, { namespace, fieldSelector, detail }, onEvent), persist);
    watches.set(key, watch);
  }
  return watch;
}

const noSubscribe = () => () => undefined;
const emptySnapshot = () => EMPTY;

function useWatch(watch: Watch | null): WatchSnapshot {
  return useSyncExternalStore(watch?.subscribe ?? noSubscribe, watch?.getSnapshot ?? emptySnapshot);
}

/** Without permission to list across namespaces, fall back to watching just the one picked. */
const isForbidden = (error: string | null) => error !== null && /forbidden/i.test(error);

/**
 * Live rows of one resource type; `namespace` null means all namespaces.
 *
 * Plain lists always share one cluster-wide watch and filter by namespace here, so changing
 * the namespace is instant instead of starting a new list request. `fieldSelector` and
 * `detail` (annotations, ConfigMap data) are for single objects and get their own watch.
 */
export function useResources(
  context: string | null,
  resource: ResourceType | null,
  namespace: string | null,
  fieldSelector: string | null = null,
  detail = false,
): WatchSnapshot {
  const scoped = fieldSelector !== null || detail;
  const primary =
    context && resource
      ? watchFor(context, resource, { namespace: scoped ? namespace : null, fieldSelector, detail })
      : null;
  const shared = useWatch(primary);

  const filterHere = !scoped && namespace !== null && resource?.namespaced === true;
  const fallback =
    filterHere && context && resource && isForbidden(shared.error)
      ? watchFor(context, resource, { namespace, fieldSelector: null, detail: false })
      : null;
  const own = useWatch(fallback);

  return useMemo(() => {
    if (fallback) return own;
    if (!filterHere) return shared;
    return { ...shared, rows: shared.rows.filter((row) => row.namespace === namespace) };
  }, [fallback, own, filterHere, shared, namespace]);
}

/** Keeps a cluster-wide list watch open (and warm) until the returned function is called. */
export function holdResources(context: string, resource: ResourceType): () => void {
  return watchFor(context, resource, { namespace: null, fieldSelector: null, detail: false }).subscribe(() => undefined);
}

/** Closes every watch, e.g. after the kubeconfig was reloaded and the backend dropped its sessions. */
export function closeAllWatches() {
  // Deleting the current entry while iterating a Map is safe.
  for (const watch of watches.values()) watch.close();
}
