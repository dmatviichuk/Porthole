import type { ResourceRow } from "./types";

/**
 * Last known rows of cluster-wide watches, kept in IndexedDB so the next launch (or the next
 * visit to a context) shows them instantly while the live watch catches up. Purely a
 * convenience: any failure just means starting empty, as without a cache.
 */
const DB_NAME = "porthole-cache";
const STORE = "snapshots";
/** Tables larger than this are not worth the write; they load live. */
const MAX_ROWS = 20_000;
/** Busy clusters change every second; write at most this often per watch. */
const WRITE_EVERY_MS = 5_000;

let database: Promise<IDBDatabase | null> | null = null;

function db(): Promise<IDBDatabase | null> {
  if (!database) {
    database = new Promise((resolve) => {
      try {
        const request = indexedDB.open(DB_NAME, 1);
        request.addEventListener("upgradeneeded", () => request.result.createObjectStore(STORE));
        request.addEventListener("success", () => resolve(request.result));
        request.addEventListener("error", () => resolve(null));
      } catch {
        resolve(null);
      }
    });
  }
  return database;
}

export async function loadSnapshot(key: string): Promise<ResourceRow[] | null> {
  const store = await db();
  if (!store) return null;
  return new Promise((resolve) => {
    try {
      const request = store.transaction(STORE, "readonly").objectStore(STORE).get(key);
      request.addEventListener("success", () =>
        resolve(Array.isArray(request.result) ? (request.result as ResourceRow[]) : null),
      );
      request.addEventListener("error", () => resolve(null));
    } catch {
      resolve(null);
    }
  });
}

const pending = new Map<string, ResourceRow[]>();
const timers = new Map<string, number>();
const written = new Set<string>();

function write(key: string) {
  window.clearTimeout(timers.get(key));
  timers.delete(key);
  const rows = pending.get(key);
  pending.delete(key);
  if (!rows) return;
  written.add(key);
  void db().then((store) => {
    try {
      store?.transaction(STORE, "readwrite").objectStore(STORE).put(rows, key);
    } catch {
      // Quota or a closed database: the cache is optional.
    }
  });
}

/**
 * Remembers the latest rows for `key`. The first list of a session is written at once (so
 * even a short session leaves a snapshot); later changes are batched to one write per few seconds.
 */
export function saveSnapshot(key: string, rows: ResourceRow[]) {
  if (rows.length > MAX_ROWS) return;
  pending.set(key, rows);
  if (!written.has(key)) write(key);
  else if (!timers.has(key)) timers.set(key, window.setTimeout(() => write(key), WRITE_EVERY_MS));
}

/** Writes whatever is still batched, e.g. when the window is closing. */
export function flushSnapshots() {
  // write() removes the entry; deleting the current key while iterating a Map is safe.
  for (const key of pending.keys()) write(key);
}

if (typeof window !== "undefined") {
  window.addEventListener("pagehide", flushSnapshots);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") flushSnapshots();
  });
}
