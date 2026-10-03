/** Joins class names, skipping the falsy ones: `cx("row", selected && "bg-selected")`. */
export function cx(...names: (string | false | null | undefined)[]): string {
  return names.filter(Boolean).join(" ");
}
