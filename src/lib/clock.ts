import { useSyncExternalStore } from "react";

// One shared 1 s tick for every age cell instead of an interval per row.
let now = Date.now();
let timer: number | undefined;
const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  timer ??= window.setInterval(() => {
    now = Date.now();
    for (const l of listeners) l();
  }, 1000);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      window.clearInterval(timer);
      timer = undefined;
    }
  };
}

export const useNow = () => useSyncExternalStore(subscribe, () => now);
