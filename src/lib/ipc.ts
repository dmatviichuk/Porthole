import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  Contexts,
  ExecEvent,
  LogEvent,
  LogOptions,
  LogTarget,
  ResourceInfo,
  ResourceRow,
  ResourceType,
  WatchEvent,
} from "./types";
import type { NodeUsage, PodUsage } from "./utilization";

/** Stops a backend session. Safe to call before the start command has resolved. */
export type Stop = () => void;

function session(started: Promise<number>, onError: (message: string) => void): Stop {
  started.catch((err: unknown) => onError(String(err)));
  return () => {
    void started.then(
      (id) => invoke("stop_session", { id }),
      () => undefined,
    );
  };
}

const channel = <T,>(onMessage: (message: T) => void) => new Channel<T>(onMessage);

const ignore = () => undefined;

/** Strips the type down to the fields the Rust `ResourceType` accepts. */
const resourceArg = ({ group, version, kind, plural, namespaced }: ResourceType): ResourceType => ({
  group,
  version,
  kind,
  plural,
  namespaced,
});

export const listContexts = () => invoke<Contexts>("list_contexts");

export const reloadKubeconfig = () => invoke<Contexts>("reload_kubeconfig");

/** Streams left over from before a webview reload have nobody listening. */
export const stopAllSessions = () => invoke<void>("stop_all_sessions");

export const clusterVersion = (context: string) => invoke<string>("cluster_version", { context });

/** Per-node usage from metrics-server; rejects when the metrics API is not installed. */
export const nodeUsage = (context: string) => invoke<NodeUsage[]>("node_usage", { context });

/** Per-pod and per-container usage; `namespace` null covers the whole cluster. */
export const podUsage = (context: string, namespace: string | null) =>
  invoke<PodUsage[]>("pod_usage", { context, namespace });

export const listResources = (context: string, refresh = false) =>
  invoke<ResourceInfo[]>("list_resources", { context, refresh });

export type WatchOptions = {
  /** null watches every namespace. */
  namespace: string | null;
  fieldSelector: string | null;
  /** Include annotations; meant for single-object watches. */
  detail: boolean;
};

export function watchResources(
  context: string,
  resource: ResourceType,
  options: WatchOptions,
  onEvent: (event: WatchEvent<ResourceRow>) => void,
): Stop {
  const started = invoke<number>("watch_resources", {
    context,
    resource: resourceArg(resource),
    options,
    channel: channel(onEvent),
  });
  return session(started, (message) => onEvent({ type: "error", message }));
}

export const getYaml = (context: string, resource: ResourceType, namespace: string | null, name: string) =>
  invoke<string>("get_yaml", { context, resource: resourceArg(resource), namespace, name });

/** Saves edited YAML over the object it came from; the backend refuses anything else. */
export const saveYaml = (
  context: string,
  resource: ResourceType,
  namespace: string | null,
  name: string,
  yaml: string,
) => invoke<void>("save_yaml", { context, resource: resourceArg(resource), namespace, name, yaml });

export const deleteResource = (context: string, resource: ResourceType, namespace: string | null, name: string) =>
  invoke<void>("delete_resource", { context, resource: resourceArg(resource), namespace, name });

export function streamLogs(
  context: string,
  targets: LogTarget[],
  options: LogOptions,
  onEvent: (event: LogEvent) => void,
): Stop {
  const started = invoke<number>("stream_logs", { context, targets, options, channel: channel(onEvent) });
  return session(started, (message) => onEvent({ type: "error", source: 0, message }));
}

export type Shell = {
  stop: Stop;
  write: (data: string) => void;
  resize: (cols: number, rows: number) => void;
};

export function startShell(
  target: { context: string; namespace: string; pod: string; container: string },
  size: { cols: number; rows: number },
  onOutput: (bytes: Uint8Array) => void,
  onEvent: (event: ExecEvent) => void,
): Shell {
  const output = channel<ArrayBuffer>((buffer) => onOutput(new Uint8Array(buffer)));
  const started = invoke<number>("start_shell", { ...target, ...size, output, events: channel(onEvent) });
  // Input sent after the shell exits fails with "session is no longer running"; nothing to do then.
  return {
    stop: session(started, (message) => onEvent({ type: "error", message })),
    write: (data) => void started.then((id) => invoke("shell_input", { id, data }).catch(ignore), ignore),
    resize: (cols, rows) => void started.then((id) => invoke("shell_resize", { id, cols, rows }).catch(ignore), ignore),
  };
}
