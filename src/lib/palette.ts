export type PaletteItem = {
  id: string;
  /** Items show under a heading per group, groups in the order their first item comes. */
  group: string;
  label: string;
  /** Dimmed after the label; also searched. */
  detail?: string;
  /** More words the item is found by. */
  keywords?: string;
  /** A shortcut shown on the right. */
  hint?: string;
  run: () => void;
};

/**
 * How well `item` matches: every word must appear in its label, detail or keywords. An exact
 * label beats a label starting with the first word, which beats a word of the label starting
 * with it, which beats a match anywhere. null when some word is missing.
 */
export function score(item: PaletteItem, terms: string[]): number | null {
  const label = item.label.toLowerCase();
  const text = `${label} ${item.detail ?? ""} ${item.keywords ?? ""}`.toLowerCase();
  if (!terms.every((term) => text.includes(term))) return null;
  const first = terms[0] ?? "";
  if (label === terms.join(" ")) return 3;
  if (label.startsWith(first)) return 2;
  if (label.split(/[\s\-_./:]+/).some((word) => word.startsWith(first))) return 1;
  return 0;
}

/** The items matching `query`, best first within each group, at most `limit(group)` per group. */
export function rank(items: PaletteItem[], query: string, limit: (group: string) => number): PaletteItem[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  const groups = new Map<string, { item: PaletteItem; score: number; order: number }[]>();
  items.forEach((item, order) => {
    const value = terms.length === 0 ? 0 : score(item, terms);
    if (value === null) return;
    const list = groups.get(item.group) ?? [];
    list.push({ item, score: value, order });
    groups.set(item.group, list);
  });
  return [...groups].flatMap(([group, list]) =>
    list
      .toSorted((a, b) => b.score - a.score || a.order - b.order)
      .slice(0, limit(group))
      .map((entry) => entry.item),
  );
}
