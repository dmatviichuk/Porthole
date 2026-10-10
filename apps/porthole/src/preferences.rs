//! One-time, read-only import of Porthole 1.1.0's viewer preferences.
use crate::model::Prefs;
#[cfg(any(target_os = "macos", test))]
use crate::model::Theme;
#[cfg(any(target_os = "macos", test))]
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::path::Path;
pub fn legacy() -> Option<Prefs> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        let root = std::path::PathBuf::from(home).join("Library/WebKit/dev.dmatviichuk.porthole/WebsiteData");
        let mut values = BTreeMap::new();
        read_tree(&root, 0, &mut values);
        if values.is_empty() {
            return None;
        }
        Some(decode(&values))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
pub fn import_window(cc: &eframe::CreationContext<'_>) {
    #[cfg(target_os = "macos")]
    if cc.storage.and_then(|s| s.get_string("window")).is_some() {
        return;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = cc;
    #[cfg(target_os = "macos")]
    {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = std::path::PathBuf::from(home)
            .join("Library/Application Support/dev.dmatviichuk.porthole/.window-state.json");
        let Some(state) = std::fs::read(path)
            .ok()
            .and_then(|s| serde_json::from_slice::<serde_json::Value>(&s).ok())
        else {
            return;
        };
        let Some(window) = state.get("main") else {
            return;
        };
        let scale = cc.egui_ctx.native_pixels_per_point().unwrap_or(1.0);
        if let (Some(width), Some(height)) = (window["width"].as_f64(), window["height"].as_f64()) {
            cc.egui_ctx
                .send_viewport_cmd(eframe::egui::ViewportCommand::InnerSize(eframe::egui::vec2(
                    width as f32 / scale,
                    height as f32 / scale,
                )));
        }
        if let (Some(x), Some(y)) = (window["x"].as_f64(), window["y"].as_f64()) {
            cc.egui_ctx
                .send_viewport_cmd(eframe::egui::ViewportCommand::OuterPosition(eframe::egui::pos2(
                    x as f32 / scale,
                    y as f32 / scale,
                )));
        }
        cc.egui_ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Maximized(
            window["maximized"].as_bool().unwrap_or(false),
        ));
        cc.egui_ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Fullscreen(
            window["fullscreen"].as_bool().unwrap_or(false),
        ));
    }
}
#[cfg(target_os = "macos")]
fn read_tree(root: &Path, depth: usize, values: &mut BTreeMap<String, String>) {
    if depth > 8 {
        return;
    }
    let Ok(files) = std::fs::read_dir(root) else {
        return;
    };
    for entry in files.flatten() {
        let p = entry.path();
        if p.is_dir() {
            read_tree(&p, depth + 1, values);
        } else if p.file_name().is_some_and(|n| n == "localstorage.sqlite3") {
            let Ok(db) = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
                continue;
            };
            let Ok(mut query) = db.prepare("SELECT key,value FROM ItemTable WHERE key LIKE 'porthole.%'") else {
                continue;
            };
            let Ok(rows) = query.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))) else {
                continue;
            };
            for (key, bytes) in rows.flatten() {
                let utf16: Vec<_> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| u16::from_le_bytes([p[0], p[1]]))
                    .collect();
                if let Ok(value) = String::from_utf16(&utf16) {
                    values.insert(key, value);
                }
            }
        }
    }
}
#[cfg(any(target_os = "macos", test))]
fn decode(values: &BTreeMap<String, String>) -> Prefs {
    let mut prefs = Prefs::default();
    for (key, value) in values {
        match key.as_str() {
            "porthole.context" => prefs.context = Some(value.clone()),
            "porthole.theme" => {
                prefs.theme = match value.as_str() {
                    "light" => Theme::Light,
                    "dark" => Theme::Dark,
                    _ => Theme::Auto,
                }
            }
            "porthole.sidebarWidth" => {
                if let Ok(v) = value.parse::<f32>() {
                    prefs.sidebar_width = v.clamp(200.0, 560.0)
                }
            }
            "porthole.dockHeight" => {
                if let Ok(v) = value.parse() {
                    prefs.dock_height = v;
                }
            }
            "porthole.clusterLabels" => prefs.labels = serde_json::from_str(value).unwrap_or_default(),
            "porthole.pref.logs.timestamps" => prefs.timestamps = value == "true",
            "porthole.pref.logs.wrap" => prefs.wrap_logs = value == "true",
            "porthole.pref.logs.format" => prefs.format_logs = value == "true",
            _ => {
                if let Some(context) = key.strip_prefix("porthole.namespace.") {
                    prefs
                        .namespaces
                        .insert(context.into(), (!value.is_empty()).then(|| value.clone()));
                }
                if let Some(table) = key.strip_prefix("porthole.columns.")
                    && let Ok(order) = serde_json::from_str(value)
                {
                    prefs.columns.insert(table.into(), order);
                }
            }
        }
    }
    prefs
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_preferences() {
        let values = BTreeMap::from([
            ("porthole.theme".into(), "dark".into()),
            ("porthole.sidebarWidth".into(), "300".into()),
            ("porthole.context".into(), "one".into()),
            ("porthole.namespace.one".into(), "shop".into()),
            ("porthole.pref.logs.wrap".into(), "false".into()),
            (
                "porthole.clusterLabels".into(),
                r#"{"one":{"tag":"Production","color":"red","confirmDeletes":true}}"#.into(),
            ),
        ]);
        let p = decode(&values);
        assert_eq!(p.theme, Theme::Dark);
        assert_eq!(p.sidebar_width, 300.0);
        assert_eq!(p.namespaces["one"].as_deref(), Some("shop"));
        assert!(!p.wrap_logs);
        assert!(p.labels["one"].confirm_deletes);
    }
}
