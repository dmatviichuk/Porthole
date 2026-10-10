use super::{Action, App, Overlay, style::*};
use crate::model::{self, Tab, Theme, View};
use eframe::egui::*;
pub fn kind_label(kind: &str) -> String {
    regex::Regex::new(r"([a-z])([A-Z])")
        .unwrap()
        .replace_all(kind, "$1 $2")
        .to_lowercase()
}
fn frame(t: Tokens, margin: i8) -> Frame {
    Frame::NONE
        .fill(t.bg)
        .stroke(Stroke::new(1.0, t.strong))
        .corner_radius(12)
        .inner_margin(margin)
        .shadow(Shadow {
            offset: [0, 12],
            blur: 36,
            spread: 0,
            color: Color32::from_black_alpha(80),
        })
}
#[allow(clippy::too_many_arguments)]
fn dialog_input(ui: &mut Ui, r: Rect, value: &mut String, id: &str, family: &str, hint: &str, t: Tokens) -> Response {
    ui.painter().rect_filled(r, 6, t.raised);
    let mut layouter = |ui: &Ui, value: &dyn TextBuffer, _: f32| {
        ui.painter().layout_job(text_job(value.as_str(), 13.0, family, t.text))
    };
    let response = ui.place(
        rect(r.left() + 9.0, r.top() + 7.0, r.width() - 18.0, 18.2),
        TextEdit::singleline(value)
            .id(Id::new(id))
            .font(font(13.0, family))
            .layouter(&mut layouter)
            .frame(Frame::NONE)
            .margin(0)
            .hint_text(hint),
    );
    ui.painter().rect_stroke(
        r,
        6,
        Stroke::new(1.0, if response.has_focus() { t.accent } else { t.strong }),
        StrokeKind::Inside,
    );
    response
}
fn paragraph(ui: &Ui, text: &str, width: f32, size: f32, family: &str, color: Color32) -> std::sync::Arc<Galley> {
    let mut job = text_job(text, size, family, color);
    for section in &mut job.sections {
        section.format.line_height = Some(size * 1.4);
    }
    css_paragraph(ui, job, width)
}
fn shortcut_section(ui: &Ui, origin: Pos2, width: f32, title: &str, rows: &[(String, String)], t: Tokens) -> f32 {
    label(
        ui,
        rect(origin.x, origin.y, width, 15.4),
        title,
        11.0,
        "medium",
        t.muted,
    );
    let mut y = origin.y + 21.4;
    for (name, keys) in rows {
        let keys: Vec<_> = keys.split_whitespace().collect();
        let widths: Vec<_> = keys
            .iter()
            .map(|key| text_width(ui, key, 11.5, "sans") + 14.0)
            .collect();
        let key_width = widths.iter().sum::<f32>() + 4.0 * keys.len().saturating_sub(1) as f32;
        let galley = paragraph(
            ui,
            name,
            width - key_width - 16.0,
            13.0,
            "sans",
            t.text.gamma_multiply(0.9),
        );
        let height = (galley.rows.len() as f32 * 18.2).max(22.0);
        ui.painter().galley(pos2(origin.x, y), galley, t.text);
        let mut x = origin.x + width - key_width;
        for (key, width) in keys.into_iter().zip(widths) {
            let key_rect = rect(x, y, width, 22.0);
            ui.painter().rect_filled(key_rect, 4, t.raised);
            ui.painter()
                .rect_stroke(key_rect, 4, Stroke::new(1.0, t.strong), StrokeKind::Inside);
            label(ui, key_rect.shrink2(vec2(7.0, 0.0)), key, 11.5, "sans", t.text);
            x += width + 4.0;
        }
        y += height + 6.0;
    }
    y - origin.y - 6.0
}
impl App {
    pub(super) fn show_menu(&mut self, ctx: &Context) {
        let Some((items, pos)) = self.menu.clone() else {
            return;
        };
        let mut close = false;
        Area::new(Id::new("context-menu"))
            .order(Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(160.0);
                    for item in items {
                        if item.label.is_empty() {
                            ui.separator();
                        } else if !item.children.is_empty() {
                            ui.menu_button(&item.label, |ui| {
                                for child in &item.children {
                                    if ui.add_enabled(child.enabled, Button::new(&child.label)).clicked() {
                                        if let Some(a) = &child.action {
                                            self.actions.push(a.clone());
                                        }
                                        close = true;
                                        ui.close();
                                    }
                                }
                            });
                        } else if ui
                            .add_enabled(item.enabled, Button::new(&item.label).frame(false))
                            .clicked()
                        {
                            if let Some(a) = item.action {
                                self.actions.push(a);
                            }
                            close = true;
                        }
                    }
                });
            });
        if close
            || ctx.input(|i| i.key_pressed(Key::Escape))
            || (ctx.input(|i| i.pointer.any_pressed()) && !ctx.is_pointer_over_egui())
        {
            self.menu = None;
        }
    }
    pub(super) fn dialogs(&mut self, ctx: &Context) {
        if self.delete.is_some() {
            self.delete_dialog(ctx);
        } else if self.label_draft.is_some() {
            self.label_dialog(ctx);
        } else {
            match self.overlay {
                Some(Overlay::Palette) => self.palette(ctx),
                Some(Overlay::Shortcuts) => self.shortcuts_dialog(ctx),
                None => {}
            }
        }
    }
    fn delete_dialog(&mut self, ctx: &Context) {
        let t = self.t;
        let mut delete = self.delete.take().unwrap();
        let kind = kind_label(&delete.resource.kind);
        let must_type = delete.resource.kind == "Namespace"
            || self
                .prefs
                .labels
                .get(&delete.context)
                .is_some_and(|l| l.confirm_deletes);
        let mut cancel = false;
        let mut confirm = false;
        let fresh = self.overlay_fresh;
        let _response = Modal::new(Id::new("delete"))
            .frame(frame(t, 20))
            .backdrop_color(Color32::from_black_alpha(102))
            .show(ctx, |ui| {
                ui.set_width(398.0);
                let origin = ui.cursor().min;
                let mut y = origin.y;
                let heading = paragraph(
                    ui,
                    &format!("Delete {kind} {}?", delete.name),
                    398.0,
                    15.0,
                    "semibold",
                    t.text,
                );
                let height = heading.size().y;
                ui.painter().galley(pos2(origin.x, y), heading, t.text);
                y += height + 12.0;
                let explanation = if delete.resource.kind == "Namespace" {
                    "Every resource in this namespace is deleted with it. This cannot be undone."
                } else if delete.resource.kind == "Pod" {
                    "If a controller manages this pod, it starts a replacement."
                } else {
                    "This cannot be undone."
                };
                let galley = paragraph(ui, explanation, 398.0, 13.0, "sans", t.muted);
                let height = galley.size().y;
                ui.painter().galley(pos2(origin.x, y), galley, t.muted);
                y += height + 12.0;
                let mut job = eframe::egui::text::LayoutJob::default();
                let mut append = |value: &str, family: &str, color| {
                    let mut format = text_format(13.0, family, color);
                    format.line_height = Some(18.2);
                    job.append(value, 0.0, format);
                };
                append("Cluster ", "sans", t.muted);
                append(&delete.context, "medium", t.text);
                if let Some(namespace) = &delete.namespace {
                    append(", namespace ", "sans", t.muted);
                    append(namespace, "medium", t.text);
                }
                append(".", "sans", t.muted);
                let galley = css_paragraph(ui, job, 398.0);
                let height = galley.size().y;
                ui.painter().galley(pos2(origin.x, y), galley, t.text);
                y += height + 12.0;
                if must_type {
                    let galley = paragraph(
                        ui,
                        &format!("Type {} to confirm", delete.name),
                        398.0,
                        13.0,
                        "sans",
                        t.muted,
                    );
                    let height = galley.size().y;
                    ui.painter().galley(pos2(origin.x, y), galley, t.muted);
                    y += height + 6.0;
                    let response = dialog_input(
                        ui,
                        rect(origin.x, y, 398.0, 32.2),
                        &mut delete.typed,
                        "delete-name",
                        "mono",
                        "",
                        t,
                    );
                    if fresh {
                        response.request_focus();
                    }
                    y += 32.2 + 12.0;
                }
                y += 8.0;
                let text = if delete.busy {
                    "Deleting…".into()
                } else {
                    format!("Delete {kind}")
                };
                let width = text_width(ui, &text, 13.0, "medium") + 24.0;
                let confirm_rect = rect(origin.x + 398.0 - width, y + 1.0, width, 30.2);
                let ready = (!must_type || delete.typed == delete.name) && !delete.busy;
                ui.add_enabled_ui(ready, |ui| {
                    let response = button_with_font(
                        ui,
                        confirm_rect,
                        "confirm-delete",
                        &text,
                        13.0,
                        "medium",
                        Color32::WHITE,
                        Some(t.failed),
                        None,
                        t,
                    );
                    if fresh && !must_type {
                        response.request_focus();
                    }
                    confirm = response.clicked();
                });
                cancel = button(
                    ui,
                    rect(confirm_rect.left() - 8.0 - 66.0, y, 66.0, 32.2),
                    "cancel-delete",
                    "Cancel",
                    13.0,
                    t.text,
                    None,
                    Some(t.strong),
                    t,
                )
                .clicked();
                ui.allocate_space(vec2(398.0, y + 32.2 - origin.y));
                if ui.input(|i| i.key_pressed(Key::Enter)) && ready {
                    confirm = true;
                }
            });
        self.overlay_fresh = false;
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if confirm {
            delete.busy = true;
            self.data.delete(
                delete.context.clone(),
                delete.resource.clone(),
                delete.namespace.clone(),
                delete.name.clone(),
            );
        }
        if !cancel {
            self.delete = Some(delete);
        }
    }
    fn label_dialog(&mut self, ctx: &Context) {
        let t = self.t;
        let mut draft = self.label_draft.take().unwrap();
        let existing = self.prefs.labels.contains_key(&draft.context);
        let mut cancel = false;
        let mut save = false;
        let mut remove = false;
        let fresh = self.overlay_fresh;
        let _response = Modal::new(Id::new("label-cluster"))
            .frame(frame(t, 20))
            .backdrop_color(Color32::from_black_alpha(102))
            .show(ctx, |ui| {
                ui.set_width(378.0);
                let origin = ui.cursor().min;
                let x = origin.x;
                let mut y = origin.y;
                label(ui, rect(x, y, 378.0, 21.0), "Label cluster", 15.0, "semibold", t.text);
                y += 21.0;
                label(ui, rect(x, y, 378.0, 18.2), &draft.context, 13.0, "sans", t.muted);
                y += 18.2 + 16.0;
                label(ui, rect(x, y, 378.0, 18.2), "Tag", 13.0, "sans", t.muted);
                y += 18.2;
                let response = dialog_input(
                    ui,
                    rect(x, y, 378.0, 32.2),
                    &mut draft.draft.tag,
                    "cluster-tag",
                    "sans",
                    "e.g. Production, Staging, Team A",
                    t,
                );
                if fresh {
                    response.request_focus();
                }
                if draft.draft.tag.chars().count() > 40 {
                    draft.draft.tag = draft.draft.tag.chars().take(40).collect();
                }
                y += 32.2 + 16.0;
                label(ui, rect(x, y, 378.0, 18.2), "Colour", 13.0, "sans", t.muted);
                y += 18.2 + 6.0;
                for (index, (name, color)) in LABEL_COLORS.into_iter().enumerate() {
                    let r = rect(x + index as f32 * 36.0, y, 28.0, 28.0);
                    let response = ui
                        .interact(r, ui.id().with(("label-color", name)), Sense::click())
                        .on_hover_text(name);
                    response.widget_info(|| {
                        WidgetInfo::selected(WidgetType::RadioButton, true, draft.draft.color == name, name)
                    });
                    ui.painter().circle_filled(r.center(), 14.0, hex(color));
                    if draft.draft.color == name {
                        ui.painter().circle_stroke(r.center(), 17.0, Stroke::new(2.0, t.text));
                        self.icons.paint(ui, "check", r.shrink(7.0), Color32::WHITE);
                    }
                    if response.clicked() {
                        draft.draft.color = name.into();
                    }
                }
                y += 28.0 + 16.0;
                let description = "Ask to type the name before deleting anything";
                let response = ui.interact(rect(x, y, 378.0, 35.0), ui.id().with("confirm-deletes"), Sense::click());
                response.widget_info(|| {
                    WidgetInfo::selected(WidgetType::Checkbox, true, draft.draft.confirm_deletes, description)
                });
                if response.clicked() {
                    draft.draft.confirm_deletes = !draft.draft.confirm_deletes;
                }
                let check = rect(x, y + 2.0, 13.0, 13.0);
                ui.painter()
                    .rect_filled(check, 3, if draft.draft.confirm_deletes { t.accent } else { t.bg });
                ui.painter()
                    .rect_stroke(check, 3, Stroke::new(1.0, t.muted), StrokeKind::Inside);
                if draft.draft.confirm_deletes {
                    self.icons.paint(ui, "check", check.shrink(1.5), Color32::WHITE);
                }
                label(ui, rect(x + 21.0, y, 357.0, 18.2), description, 13.0, "sans", t.text);
                label(
                    ui,
                    rect(x + 21.0, y + 18.2, 357.0, 16.8),
                    "For clusters where a slip is expensive.",
                    12.0,
                    "sans",
                    t.muted,
                );
                y += 35.0 + 16.0;
                if !draft.draft.tag.trim().is_empty() {
                    let width = text_width(ui, "Preview: ", 12.0, "sans");
                    label(ui, rect(x, y, width, 16.8), "Preview: ", 12.0, "sans", t.muted);
                    label(
                        ui,
                        rect(x + width, y, 378.0 - width, 16.8),
                        draft.draft.tag.trim(),
                        12.0,
                        "medium",
                        label_color(&draft.draft.color),
                    );
                    y += 16.8 + 16.0;
                }
                y += 4.0;
                if existing {
                    remove = button(
                        ui,
                        rect(x, y, 99.0, 32.2),
                        "remove-label",
                        "Remove label",
                        13.0,
                        t.muted,
                        None,
                        None,
                        t,
                    )
                    .clicked();
                }
                save = button_with_font(
                    ui,
                    rect(x + 324.0, y + 1.0, 54.0, 30.2),
                    "save-label",
                    "Save",
                    13.0,
                    "medium",
                    Color32::WHITE,
                    Some(t.accent),
                    None,
                    t,
                )
                .clicked();
                cancel = button(
                    ui,
                    rect(x + 250.0, y, 66.0, 32.2),
                    "cancel-label",
                    "Cancel",
                    13.0,
                    t.text,
                    None,
                    Some(t.strong),
                    t,
                )
                .clicked();
                ui.allocate_space(vec2(378.0, y + 32.2 - origin.y));
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    save = true;
                }
            });
        self.overlay_fresh = false;
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if save {
            draft.draft.tag = draft.draft.tag.trim().into();
            if draft.draft.tag.is_empty() && !draft.draft.confirm_deletes {
                self.prefs.labels.remove(&draft.context);
            } else {
                self.prefs.labels.insert(draft.context.clone(), draft.draft.clone());
            }
        } else if remove {
            self.prefs.labels.remove(&draft.context);
        } else if !cancel {
            self.label_draft = Some(draft);
        }
    }
    fn palette(&mut self, ctx: &Context) {
        let t = self.t;
        let typed = !self.palette_query.trim().is_empty();
        let mut items = self.palette_items(typed);
        let terms: Vec<_> = self
            .palette_query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        items.retain(|item| {
            model::matches(
                &self.palette_query,
                &format!("{} {} {}", item.label, item.detail, item.keywords),
            )
        });
        let mut groups: Vec<(String, Vec<PaletteItem>)> = vec![];
        for item in items {
            if let Some((_, g)) = groups.iter_mut().find(|(n, _)| *n == item.group) {
                g.push(item);
            } else {
                groups.push((item.group.clone(), vec![item]));
            }
        }
        let mut shown = vec![];
        for (group, mut list) in groups {
            list.sort_by_key(|item| std::cmp::Reverse(palette_score(item, &terms)));
            if typed {
                list.truncate(if group == "Open" { 12 } else { 8 });
            }
            shown.extend(list);
        }
        self.palette_cursor = self.palette_cursor.min(shown.len().saturating_sub(1));
        let fresh = self.overlay_fresh;
        let mut action = None;
        let mut close = false;
        let screen = ctx.content_rect();
        let response = Modal::new(Id::new("palette"))
            .area(
                Area::new(Id::new("palette"))
                    .order(Order::Foreground)
                    .default_size(vec2(560.0, 530.0))
                    .fixed_pos(pos2(screen.center().x - 280.0, screen.top() + screen.height() * 0.12)),
            )
            .frame(frame(t, 0))
            .backdrop_color(Color32::from_black_alpha(102))
            .show(ctx, |ui| {
                ui.set_width(558.0);
                ui.set_max_height(48.0 + (screen.height() * 0.6).min(480.0));
                let origin = ui.cursor().min;
                let sr = rect(origin.x, origin.y, 558.0, 48.0);
                self.icons.paint(
                    ui,
                    "search",
                    rect(sr.left() + 16.0, sr.center().y - 7.5, 15.0, 15.0),
                    t.muted,
                );
                let response = ui.place(
                    rect(sr.left() + 39.0, sr.top() + 14.0, 503.0, 20.0),
                    TextEdit::singleline(&mut self.palette_query)
                        .font(font(14.0, "sans"))
                        .hint_text("Go to a view, type, namespace, object or cluster…")
                        .frame(Frame::NONE)
                        .margin(0)
                        .id(Id::new("palette-query")),
                );
                if fresh {
                    response.request_focus();
                }
                if response.changed() {
                    self.palette_cursor = 0;
                }
                ui.allocate_space(vec2(558.0, 48.0));
                line(ui, sr.left_bottom(), sr.right_bottom(), t.line);
                let count = shown.len();
                let down = ui.input(|i| i.key_pressed(Key::ArrowDown) || (i.modifiers.ctrl && i.key_pressed(Key::N)));
                let up = ui.input(|i| i.key_pressed(Key::ArrowUp) || (i.modifiers.ctrl && i.key_pressed(Key::P)));
                if count > 0 {
                    if down {
                        self.palette_cursor = (self.palette_cursor + 1) % count;
                    }
                    if up {
                        self.palette_cursor = (self.palette_cursor + count - 1) % count;
                    }
                    if ui.input(|i| i.key_pressed(Key::Enter)) {
                        action = Some(shown[self.palette_cursor].action.clone());
                    }
                }
                if ui.input(|i| i.modifiers.command && i.key_pressed(Key::K)) {
                    close = true;
                }
                ScrollArea::vertical()
                    .id_salt("palette-list")
                    .max_height((screen.height() * 0.6).min(480.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        if shown.is_empty() {
                            ui.add_space(12.0);
                            ui.label(format!("Nothing matches \"{}\".", self.palette_query));
                            ui.add_space(12.0);
                        }
                        let mut group = String::new();
                        for (index, item) in shown.iter().enumerate() {
                            if item.group != group {
                                group = item.group.clone();
                                let o = ui.cursor().min;
                                let width = ui.available_width();
                                ui.allocate_space(vec2(width, 27.4));
                                label(
                                    ui,
                                    rect(o.x + 16.0, o.y + 8.0, width - 32.0, 15.4),
                                    &group,
                                    11.0,
                                    "medium",
                                    t.muted,
                                );
                            }
                            let o = ui.cursor().min;
                            let width = ui.available_width();
                            let rr = rect(o.x + 6.0, o.y, width - 12.0, 30.2);
                            ui.allocate_space(vec2(width, 30.2));
                            let response = ui.interact(rr, ui.id().with(index), Sense::click());
                            if index == self.palette_cursor {
                                ui.painter().rect_filled(rr, 6, t.selected);
                            }
                            if response.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                                self.palette_cursor = index;
                            }
                            let label_w = text_width(ui, &item.label, 13.0, "sans").min(380.0);
                            label(
                                ui,
                                rect(rr.left() + 10.0, rr.top(), label_w, rr.height()),
                                &item.label,
                                13.0,
                                "sans",
                                t.text,
                            );
                            label(
                                ui,
                                rect(
                                    rr.left() + 22.0 + label_w,
                                    rr.top(),
                                    (rr.width() - label_w - 40.0).max(0.0),
                                    rr.height(),
                                ),
                                &item.detail,
                                13.0,
                                "sans",
                                t.muted,
                            );
                            if !item.hint.is_empty() {
                                let w = text_width(ui, &item.hint, 12.0, "sans");
                                label(
                                    ui,
                                    rect(rr.right() - 10.0 - w, rr.top(), w, rr.height()),
                                    &item.hint,
                                    12.0,
                                    "sans",
                                    t.muted,
                                );
                            }
                            if response.clicked() {
                                action = Some(item.action.clone());
                            }
                            if index == self.palette_cursor && (down || up) {
                                ui.scroll_to_rect(rr, None);
                            }
                        }
                        ui.add_space(6.0);
                    });
            });
        self.overlay_fresh = false;
        if response.should_close() || close || action.is_some() {
            self.overlay = None;
        }
        if let Some(a) = action {
            self.actions.push(a);
        }
    }
    fn palette_items(&mut self, typed: bool) -> Vec<PaletteItem> {
        let command = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
        let mut list = vec![
            PaletteItem::new(
                "Go to",
                "Applications",
                "",
                &format!("{command}1"),
                Action::Navigate(View::Applications),
            ),
            PaletteItem::new(
                "Go to",
                "All Resources",
                "",
                &format!("{command}2"),
                Action::Navigate(View::AllResources),
            ),
            PaletteItem::new(
                "Go to",
                "Overview",
                "",
                &format!("{command}3"),
                Action::Navigate(View::Overview),
            ),
        ];
        if typed {
            if let Some(d) = self.data.context_data.get(&self.context) {
                for info in &d.types {
                    let mut item = PaletteItem::new(
                        "Resource types",
                        &model::plural(&info.resource.kind),
                        &model::api_version(&info.resource),
                        "",
                        Action::Navigate(View::Resources(info.resource.clone())),
                    );
                    item.keywords = format!("{} {}", info.resource.kind, info.resource.plural);
                    list.push(item);
                }
            }
            list.push(PaletteItem::new(
                "Namespaces",
                "All namespaces",
                if self.namespace.is_none() { "Current" } else { "" },
                "",
                Action::Namespace(None),
            ));
            let mut names: Vec<_> = self
                .snapshot("Namespace", None)
                .rows
                .into_iter()
                .map(|r| r.name)
                .collect();
            names.sort();
            for name in names {
                list.push(PaletteItem::new(
                    "Namespaces",
                    &name,
                    if self.namespace.as_ref() == Some(&name) {
                        "Namespace, current"
                    } else {
                        "Namespace"
                    },
                    "",
                    Action::Namespace(Some(name.clone())),
                ));
            }
            for kind in [
                "Deployment",
                "StatefulSet",
                "DaemonSet",
                "CronJob",
                "Job",
                "Pod",
                "Service",
                "Ingress",
                "Node",
            ] {
                for row in self.snapshot(kind, None).rows {
                    list.push(PaletteItem::new(
                        "Open",
                        &row.name,
                        &format!(
                            "{kind}{}",
                            row.namespace.as_ref().map(|ns| format!(" · {ns}")).unwrap_or_default()
                        ),
                        "",
                        Action::Navigate(Self::resource_view(model::resource(kind), &row, Tab::Overview)),
                    ));
                }
            }
        }
        if let Ok(contexts) = &self.data.contexts {
            for c in &contexts.contexts {
                let detail = [
                    self.prefs
                        .labels
                        .get(&c.name)
                        .map(|l| l.tag.as_str())
                        .unwrap_or_default(),
                    if c.name == self.context { "Current" } else { "" },
                ]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
                let mut item = PaletteItem::new("Clusters", &c.name, &detail, "", Action::Context(c.name.clone()));
                item.keywords = "cluster context".into();
                list.push(item);
            }
        }
        for s in &self.shells {
            let mut item = PaletteItem::new(
                "Shells",
                &format!("{} · {}", s.pod, s.container),
                if self.active_shell == Some(s.key) {
                    "Shell, current"
                } else {
                    "Shell"
                },
                if self.active_shell == Some(s.key) { "⌃`" } else { "" },
                Action::ActiveShell(s.key),
            );
            item.keywords = format!("shell terminal {}", s.namespace);
            list.push(item);
        }
        list.push(PaletteItem::new(
            "Commands",
            "Keyboard shortcuts",
            "",
            "?",
            Action::Overlay(Overlay::Shortcuts),
        ));
        if let Some(s) = self.shells.iter().find(|s| Some(s.key) == self.active_shell) {
            list.push(PaletteItem::new(
                "Commands",
                &format!("Close shell {} · {}", s.pod, s.container),
                "",
                "",
                Action::CloseShell(s.key),
            ));
        }
        if !self.context.is_empty() {
            list.push(PaletteItem::new(
                "Commands",
                if self.prefs.labels.contains_key(&self.context) {
                    "Edit cluster label…"
                } else {
                    "Label this cluster…"
                },
                "",
                "",
                Action::Label(self.context.clone()),
            ));
        }
        list.push(PaletteItem::new(
            "Commands",
            "Reload kubeconfig",
            "",
            "",
            Action::Reload,
        ));
        list.push(PaletteItem::new(
            "Commands",
            "Rediscover resource types",
            "",
            "",
            Action::Rediscover,
        ));
        for (theme, title) in [(Theme::Light, "Light"), (Theme::Dark, "Dark"), (Theme::Auto, "Auto")] {
            list.push(PaletteItem::new(
                "Commands",
                &format!("Theme: {title}"),
                if self.prefs.theme == theme { "Current" } else { "" },
                "",
                Action::Theme(theme),
            ));
        }
        list
    }
    fn shortcuts_dialog(&mut self, ctx: &Context) {
        let t = self.t;
        let mut done = false;
        let response = Modal::new(Id::new("shortcuts"))
            .frame(frame(t, 20))
            .backdrop_color(Color32::from_black_alpha(102))
            .show(ctx, |ui| {
                ui.set_width(598.0);
                ui.set_max_height(ctx.content_rect().height() * 0.8 - 42.0);
                let origin = ui.cursor().min;
                label(
                    ui,
                    rect(origin.x, origin.y, 400.0, 27.5),
                    "Keyboard shortcuts",
                    15.0,
                    "semibold",
                    t.text,
                );
                let done_rect = rect(origin.x + 570.5, origin.y, 27.5, 27.5);
                let response = ui.interact(done_rect, ui.id().with("shortcuts-close"), Sense::click());
                response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Close keyboard shortcuts"));
                if response.hovered() {
                    ui.painter().rect_filled(done_rect, 6, t.hover);
                }
                self.icons.paint(ui, "x", done_rect.shrink(6.0), t.muted);
                if self.overlay_fresh {
                    response.request_focus();
                }
                if response.has_focus() {
                    ui.painter()
                        .rect_stroke(done_rect, 6, Stroke::new(2.0, t.accent), StrokeKind::Inside);
                }
                done = response.clicked();
                ui.allocate_space(vec2(598.0, 43.5));
                ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.8 - 68.0)
                    .show(ui, |ui| {
                        let m = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
                        let sections: Vec<(&str, Vec<(String, String)>)> = vec![
                            (
                                "Anywhere",
                                vec![
                                    (
                                        "Command palette: views, resource types, namespaces, clusters, objects".into(),
                                        format!("{m}K"),
                                    ),
                                    (
                                        "Applications, All Resources, Overview".into(),
                                        format!("{m}1  {m}2  {m}3"),
                                    ),
                                    ("Back, forward".into(), format!("{m}[  {m}]")),
                                    (
                                        "Next, previous tab".into(),
                                        if cfg!(target_os = "macos") {
                                            "⌃Tab  ⌃⇧Tab"
                                        } else {
                                            "Ctrl+Tab  Ctrl+Shift+Tab"
                                        }
                                        .into(),
                                    ),
                                    ("Search this view (logs filter, YAML find)".into(), format!("{m}F  /")),
                                    (
                                        "Move focus between the shells and the view".into(),
                                        if cfg!(target_os = "macos") { "⌃`" } else { "Ctrl+`" }.into(),
                                    ),
                                    ("This list".into(), "?".into()),
                                ],
                            ),
                            (
                                "Tables",
                                vec![
                                    ("Move (also K, J)".into(), "↑  ↓".into()),
                                    (format!("First, last row (also {m}↑, {m}↓)"), "Home  End".into()),
                                    ("Move a page".into(), "PgUp  PgDn".into()),
                                    ("Open".into(), "↩".into()),
                                    ("View logs".into(), "L".into()),
                                    ("Open shell".into(), "S".into()),
                                    ("Edit YAML".into(), "Y".into()),
                                    ("Copy name".into(), "C".into()),
                                    ("Actions menu, the same as a right click".into(), "M".into()),
                                ],
                            ),
                            (
                                "Search fields",
                                vec![
                                    ("Go to the results".into(), "↓  ↩".into()),
                                    ("Clear; again to go to the results".into(), "Esc".into()),
                                ],
                            ),
                            ("YAML", vec![("Save changes".into(), format!("{m}S"))]),
                        ];
                        let origin = ui.cursor().min;
                        let width = ui.available_width();
                        let column = (width - 32.0) / 2.0;
                        let mut y = origin.y;
                        for pair in sections.chunks(2) {
                            let mut height: f32 = 0.0;
                            for (index, (title, rows)) in pair.iter().enumerate() {
                                height = height.max(shortcut_section(
                                    ui,
                                    pos2(origin.x + index as f32 * (column + 32.0), y),
                                    column,
                                    title,
                                    rows,
                                    t,
                                ));
                            }
                            y += height + 20.0;
                        }
                        ui.allocate_space(vec2(width, y - origin.y - 20.0));
                    });
            });
        if response.should_close() || done {
            self.overlay = None;
        }
        self.overlay_fresh = false;
    }
    pub(super) fn show_notices(&mut self, ctx: &Context, r: Rect) {
        if self.notices.is_empty() {
            return;
        }
        Area::new(Id::new("notices"))
            .order(Order::Tooltip)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-24.0, -24.0))
            .show(ctx, |ui| {
                ui.set_max_width((r.width() - 48.0).min(420.0));
                ui.spacing_mut().item_spacing.y = 8.0;
                let mut dismiss = None;
                for (i, n) in self.notices.iter().enumerate() {
                    Frame::NONE
                        .fill(self.t.raised)
                        .stroke(Stroke::new(1.0, self.t.strong))
                        .corner_radius(8)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                self.icons.paint(
                                    ui,
                                    if n.error { "x" } else { "check" },
                                    rect(ui.cursor().min.x, ui.cursor().min.y + 2.0, 14.0, 14.0),
                                    if n.error { self.t.failed } else { self.t.ok },
                                );
                                ui.add_space(22.0);
                                ui.label(&n.text);
                                if ui.small_button("×").clicked() {
                                    dismiss = Some(i);
                                }
                            });
                        });
                }
                if let Some(i) = dismiss {
                    self.notices.remove(i);
                }
            });
    }
}
struct PaletteItem {
    group: String,
    label: String,
    detail: String,
    keywords: String,
    hint: String,
    action: Action,
}
impl PaletteItem {
    fn new(group: &str, label: &str, detail: &str, hint: &str, action: Action) -> Self {
        Self {
            group: group.into(),
            label: label.into(),
            detail: detail.into(),
            keywords: String::new(),
            hint: hint.into(),
            action,
        }
    }
}
fn palette_score(item: &PaletteItem, terms: &[String]) -> u8 {
    let label = item.label.to_lowercase();
    let first = terms.first().map(String::as_str).unwrap_or_default();
    if label == terms.join(" ") {
        3
    } else if label.starts_with(first) {
        2
    } else if label
        .split([' ', '-', '_', '.', '/', ':'])
        .any(|word| word.starts_with(first))
    {
        1
    } else {
        0
    }
}
