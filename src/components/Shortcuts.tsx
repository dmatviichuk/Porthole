import { useEffect } from "react";

import { openFind } from "../lib/find";
import { focusPrimary, isTyping, mod } from "../lib/keys";
import { useApp, type View } from "../store";
import { stepTab } from "./TopBar";

/** Cmd/Ctrl+1, 2, 3. */
const VIEWS: Record<string, View> = {
  "1": { name: "applications" },
  "2": { name: "resources", resource: null },
  "3": { name: "overview" },
};

const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"]);

/** Opens a view, or focuses it when it is already open. */
export function go(view: View) {
  const { history, index, navigate } = useApp.getState();
  if (history[index]?.name === view.name) focusPrimary();
  else navigate(view);
}

/** Shows a shell tab and puts the keyboard in its terminal. */
export function focusShell(key: string) {
  useApp.getState().setActiveShell(key);
  // xterm reads keys from a hidden textarea; the tab may only become visible on the next frame.
  requestAnimationFrame(() =>
    document.querySelector<HTMLElement>(`[data-shell="${CSS.escape(key)}"] textarea`)?.focus(),
  );
}

/** Ctrl+`: from the view to the open shell, and back. */
function toggleShellFocus() {
  if (document.querySelector("[data-dock]")?.contains(document.activeElement)) {
    focusPrimary();
    return;
  }
  const { activeShell } = useApp.getState();
  if (activeShell) focusShell(activeShell);
}

/**
 * The app-wide keys. Anything a focused control handles first (a table row key, the YAML editor's
 * own Cmd+F or Cmd+[, a key the terminal takes) is left to it, and an open dialog takes every key.
 */
export function Shortcuts() {
  useEffect(() => {
    const root = document.documentElement;
    // Keyboard-only styling, such as a table's row cursor, follows the last kind of input.
    const onPointer = () => {
      root.dataset.input = "pointer";
    };
    const onAnyKey = (e: KeyboardEvent) => {
      if (!MODIFIERS.has(e.key)) root.dataset.input = "keyboard";
    };

    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || document.querySelector("dialog[open]")) return;
      const app = useApp.getState();
      const ctrlOnly = e.ctrlKey && !e.metaKey && !e.altKey;
      if (ctrlOnly && e.code === "Backquote") {
        e.preventDefault();
        toggleShellFocus();
      } else if (ctrlOnly && e.key === "Tab") {
        e.preventDefault();
        stepTab(e.shiftKey ? -1 : 1);
      } else if (mod(e) && !e.altKey && !e.shiftKey) {
        const key = e.key.toLowerCase();
        const view = VIEWS[key];
        let handled = true;
        if (key === "k") app.setOverlay("palette");
        else if (key === "f") handled = openFind();
        else if (key === "[") app.back();
        else if (key === "]") app.forward();
        else if (view) go(view);
        else handled = false;
        if (handled) e.preventDefault();
      } else if (!e.ctrlKey && !e.metaKey && !e.altKey && !isTyping(e.target)) {
        if (e.key === "/" && openFind()) e.preventDefault();
        else if (e.key === "?") {
          e.preventDefault();
          app.setOverlay("shortcuts");
        }
      }
    };

    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onAnyKey, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onAnyKey, true);
      window.removeEventListener("keydown", onKey);
    };
  }, []);
  return null;
}
