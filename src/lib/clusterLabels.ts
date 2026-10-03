/** Colours a cluster label can take; readable on both themes. */
export const LABEL_COLORS = {
  gray: "#8b8d98",
  red: "#e5484d",
  orange: "#f76b15",
  amber: "#d9a01b",
  green: "#30a46c",
  teal: "#12a594",
  blue: "#3d8bff",
  violet: "#8e4ec6",
} as const;

export type LabelColor = keyof typeof LABEL_COLORS;

/** What the user says a cluster is: shown next to its name, nothing is guessed from the name. */
export type ClusterLabel = {
  tag: string;
  color: LabelColor;
  /** Every delete on this cluster asks for the name to be typed back. */
  confirmDeletes: boolean;
};

const KEY = "porthole.clusterLabels";

const isLabel = (value: unknown): value is ClusterLabel => {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v.tag === "string" && typeof v.color === "string" && v.color in LABEL_COLORS && typeof v.confirmDeletes === "boolean";
};

/** Labels by kubeconfig context name; anything unreadable is dropped rather than breaking the app. */
export function loadLabels(): Record<string, ClusterLabel> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    if (typeof parsed !== "object" || parsed === null) return {};
    return Object.fromEntries(Object.entries(parsed).filter((entry): entry is [string, ClusterLabel] => isLabel(entry[1])));
  } catch {
    return {};
  }
}

export function saveLabels(labels: Record<string, ClusterLabel>) {
  try {
    localStorage.setItem(KEY, JSON.stringify(labels));
  } catch {
    // Storage blocked: labels last for this session.
  }
}
