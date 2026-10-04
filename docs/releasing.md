# Releasing

1. Bump `version` in `package.json` (the app version comes from it) and in `src-tauri/Cargo.toml`,
   and push to `main`.
2. In the Actions tab, run **Release** on `main`. It tags the commit `v<version>`.

The [Release workflow](../.github/workflows/release.yml) builds macOS (Apple silicon and Intel),
Windows and Linux in parallel into a draft release and publishes it once every build succeeds.
Pushing a tag (`git tag v1.1.0 && git push origin v1.1.0`) works too.

## Caches

Prefer running it on `main`: GitHub only shares caches saved by branch runs, so those
runs leave the compiled dependencies and packaging tools for the next release,
while a tag run can restore them but not save. Running it with **publish** off builds every
platform without releasing (the bundles are kept as workflow artifacts), which is also a way to
warm the caches after a dependency update. CI on `main` keeps its own cache the same way;
pull requests reuse it without saving copies.
