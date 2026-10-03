# Building from source

## Prerequisites

- [Rust](https://rustup.rs) stable
- Node.js 24 or newer and [pnpm](https://pnpm.io) 12 (`corepack enable` sets it up from `package.json`)
- macOS: Xcode Command Line Tools. Windows: Microsoft C++ Build Tools and WebView2 (preinstalled on
  Windows 11). Linux: `libwebkit2gtk-4.1-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev`.
  See [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/) for details.

## Build

```sh
pnpm build:dev       # quick: seconds once dependencies are built, one bundle (Porthole.app on macOS)
pnpm build:release   # what releases ship: full link-time optimization and every bundle type
```

Both install dependencies when needed and render the app icons. `build:dev` writes to
`src-tauri/target/fast/bundle/`, `build:release` to `src-tauri/target/release/bundle/`
(`macos/Porthole.app` and `dmg/` on macOS, `msi/` and `nsis/` on Windows, `appimage/`, `deb/` and
`rpm/` on Linux). They use separate cargo profiles, so switching between them never rebuilds the
other's dependencies.

To run the app with hot reload while you work on it, see [Development](development.md).
