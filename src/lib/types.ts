// Mirrors the serde types in src-tauri/src. Rust `Option` arrives as `null`.

export type ContextInfo = {
  name: string;
  cluster: string | null;
  namespace: string | null;
};

export type Contexts = {
  current: string | null;
  contexts: ContextInfo[];
};

/** One API resource type, e.g. apps/v1 deployments. */
export type ResourceType = {
  group: string;
  version: string;
  kind: string;
  plural: string;
  namespaced: boolean;
};

export type ResourceInfo = ResourceType & { verbs: string[] };

export type Health = "ok" | "progress" | "failed" | "done" | "ending";

export type ResourceRow = {
  uid: string;
  name: string;
  namespace: string | null;
  /** Unix seconds. */
  created: number | null;
  owner: { kind: string; name: string } | null;
  labels: Record<string, string>;
  /** Only on detail watches (a single object). */
  annotations?: Record<string, string>;
  status: string | null;
  health: Health | null;
  /** Kind-specific values; see columnsFor() in kinds.ts. */
  fields: Record<string, unknown>;
};

export type ContainerRow = {
  name: string;
  image: string;
  init: boolean;
  ready: boolean;
  restarts: number;
  state: string;
  /** Millicores and bytes; zero when not set. */
  requests: { cpu: number; memory: number };
  limits: { cpu: number; memory: number };
};

export type WatchEvent<R> =
  | { type: "reset"; items: R[] }
  | { type: "changes"; upserts: R[]; deletes: string[] }
  | { type: "error"; message: string };

export type LogTarget = {
  namespace: string;
  pod: string;
  container: string;
};

export type LogOptions = {
  tailLines: number | null;
  previous: boolean;
  follow: boolean;
};

export type LogLine = {
  source: number;
  ts: string | null;
  text: string;
};

export type LogEvent =
  | { type: "lines"; lines: LogLine[] }
  | { type: "ended"; source: number }
  | { type: "error"; source: number; message: string };

export type ExecEvent =
  | { type: "connected" }
  | { type: "exited"; code: number | null; message: string | null }
  | { type: "error"; message: string };
