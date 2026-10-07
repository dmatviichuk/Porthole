export const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** The command key: Cmd on macOS, Ctrl elsewhere. */
export const mod = (e: { metaKey: boolean; ctrlKey: boolean }) => (isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey);

/** Shown in hints: "⌘K" on macOS, "Ctrl+K" elsewhere. */
export const MOD = isMac ? "⌘" : "Ctrl+";
export const CTRL = isMac ? "⌃" : "Ctrl+";
export const SHIFT = isMac ? "⇧" : "Shift+";

/** True when keys go to text: inputs, the YAML editor, the terminal (xterm types into a textarea). */
export function isTyping(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || target.matches("input, textarea, select"));
}

/**
 * Moves focus to what the current view is about: its table, its page or its editor. Views mark
 * that element with `data-primary`; it also takes focus when the view opens.
 */
export function focusPrimary(): boolean {
  const target = document.querySelector<HTMLElement>("main [data-primary]");
  target?.focus({ preventScroll: true });
  return target !== null;
}

/** A ref that focuses the element when it mounts: the main area of a view or tab. */
export const focusOnMount = (element: HTMLElement | null) => {
  element?.focus({ preventScroll: true });
};

export type Shortcut = { keys: string[]; label: string };

/** Every shortcut, as the Keyboard Shortcuts dialog lists them. */
export const SHORTCUTS: { title: string; items: Shortcut[] }[] = [
  {
    title: "Anywhere",
    items: [
      { keys: [`${MOD}K`], label: "Command palette: views, resource types, namespaces, clusters, objects" },
      { keys: [`${MOD}1`, `${MOD}2`, `${MOD}3`], label: "Applications, All Resources, Overview" },
      { keys: [`${MOD}[`, `${MOD}]`], label: "Back, forward" },
      { keys: [`${CTRL}Tab`, `${CTRL}${SHIFT}Tab`], label: "Next, previous tab" },
      { keys: [`${MOD}F`, "/"], label: "Search this view (logs filter, YAML find)" },
      { keys: [`${CTRL}\``], label: "Move focus between the shells and the view" },
      { keys: ["?"], label: "This list" },
    ],
  },
  {
    title: "Tables",
    items: [
      { keys: ["↑", "↓"], label: "Move (also K, J)" },
      { keys: ["Home", "End"], label: `First, last row (also ${MOD}↑, ${MOD}↓)` },
      { keys: ["PgUp", "PgDn"], label: "Move a page" },
      { keys: ["↩"], label: "Open" },
      { keys: ["L"], label: "View logs" },
      { keys: ["S"], label: "Open shell" },
      { keys: ["Y"], label: "Edit YAML" },
      { keys: ["C"], label: "Copy name" },
      { keys: ["M"], label: "Actions menu, the same as a right click" },
    ],
  },
  {
    title: "Search fields",
    items: [
      { keys: ["↓", "↩"], label: "Go to the results" },
      { keys: ["Esc"], label: "Clear; again to go to the results" },
    ],
  },
  {
    title: "YAML",
    items: [{ keys: [`${MOD}S`], label: "Save changes" }],
  },
];
