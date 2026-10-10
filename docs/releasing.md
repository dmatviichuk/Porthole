# Releasing

1. Update `workspace.package.version` in the root `Cargo.toml` and refresh `Cargo.lock` with `cargo check --workspace`.
2. Run `cargo xtask check` and compare the application in both themes against the previous release.
3. Build installers with `cargo xtask package --profile release`.
4. After review, push the matching `v<version>` tag, or run the Release workflow on `main`.

The workflow builds macOS Apple silicon and Intel, Windows and Linux installers.
One draft release collects all platforms and is published only after every build succeeds.
Turn off `publish` to build workflow artifacts without publishing a release.
