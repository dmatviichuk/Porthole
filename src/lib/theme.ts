import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ThemePref = "light" | "dark" | "auto";

const KEY = "porthole.theme";

export function savedTheme(): ThemePref {
  try {
    const value = localStorage.getItem(KEY);
    return value === "light" || value === "dark" ? value : "auto";
  } catch {
    return "auto";
  }
}

export function applyTheme(pref: ThemePref) {
  document.documentElement.dataset.theme = pref;
  try {
    localStorage.setItem(KEY, pref);
  } catch {
    // Storage blocked: the choice lasts for this session only.
  }
  // Native chrome (traffic lights, context menus, scrollbars) follows the same choice.
  if (isTauri()) void getCurrentWindow().setTheme(pref === "auto" ? null : pref).catch(() => undefined);
}

/** Whether the page currently renders dark, resolving "auto" against the OS. */
export function isDark(pref: ThemePref): boolean {
  if (pref !== "auto") return pref === "dark";
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}
