import { useSyncExternalStore } from "react";

import { listResources } from "./ipc";
import type { ResourceInfo } from "./types";

export type DiscoverySnapshot = {
  types: ResourceInfo[];
  /** This session's discovery has finished; until then `types` may be the saved list. */
  fresh: boolean;
  error: string | null;
};

const EMPTY: DiscoverySnapshot = { types: [], fresh: false, error: null };

/**
 * Resource types per context: the list saved last time shows at once, and discovery runs once
 * per session (or on request) to refresh it. Started as soon as a context is selected.
 */
class Discovery {
  snapshot: DiscoverySnapshot;
  private listeners = new Set<() => void>();
  private running = false;

  constructor(private readonly context: string) {
    this.snapshot = { ...EMPTY, types: saved(context) ?? [] };
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    if (!this.snapshot.fresh) this.refresh(false);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = () => this.snapshot;

  refresh = (force = true) => {
    if (this.running) return;
    this.running = true;
    listResources(this.context, force).then(
      (types) => {
        this.running = false;
        save(this.context, types);
        this.publish({ types, fresh: true, error: null });
      },
      (err: unknown) => {
        this.running = false;
        this.publish({ ...this.snapshot, fresh: true, error: String(err) });
      },
    );
  };

  private publish(snapshot: DiscoverySnapshot) {
    this.snapshot = snapshot;
    for (const listener of this.listeners) listener();
  }
}

const storageKey = (context: string) => `porthole.discovery.${context}`;

function saved(context: string): ResourceInfo[] | null {
  try {
    const raw = localStorage.getItem(storageKey(context));
    return raw ? (JSON.parse(raw) as ResourceInfo[]) : null;
  } catch {
    return null;
  }
}

function save(context: string, types: ResourceInfo[]) {
  try {
    localStorage.setItem(storageKey(context), JSON.stringify(types));
  } catch {
    // Storage full or blocked: discovery simply runs again next time.
  }
}

const discoveries = new Map<string, Discovery>();

function discoveryFor(context: string) {
  let found = discoveries.get(context);
  if (!found) {
    found = new Discovery(context);
    discoveries.set(context, found);
  }
  return found;
}

const noSubscribe = () => () => undefined;
const emptySnapshot = () => EMPTY;

export function useDiscovery(context: string | null): DiscoverySnapshot & { refresh: () => void } {
  const discovery = context ? discoveryFor(context) : null;
  const snapshot = useSyncExternalStore(discovery?.subscribe ?? noSubscribe, discovery?.getSnapshot ?? emptySnapshot);
  return { ...snapshot, refresh: () => discovery?.refresh() };
}

/** Starts discovery for a context ahead of time and keeps it until the returned function runs. */
export function holdDiscovery(context: string): () => void {
  const unsubscribe = discoveryFor(context).subscribe(() => undefined);
  return () => {
    unsubscribe();
  };
}

/** After a kubeconfig reload, contexts may point elsewhere: discover again on next use. */
export function forgetDiscovery() {
  discoveries.clear();
}
