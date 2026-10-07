# Contributing

Thanks for helping with Porthole. Bug reports, ideas and pull requests are all welcome. Everyone
taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).

Found a security problem? Don't open an issue; follow the [Security policy](SECURITY.md).

## Reporting a bug

Open a [bug report](https://github.com/dmatviichuk/Porthole/issues/new?template=bug_report.yml)
and tell us the Porthole version, your operating system and the cluster: its Kubernetes version,
where it runs (EKS, GKE, AKS, k3s, kind...) and how your kubeconfig signs in (an exec plugin such
as `aws eks get-token` or `kubelogin`, a token, a client certificate).

The log often explains what went wrong. On macOS and Linux, start Porthole from a terminal and copy
what it prints. `RUST_LOG` adds detail:

```sh
RUST_LOG=porthole_lib=debug,kube=info /Applications/Porthole.app/Contents/MacOS/porthole  # macOS
RUST_LOG=porthole_lib=debug,kube=info porthole  # Linux; for the AppImage, run its file instead
```

Remove cluster names, hostnames, tokens and anything else private before you paste a log, YAML or
a screenshot.

## Suggesting a feature

Open a [feature request](https://github.com/dmatviichuk/Porthole/issues/new?template=feature_request.yml)
that describes the problem you want solved, not only the solution. For anything bigger than a small
fix, open an issue before writing code so we can agree on the approach first.

Two decisions are part of what Porthole is, and changes against them will be declined:

- **It never creates resources.** No create view, no apply. Saving YAML replaces the object it
  opened and must keep its kind, name, namespace and `resourceVersion`.
- **It never waits on the cluster to switch views.** Views read from one cluster-wide watch per
  resource type, shared and saved locally, so switching views or namespaces shows no loader. New
  views use those watches instead of polling or fetching on open.

## Making a change

1. Fork the repository and branch from `main`.
2. Set up the prerequisites from [Building from source](../docs/building.md), then run the app with
   hot reload as described in [Development](../docs/development.md).
3. Keep each pull request to one change, and add or update tests with it: UI tests are vitest files
   next to the code (`src/lib/apps.test.ts`), backend tests live in `src-tauri/src`.
4. If you touched watches, logs, shells, YAML save or delete, also run the end-to-end test from
   [Development](../docs/development.md) against a disposable cluster (k3s or kind in Docker), never
   one you care about.
5. Run `pnpm check`. It runs the checks CI runs: typecheck, oxlint, vitest, `cargo fmt --check`, clippy
   with warnings denied and `cargo test`.
6. Open the pull request against `main`. Say what changed and how you checked it; for UI changes,
   add screenshots in both the light and the dark theme.

Leave the version alone; releases are cut separately (see [Releasing](../docs/releasing.md)).

## Code

- Match the code around you. Rust is formatted with `cargo fmt`; comments explain why, not what.
- The UI talks to the backend only through Tauri commands and channels. Nothing listens on a local
  port.
- Every dependency stays on its latest release, and [Renovate](../docs/development.md#dependency-updates)
  keeps it there, so don't bump versions in a feature pull request. Prefer a few lines of our own
  over a new package; if you do add one, say why in the pull request.

## Commit messages

The subject says what changes, in plain words: sentence case, no trailing period, an area prefix
only when it helps. For example:

```
Remember the window size and position
Cmd+F opens the search on screen; logs mark what the filter matched
CI: install Linux packages with plain apt
```

Use the body to explain why, when the subject doesn't make it obvious.

## License

Porthole is licensed under the [Apache License, Version 2.0](../LICENSE). Contributions you submit
are licensed under the same terms, as section 5 of the license describes.
