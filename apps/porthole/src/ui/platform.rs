//! Native macOS menus use the same menu implementation as the 1.1.0 app.
use super::{Action, MenuItem};
#[cfg(target_os = "macos")]
pub struct Platform {
    view: *mut std::ffi::c_void,
    _menu: muda::Menu,
    events: std::sync::mpsc::Receiver<(muda::MenuEvent, Option<String>)>,
    edits: std::collections::HashMap<muda::MenuId, Edit>,
}
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum Edit {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}
#[cfg(not(target_os = "macos"))]
pub struct Platform;
impl Platform {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(target_os = "macos")]
        {
            use muda::{Menu, PredefinedMenuItem as P, Submenu};
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let view = match cc.window_handle().map(|h| h.as_raw()) {
                Ok(RawWindowHandle::AppKit(h)) => h.ns_view.as_ptr(),
                _ => std::ptr::null_mut(),
            }; // OpenGL colors are sRGB, as were the original WebKit CSS colors. Tag the
            // window explicitly so wide-gamut displays do not reinterpret them as Display P3.
            if !view.is_null() {
                unsafe {
                    let ns_view = &*view.cast::<objc2_app_kit::NSView>();
                    if let Some(window) = ns_view.window() {
                        window.setColorSpace(Some(&objc2_app_kit::NSColorSpace::sRGBColorSpace()));
                    }
                }
            }
            let menu = Menu::new();
            let app = Submenu::new("Porthole", true);
            let _ = app.append_items(&[
                &P::about(
                    Some("About Porthole"),
                    Some(muda::AboutMetadata {
                        name: Some("Porthole".into()),
                        version: Some(env!("CARGO_PKG_VERSION").into()),
                        copyright: Some("Copyright © 2026 Dmytro Matviichuk".into()),
                        ..Default::default()
                    }),
                ),
                &P::separator(),
                &P::services(None),
                &P::separator(),
                &P::hide(None),
                &P::hide_others(None),
                &P::show_all(None),
                &P::separator(),
                &P::quit(None),
            ]);
            let file = Submenu::new("File", true);
            let _ = file.append(&P::close_window(None));
            let edit = Submenu::new("Edit", true);
            let mut edits = std::collections::HashMap::new();
            use muda::accelerator::{Accelerator, Code, Modifiers};
            for (name, code, shift, action) in [
                ("Undo", Code::KeyZ, false, Edit::Undo),
                ("Redo", Code::KeyZ, true, Edit::Redo),
                ("Cut", Code::KeyX, false, Edit::Cut),
                ("Copy", Code::KeyC, false, Edit::Copy),
                ("Paste", Code::KeyV, false, Edit::Paste),
                ("Select All", Code::KeyA, false, Edit::SelectAll),
            ] {
                if name == "Cut" {
                    let _ = edit.append(&P::separator());
                }
                let modifiers = Modifiers::META | if shift { Modifiers::SHIFT } else { Modifiers::empty() };
                let item = muda::MenuItem::new(name, true, Some(Accelerator::new(modifiers, code)));
                edits.insert(item.id().clone(), action);
                let _ = edit.append(&item);
            }
            let view_menu = Submenu::new("View", true);
            let _ = view_menu.append(&P::fullscreen(None));
            let window = Submenu::new("Window", true);
            let _ = window.append_items(&[
                &P::minimize(None),
                &P::maximize(None),
                &P::separator(),
                &P::close_window(None),
            ]);
            let help = Submenu::new("Help", true);
            let _ = menu.append_items(&[&app, &file, &edit, &view_menu, &window, &help]);
            menu.init_for_nsapp();
            let (tx, events) = std::sync::mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            let edit_ids = edits.clone();
            muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
                let paste = if matches!(edit_ids.get(&event.id), Some(Edit::Paste)) {
                    arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok()
                } else {
                    None
                };
                let _ = tx.send((event, paste));
                ctx.request_repaint();
            }));
            Self {
                view,
                _menu: menu,
                events,
                edits,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = cc;
            Self
        }
    }
    pub fn process_edits(&self, ctx: &eframe::egui::Context) {
        #[cfg(target_os = "macos")]
        {
            use eframe::egui::{Event, Key, Modifiers};
            for (event, paste) in self.events.try_iter() {
                let Some(edit) = self.edits.get(&event.id) else {
                    continue;
                };
                let event = match edit {
                    Edit::Cut => Event::Cut,
                    Edit::Copy => Event::Copy,
                    Edit::Paste => {
                        let Some(text) = paste else {
                            continue;
                        };
                        Event::Paste(text)
                    }
                    Edit::Undo | Edit::Redo | Edit::SelectAll => Event::Key {
                        key: if matches!(edit, Edit::SelectAll) {
                            Key::A
                        } else {
                            Key::Z
                        },
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers {
                            command: true,
                            mac_cmd: true,
                            shift: matches!(edit, Edit::Redo),
                            ..Modifiers::NONE
                        },
                    },
                };
                ctx.input_mut(|i| i.events.push(event));
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = ctx;
    }
    pub fn popup(&self, ctx: &eframe::egui::Context, items: &[MenuItem], pos: eframe::egui::Pos2) -> Option<Action> {
        #[cfg(target_os = "macos")]
        {
            use muda::ContextMenu;
            if self.view.is_null() {
                return None;
            }
            let menu = muda::Menu::new();
            let mut actions = std::collections::HashMap::new();
            for item in items {
                let native = convert(item, &mut actions);
                let _ = menu.append(native.as_ref());
            }
            while self.events.try_recv().is_ok() {}
            unsafe {
                let view = &*self.view.cast::<objc2_app_kit::NSView>();
                // egui and winit's flipped NSView both use a top-left origin.
                // muda converts its argument into an unflipped AppKit point.
                // Compensate for that conversion when the supplied view is flipped.
                let x = f64::from(pos.x * ctx.zoom_factor());
                let y = f64::from((pos.y + 4.0) * ctx.zoom_factor());
                let y = if view.isFlipped() {
                    view.frame().size.height - y
                } else {
                    y
                };
                menu.show_context_menu_for_nsview(self.view, Some(muda::dpi::LogicalPosition::new(x, y).into()));
            }
            let selected = self.events.try_recv().ok();
            selected.and_then(|(e, _)| actions.remove(&e.id))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (ctx, items, pos);
            None
        }
    }
}
#[cfg(target_os = "macos")]
fn convert(
    item: &MenuItem,
    actions: &mut std::collections::HashMap<muda::MenuId, Action>,
) -> Box<dyn muda::IsMenuItem> {
    if item.label.is_empty() {
        return Box::new(muda::PredefinedMenuItem::separator());
    }
    if !item.children.is_empty() {
        let menu = muda::Submenu::new(&item.label, item.enabled);
        for child in &item.children {
            let native = convert(child, actions);
            let _ = menu.append(native.as_ref());
        }
        return Box::new(menu);
    }
    let shortcut = match item.label.as_str() {
        "View logs" => Some(muda::accelerator::Code::KeyL),
        "Open shell" => Some(muda::accelerator::Code::KeyS),
        "Edit YAML" => Some(muda::accelerator::Code::KeyY),
        "Copy name" => Some(muda::accelerator::Code::KeyC),
        _ => None,
    }
    .map(|code| muda::accelerator::Accelerator::new(muda::accelerator::Modifiers::empty(), code));
    if let Some(checked) = item.checked {
        let native = muda::CheckMenuItem::new(&item.label, item.enabled, checked, shortcut);
        if let Some(action) = &item.action {
            actions.insert(native.id().clone(), action.clone());
        }
        return Box::new(native);
    }
    let native = muda::MenuItem::new(&item.label, item.enabled, shortcut);
    if let Some(action) = &item.action {
        actions.insert(native.id().clone(), action.clone());
    }
    Box::new(native)
}
