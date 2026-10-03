import { useState } from "react";

/** A boolean preference remembered in this browser profile; falls back to `initial` without storage. */
export function useBooleanPref(key: string, initial: boolean) {
  const storageKey = `porthole.pref.${key}`;
  const [value, setValue] = useState(() => {
    try {
      const stored = localStorage.getItem(storageKey);
      return stored === null ? initial : stored === "true";
    } catch {
      return initial;
    }
  });
  const update = (next: boolean) => {
    setValue(next);
    try {
      localStorage.setItem(storageKey, String(next));
    } catch {
      // Storage blocked: the choice lasts for this view only.
    }
  };
  return [value, update] as const;
}
