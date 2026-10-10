# Development

```sh
cargo run -p porthole
cargo xtask check            # formatting, clippy and all unit tests
cargo clean                 # remove Cargo build output
```

Restart after editing Rust code. Optimized dependencies keep development builds responsive.

```text
apps/porthole/src/           Native application, shared view state and egui UI
crates/kubernetes/src/       Kubernetes discovery, watches, metrics, logs, exec and editing
assets/                     Original icons and resource type definitions
xtask/src/                  Rust build, icon and packaging commands
packaging/                  Native installer configuration
```

The UI receives typed messages from the async runtime. One cluster-wide watch per resource type
is shared by all views; cached snapshots render immediately, with live watches updating them.
No local HTTP server is started.

The integration test creates and deletes a namespace. Run it only against a disposable cluster:

```sh
PORTHOLE_E2E_CONTEXT=<disposable-context> cargo test -p porthole-kubernetes e2e -- --ignored --nocapture
```

The app imports existing viewer preferences from the old macOS installation on first launch.
It reads the original preference database without changing it.

Renovate groups crate and GitHub Actions updates and maintains the Cargo lockfile.
