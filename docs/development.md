# Development

Set up the prerequisites from [Building from source](building.md) first.

```sh
pnpm app        # run the app with hot reload (dependencies are optimized, so it runs at full speed)
pnpm check      # typecheck, lint and unit tests for the UI and the Rust backend
pnpm clean      # remove every build and cache: Rust target/, dist/, generated icons, Vite caches
```

The Rust backend also has an end-to-end test that sets up a namespace, watches pods, follows logs,
opens a shell, edits YAML and deletes everything again. Point it at a disposable cluster only:

```sh
PORTHOLE_E2E_CONTEXT=<context> cargo test --manifest-path src-tauri/Cargo.toml e2e -- --ignored --nocapture
```

## Layout

```
src-tauri/src/      Rust backend
  clusters.rs         kubeconfig contexts, one client and discovery cache per context
  watch.rs            watch any resource type, batch changes, stream them to the UI
  summary.rs, pods.rs table rows per kind (kubectl's status logic, requests and limits)
  resources.rs        discovery, YAML get and edit-only save, delete
  logs.rs, exec.rs    log streams and interactive shells
  metrics.rs          node and pod usage from metrics.k8s.io
  shell_env.rs        login-shell environment for apps started outside a terminal
src/                React UI
  lib/watchStore.ts   one cluster-wide watch per (context, type), filtered per view, saved locally
  lib/warm.ts         what starts when a context is picked
  lib/apps.ts         groups pods under their workloads
  views/              Applications, All resources, resource page, Overview
```

The UI talks to the backend over Tauri's IPC channels only; nothing listens on a local port.
