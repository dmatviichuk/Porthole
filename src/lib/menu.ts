import { isTauri } from "@tauri-apps/api/core";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu } from "@tauri-apps/api/menu";

export type MenuEntry =
  | "separator"
  | { label: string; action: () => void; enabled?: boolean; checked?: boolean }
  | { label: string; items: MenuEntry[] };

type NativeItem = MenuItem | CheckMenuItem | PredefinedMenuItem | Submenu;

async function toNative(entry: MenuEntry): Promise<NativeItem> {
  if (entry === "separator") return PredefinedMenuItem.new({ item: "Separator" });
  if ("items" in entry) return Submenu.new({ text: entry.label, items: await Promise.all(entry.items.map(toNative)) });
  const options = { text: entry.label, enabled: entry.enabled ?? true, action: () => entry.action() };
  return entry.checked === undefined
    ? MenuItem.new(options)
    : CheckMenuItem.new({ ...options, checked: entry.checked });
}

/**
 * Shows a native context menu at the cursor, or below `anchor` when given.
 * Outside Tauri (a browser preview) there is no native menu, so nothing happens.
 */
export async function popupMenu(entries: MenuEntry[], anchor?: HTMLElement) {
  if (!isTauri()) return;
  const menu = await Menu.new({ items: await Promise.all(entries.map(toNative)) });
  if (anchor) {
    const rect = anchor.getBoundingClientRect();
    await menu.popup(new LogicalPosition(rect.left, rect.bottom + 4));
  } else {
    await menu.popup();
  }
}
