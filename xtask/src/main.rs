//! Development and packaging commands, using Cargo throughout.
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
fn command(root: &Path, program: &str, args: &[String]) -> Result<(), String> {
    let mut child = Command::new(program);
    child.current_dir(root).args(args);
    finish_command(program, &mut child)
}
fn finish_command(program: &str, child: &mut Command) -> Result<(), String> {
    let status = child.status().map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}
fn run() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("workspace root missing")?;
    let args: Vec<_> = env::args().skip(1).collect();
    let Some(task) = args.first() else {
        println!(
            "cargo xtask <check|icons|bundle|package|version> [--profile dev|fast|release] [--target TARGET] [--verification] [--no-build]"
        );
        return Ok(());
    };
    if task == "version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if task == "check" {
        for step in [
            vec!["fmt", "--all", "--check"],
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
            vec!["test", "--workspace", "--locked"],
        ] {
            command(root, "cargo", &step.into_iter().map(String::from).collect::<Vec<_>>())?;
        }
        return Ok(());
    }
    if !matches!(task.as_str(), "icons" | "bundle" | "package") {
        return Err(format!("unknown task {task}"));
    }
    icons(root)?;
    if task == "icons" {
        return Ok(());
    }
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let profile = value("--profile").unwrap_or_else(|| "fast".into());
    let target = value("--target").or_else(|| {
        // An explicit target keeps target-only static CRT flags away from the
        // host build scripts and procedural macros on Windows.
        if cfg!(target_env = "msvc") {
            let arch = if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else {
                "x86_64"
            };
            Some(format!("{arch}-pc-windows-msvc"))
        } else {
            None
        }
    });
    let verification = args.iter().any(|a| a == "--verification");
    if !args.iter().any(|a| a == "--no-build") {
        let mut build = vec![
            "build".into(),
            "--locked".into(),
            "-p".into(),
            "porthole".into(),
            "--profile".into(),
            profile.clone(),
        ];
        if let Some(t) = &target {
            build.extend(["--target".into(), t.clone()]);
        }
        let mut child = Command::new("cargo");
        child.current_dir(root).args(&build);
        if target.as_deref().is_some_and(|t| t.ends_with("-windows-msvc")) {
            // Ship the CRT in the executable; neither installer requires a
            // separately installed Visual C++ redistributable.
            if let Some(mut flags) = env::var_os("CARGO_ENCODED_RUSTFLAGS") {
                if !flags.is_empty() {
                    flags.push("\u{1f}");
                }
                flags.push("-C\u{1f}target-feature=+crt-static");
                child.env("CARGO_ENCODED_RUSTFLAGS", flags);
            } else {
                let mut flags = env::var_os("RUSTFLAGS").unwrap_or_default();
                flags.push(" -C target-feature=+crt-static");
                child.env("RUSTFLAGS", flags);
            }
        }
        finish_command("cargo", &mut child)?;
    }
    let mut binaries = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    if let Some(t) = &target {
        binaries.push(t);
    }
    binaries.push(if profile == "dev" || profile == "debug" {
        "debug"
    } else {
        &profile
    });
    let out = binaries.join("bundle");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    if cfg!(target_os = "macos") && task == "bundle" {
        return mac_bundle(root, &binaries, &out, verification);
    }
    if verification {
        return Err("--verification is only for the local macOS comparison bundle".into());
    }
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("packaging/packager.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    config["version"] = env!("CARGO_PKG_VERSION").into();
    config["outDir"] = out.to_string_lossy().into_owned().into();
    config["binariesDir"] = binaries.to_string_lossy().into_owned().into();
    config["licenseFile"] = root.join("LICENSE").to_string_lossy().into_owned().into();
    config["icons"] = serde_json::json!(
        ["porthole.icns", "porthole.ico", "porthole.png"].map(|name| root
            .join("assets")
            .join(name)
            .to_string_lossy()
            .into_owned())
    );
    if let Some(t) = &target {
        config["targetTriple"] = t.clone().into();
    }
    config["formats"] = serde_json::json!(if cfg!(target_os = "macos") {
        vec!["app", "dmg"]
    } else if cfg!(windows) {
        vec!["wix", "nsis"]
    } else {
        vec!["deb", "appimage"]
    });
    let path = out.join("packager.json");
    fs::write(&path, serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    command(
        root,
        "cargo",
        &[
            "packager".into(),
            "--config".into(),
            serde_json::to_string(&config).map_err(|e| e.to_string())?,
        ],
    )?;
    #[cfg(target_os = "linux")]
    rpm(root, &binaries, &out)?;
    Ok(())
}
fn icons(root: &Path) -> Result<(), String> {
    let png = root.join("assets/porthole.png");
    let ico = root.join("assets/porthole.ico");
    if png.exists() && ico.exists() {
        return Ok(());
    }
    let svg = fs::read(root.join("assets/app-icon.svg")).map_err(|e| e.to_string())?;
    let raster = egui_extras::image::load_svg_bytes(&svg, &Default::default())?;
    let bytes: Vec<u8> = raster.pixels.iter().flat_map(|p| p.to_array()).collect();
    let image =
        image::RgbaImage::from_raw(raster.size[0] as u32, raster.size[1] as u32, bytes).ok_or("invalid app icon")?;
    image.save(png).map_err(|e| e.to_string())?;
    image::imageops::resize(&image, 256, 256, image::imageops::FilterType::Lanczos3)
        .save(ico)
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn mac_bundle(root: &Path, binaries: &Path, out: &Path, verification: bool) -> Result<(), String> {
    let name = if verification { "Porthole Native" } else { "Porthole" };
    let id = if verification {
        "dev.dmatviichuk.porthole.native"
    } else {
        "dev.dmatviichuk.porthole"
    };
    let app = out.join(format!("{name}.app"));
    let contents = app.join("Contents");
    fs::create_dir_all(contents.join("MacOS")).map_err(|e| e.to_string())?;
    fs::create_dir_all(contents.join("Resources")).map_err(|e| e.to_string())?;
    fs::copy(binaries.join("porthole"), contents.join("MacOS/porthole")).map_err(|e| e.to_string())?;
    fs::copy(
        root.join("assets/porthole.icns"),
        contents.join("Resources/porthole.icns"),
    )
    .map_err(|e| e.to_string())?;
    let plist = include_str!("../../packaging/macos/Info.plist.in")
        .replace("@NAME@", name)
        .replace("@IDENTIFIER@", id)
        .replace("@VERSION@", env!("CARGO_PKG_VERSION"));
    fs::write(contents.join("Info.plist"), plist).map_err(|e| e.to_string())?;
    command(
        root,
        "codesign",
        &[
            "--force".into(),
            "--deep".into(),
            "--sign".into(),
            "-".into(),
            app.to_string_lossy().into_owned(),
        ],
    )?;
    println!("{}", app.display());
    Ok(())
}
#[cfg(target_os = "linux")]
fn rpm(root: &Path, binaries: &Path, out: &Path) -> Result<(), String> {
    let work = out.join("rpm-build");
    let stage = work.join("SOURCES/porthole");
    for dir in [
        "usr/bin",
        "usr/share/applications",
        "usr/share/icons/hicolor/1024x1024/apps",
        "usr/share/licenses/porthole",
    ] {
        fs::create_dir_all(stage.join(dir)).map_err(|e| e.to_string())?;
    }
    for (from, to) in [
        (binaries.join("porthole"), "usr/bin/porthole"),
        (
            root.join("assets/porthole.png"),
            "usr/share/icons/hicolor/1024x1024/apps/porthole.png",
        ),
        (root.join("LICENSE"), "usr/share/licenses/porthole/LICENSE"),
    ] {
        fs::copy(from, stage.join(to)).map_err(|e| e.to_string())?;
    }
    fs::write(stage.join("usr/share/applications/porthole.desktop"), "[Desktop Entry]\nType=Application\nName=Porthole\nExec=porthole\nIcon=porthole\nCategories=Development;\nTerminal=false\n").map_err(|e| e.to_string())?;
    let spec = format!(
        "Name: porthole\nVersion: {}\nRelease: 1\nSummary: Desktop client for Kubernetes\nLicense: Apache-2.0\nAutoReqProv: no\nRequires: libX11, libxcb, libxkbcommon, fontconfig, mesa-libGL\n%description\nDesktop client for Kubernetes.\n%install\nmkdir -p \"%{{buildroot}}\"\ncp -a '{}'/. \"%{{buildroot}}/\"\n%files\n/usr/bin/porthole\n/usr/share/applications/porthole.desktop\n/usr/share/icons/hicolor/1024x1024/apps/porthole.png\n%license /usr/share/licenses/porthole/LICENSE\n",
        env!("CARGO_PKG_VERSION"),
        stage.to_string_lossy().replace('\'', "'\\''")
    );
    fs::create_dir_all(work.join("SPECS")).map_err(|e| e.to_string())?;
    let spec_path = work.join("SPECS/porthole.spec");
    fs::write(&spec_path, spec).map_err(|e| e.to_string())?;
    command(
        root,
        "rpmbuild",
        &[
            "-bb".into(),
            "--define".into(),
            format!("_topdir {}", work.display()),
            "--define".into(),
            format!("_rpmdir {}", out.display()),
            "--buildroot".into(),
            work.join("BUILDROOT/porthole").to_string_lossy().into_owned(),
            spec_path.to_string_lossy().into_owned(),
        ],
    )
}
