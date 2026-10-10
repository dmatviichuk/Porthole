# Building from source

Install Rust 1.99.0 through [rustup](https://rustup.rs). The repository and CI pin that exact toolchain.
macOS needs Xcode Command Line Tools. Windows needs Microsoft C++ Build Tools.
On Debian/Ubuntu install `build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libx11-dev libxcb1-dev libgl1-mesa-dev`.

```sh
cargo run -p porthole
cargo xtask bundle                         # fast profile; local native app bundle
cargo xtask bundle --profile dev           # development build
cargo xtask package --profile release      # release installers
```

Install the Rust packaging tool once for `package`:

```sh
cargo install cargo-packager --version 0.11.8 --locked
```

Linux packaging also needs `patchelf rpm`. Windows packaging downloads WiX and NSIS.
Packaged Windows builds link the C runtime statically, so installing the application
does not require a separate Visual C++ redistributable.
Bundles and installers are written under `target/<profile>/bundle/`, or
`target/<target-triple>/<profile>/bundle/` with `--target`.
Windows packaging always uses the target-specific directory, defaulting to the host's MSVC target.
The macOS development bundle is `Porthole.app`; release installers are DMG on macOS,
MSI and NSIS on Windows, and AppImage, DEB and RPM on Linux.

`cargo xtask icons` renders the original SVG into PNG and ICO using Rust.
The original macOS ICNS is kept in `assets/`. Icons and UI code are compiled into the executable.
The app loads the same installed system fonts as the original interface.
