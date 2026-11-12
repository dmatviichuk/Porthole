use super::{App, style::*};
use crate::{
    logs::{LogEvent, LogTarget},
    model,
    resources::ResourceType,
    summary::ResourceRow,
};
use eframe::egui::{
    text::CCursor,
    text_selection::{
        CCursorRange,
        text_cursor_state::{ccursor_next_word, ccursor_previous_word},
    },
    *,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};
#[derive(Default)]
pub(super) struct State {
    pub token: u64,
    pub sessions: HashMap<String, u64>,
    pub sources: Vec<String>,
    pub lines: Vec<Line>,
    filter: String,
    follow: bool,
    initialized: bool,
    all_selected: bool,
    /// Only on-screen lines are laid out, so egui's label selection drops a
    /// selection whose ends scroll away. This one is kept by line id instead.
    selection: Option<Selection>,
    selecting: bool,
    next_id: u64,
    cache: HashMap<String, Vec<Part>>,
}
pub(super) struct Line {
    /// Stays with the line while older lines are dropped from the front.
    id: u64,
    source: usize,
    ts: Option<String>,
    text: String,
    system: bool,
    layout: Option<((u32, bool), f32)>,
    raw_length: usize,
    display_length: std::sync::OnceLock<(usize, f32)>,
}
impl Line {
    fn new(source: usize, ts: Option<String>, text: String, system: bool) -> Self {
        let raw_length = text.chars().count();
        Self {
            id: 0,
            source,
            ts,
            text,
            system,
            layout: None,
            raw_length,
            display_length: std::sync::OnceLock::new(),
        }
    }
}
#[derive(Clone)]
struct Part {
    text: String,
    tone: Tone,
}
#[derive(Clone, Copy)]
enum Tone {
    Text,
    Muted,
    Faint,
    Key,
    String,
    Number,
    Error,
    Warn,
    Info,
    Debug,
    Gap(f32),
}
/// A character offset into the text a line shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Point {
    line: u64,
    char: usize,
}
#[derive(Clone, Copy, Debug)]
struct Selection {
    anchor: Point,
    head: Point,
    /// Formatting changes the shown text, and the offsets with it.
    formatted: bool,
}
impl Selection {
    fn ordered(&self) -> (Point, Point) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }
    fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
    /// The selected characters of a line that shows `length` characters.
    fn range(&self, line: u64, length: usize) -> Option<Range<usize>> {
        let (start, end) = self.ordered();
        if line < start.line || line > end.line {
            return None;
        }
        let from = if line == start.line { start.char.min(length) } else { 0 };
        let to = if line == end.line { end.char.min(length) } else { length };
        Some(from..to.max(from))
    }
}
/// How the panel shows each line.
struct View<'a> {
    timestamps: bool,
    wrap: bool,
    formatted: bool,
    /// Source labels, when lines come from more than one container.
    labels: Option<&'a HashMap<String, String>>,
}
/// A line laid out on screen: its index, top, text position and text.
type Shown = (usize, f32, Pos2, Arc<Galley>);
impl State {
    fn push(&mut self, mut line: Line) {
        line.id = self.next_id;
        self.next_id += 1;
        self.lines.push(line);
    }
    pub fn receive(&mut self, key: String, event: LogEvent) {
        if !self.sessions.contains_key(&key) {
            return;
        }
        let source = self.sources.iter().position(|s| *s == key).unwrap_or_else(|| {
            self.sources.push(key.clone());
            self.sources.len() - 1
        });
        match event {
            LogEvent::Lines { lines } => {
                static ANSI: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
                let re = ANSI.get_or_init(|| regex::Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap());
                for l in lines {
                    self.push(Line::new(source, l.ts, re.replace_all(&l.text, "").into_owned(), false));
                }
            }
            LogEvent::Ended { .. } => self.push(Line::new(
                source,
                None,
                format!(
                    "{}: log stream ended",
                    key.split('/').skip(1).collect::<Vec<_>>().join("/")
                ),
                true,
            )),
            LogEvent::Error { message, .. } => self.push(Line::new(
                source,
                None,
                format!("{}: {message}", key.split('/').skip(1).collect::<Vec<_>>().join("/")),
                true,
            )),
        }
        if self.lines.len() > 50000 {
            self.lines.drain(..self.lines.len() - 50000);
        }
    }
    /// The lines as plain text, with the timestamp and source columns the panel shows.
    fn plain_text(&self, visible: &[usize], timestamps: bool, labels: Option<&HashMap<String, String>>) -> String {
        visible
            .iter()
            .map(|&i| {
                let line = &self.lines[i];
                let mut columns = vec![];
                if timestamps && let Some(ts) = &line.ts {
                    columns.push(log_time(ts));
                }
                if let Some(labels) = labels {
                    columns.push(source_label(&self.sources, labels, line.source).to_owned());
                }
                columns.push(line.text.clone());
                columns.join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// The selected text as the panel shows it, including lines that are off screen.
    fn selected_text(&self, visible: &[usize], selection: Selection) -> String {
        let (start, end) = selection.ordered();
        visible
            .iter()
            .map(|&i| &self.lines[i])
            .filter(|line| (start.line..=end.line).contains(&line.id))
            .map(|line| {
                let parts = shown_parts(line, selection.formatted);
                let length = parts.iter().map(|p| p.text.chars().count()).sum();
                slice(&parts, selection.range(line.id, length).unwrap_or_default())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// The lines below the toolbar: virtualised, selectable and copyable.
    fn show(&mut self, ui: &mut Ui, body: Rect, visible: &[usize], view: &View, keyboard_free: bool, t: Tokens) {
        let char_w = text_width(ui, "0000000000", 12.0, "mono") / 10.0;
        let prefix = 24.0
            + if view.timestamps { 12.0 * char_w + 12.0 } else { 0.0 }
            + if view.labels.is_some() { 162.0 } else { 0.0 };
        let text_width = (body.width() - prefix - 24.0).max(120.0);
        if ui.input(|i| i.pointer.any_pressed() || i.key_pressed(Key::Escape)) {
            self.all_selected = false;
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) || self.selection.is_some_and(|s| s.formatted != view.formatted) {
            self.selection = None;
        }
        // Only on-screen lines are laid out, so egui's own select-all would stop
        // at the line that holds the selection. Consumed before anything sees it.
        if keyboard_free && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
            self.all_selected = true;
            self.selection = None;
        }
        if keyboard_free && ui.input(|i| i.events.iter().any(|e| matches!(e, Event::Copy | Event::Cut))) {
            let text = if self.all_selected {
                self.plain_text(visible, view.timestamps, view.labels)
            } else if let Some(selection) = self.selection.filter(|s| !s.is_empty()) {
                self.selected_text(visible, selection)
            } else {
                String::new()
            };
            if !text.is_empty() {
                ui.ctx().copy_text(text);
            }
        }
        let formatted = view.formatted;
        let wrap = view.wrap;
        let layout_key = (if wrap { text_width.to_bits() } else { 0 }, formatted);
        let mut heights = Vec::with_capacity(visible.len() + 1);
        heights.push(0.0);
        for &i in visible {
            let line = &self.lines[i];
            let height = line
                .layout
                .filter(|(key, _)| *key == layout_key)
                .map(|(_, h)| h)
                .unwrap_or_else(|| {
                    if !wrap {
                        return 19.0;
                    }
                    let (characters, gaps) = if formatted && !line.system {
                        *line.display_length.get_or_init(|| {
                            parse(&line.text).iter().fold((0, 0.0), |(characters, gaps), part| {
                                (
                                    characters + part.text.chars().count(),
                                    gaps + if let Tone::Gap(w) = part.tone { w } else { 0.0 },
                                )
                            })
                        })
                    } else {
                        (line.raw_length, 0.0)
                    };
                    19.0 * ((characters as f32 * char_w + gaps) / text_width).ceil().max(1.0)
                });
            heights.push(heights.last().copied().unwrap() + height);
        }
        if self.cache.len() > 5000 {
            self.cache.clear();
        }
        let total = *heights.last().unwrap_or(&0.0);
        // A press on the lines holds them still, so the text under the pointer
        // stays put while new lines arrive.
        let holding = self.selecting
            || ui.input(|i| i.pointer.primary_pressed() && i.pointer.press_origin().is_some_and(|p| body.contains(p)));
        let mut at_bottom = self.follow;
        let mut measured_heights = Vec::new();
        ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
            ui.set_clip_rect(body);
            let scroll = ScrollArea::new([!wrap, true])
                .id_salt("logs-scroll")
                .auto_shrink([false, false])
                .stick_to_bottom(self.follow && !holding)
                .show_viewport(ui, |ui, viewport| {
                    ui.set_min_size(vec2(
                        if wrap {
                            body.width()
                        } else {
                            body.width().max(
                                prefix
                                    + visible
                                        .iter()
                                        .map(|&i| self.lines[i].raw_length as f32 * char_w)
                                        .fold(0.0, f32::max)
                                    + 24.0,
                            )
                        },
                        total,
                    ));
                    let origin = ui.min_rect().min;
                    let area = viewport.translate(origin.to_vec2());
                    let response = ui.interact(area, ui.scope_id().with("log-lines"), Sense::click_and_drag());
                    let first = heights.partition_point(|h| *h < viewport.top()).saturating_sub(2);
                    let last = heights
                        .partition_point(|h| *h < viewport.bottom() + 40.0)
                        .min(visible.len());
                    let mut shown: Vec<Shown> = Vec::with_capacity(last.saturating_sub(first));
                    for (index, top) in visible[first.min(last)..last].iter().zip(&heights[first.min(last)..]) {
                        let line = &self.lines[*index];
                        let parts = if formatted && !line.system {
                            self.cache
                                .entry(line.text.clone())
                                .or_insert_with(|| parse(&line.text))
                                .clone()
                        } else {
                            shown_parts(line, false)
                        };
                        let job = log_job(&parts, &self.filter, t, if wrap { text_width } else { f32::INFINITY });
                        let galley = ui.painter().layout_job(job);
                        let h = galley.size().y;
                        if line.layout.is_none_or(|(key, height)| key != layout_key || height != h) {
                            measured_heights.push((*index, h));
                        }
                        let leading = galley
                            .rows
                            .first()
                            .and_then(|r| r.glyphs.first())
                            .map_or(0.0, |glyph| (glyph.line_height - glyph.font_height).max(0.0) / 2.0);
                        let y = origin.y + top;
                        shown.push((*index, y, pos2(origin.x + prefix, y + leading), galley));
                    }
                    self.select(ui, &response, area, &shown, view);
                    let visuals = ui.visuals().clone();
                    let selection = self.selection;
                    for (index, y, text_pos, mut galley) in shown {
                        let line = &self.lines[index];
                        let all = self.all_selected;
                        if all {
                            // The drag-selection look, carried across the timestamp and source
                            // columns of the first row.
                            if let Some(row) = galley.rows.first() {
                                let row = row.rect().translate(text_pos.to_vec2());
                                ui.painter().rect_filled(
                                    Rect::from_x_y_ranges(origin.x + 24.0..=text_pos.x, row.y_range()),
                                    0.0,
                                    visuals.selection.bg_fill,
                                );
                            }
                        }
                        let range = if all {
                            Some(0..usize::MAX)
                        } else {
                            selection.and_then(|s| s.range(line.id, galley.text().chars().count()))
                        };
                        if let Some(range) = range.filter(|r| !r.is_empty()) {
                            let range = CCursorRange::two(
                                CCursor::new(range.start),
                                CCursor::new(range.end.min(galley.text().chars().count())),
                            );
                            text_selection::visuals::paint_text_selection(&mut galley, &visuals, &range, None);
                        }
                        let mut x = origin.x + 24.0;
                        if view.timestamps {
                            let timestamp = line.ts.as_deref().map(log_time).unwrap_or_default();
                            let color = if all { visuals.selection.stroke.color } else { t.faint };
                            label(ui, rect(x, y, 12.0 * char_w, 19.0), &timestamp, 12.0, "mono", color);
                            x += 12.0 * char_w + 12.0;
                        }
                        if let Some(labels) = view.labels {
                            label(
                                ui,
                                rect(x, y, 150.0, 19.0),
                                source_label(&self.sources, labels, line.source),
                                12.0,
                                "mono",
                                if all {
                                    visuals.selection.stroke.color
                                } else {
                                    hex(["#4f9cf9", "#e5a03c", "#3cc97c", "#c47cf0", "#f06f8a", "#3cc3c9"]
                                        [line.source % 6])
                                },
                            );
                        }
                        ui.painter().galley(text_pos, galley, t.text);
                    }
                });
            at_bottom = scroll.state.clamped_offset().y + body.height() >= total - 24.0;
        });
        if !measured_heights.is_empty() {
            for (index, height) in measured_heights {
                self.lines[index].layout = Some((layout_key, height));
            }
            ui.ctx().request_repaint();
        }
        self.follow = at_bottom;
    }
    /// Drag, shift-click, double-click (word) and triple-click (line) selection.
    fn select(&mut self, ui: &Ui, response: &Response, area: Rect, shown: &[Shown], view: &View) {
        if response.hovered() || self.selecting {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
        }
        // The line and character under a point, clamped to the lines on screen.
        let hit = |pos: Pos2| -> Option<(usize, usize)> {
            let pos = pos.clamp(area.min, area.max);
            let at = shown.iter().rposition(|(_, y, ..)| *y <= pos.y).unwrap_or(0);
            let (_, _, text_pos, galley) = shown.get(at)?;
            let local = pos - *text_pos;
            let char = if local.y >= galley.size().y {
                galley.text().chars().count()
            } else {
                galley.cursor_from_pos(local).index.0
            };
            Some((at, char))
        };
        let point = |(at, char): (usize, usize)| Point {
            line: self.lines[shown[at].0].id,
            char,
        };
        let (pressed, down, shift, pointer) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.modifiers.shift,
                i.pointer.interact_pos(),
            )
        });
        let target = pointer.and_then(hit);
        if pressed && response.is_pointer_button_down_on() {
            if let Some(target) = target {
                let head = point(target);
                let anchor = match self.selection {
                    Some(s) if shift => s.anchor,
                    _ => head,
                };
                self.selection = Some(Selection {
                    anchor,
                    head,
                    formatted: view.formatted,
                });
                self.selecting = true;
            }
        } else if self.selecting && down {
            if let (Some(selection), Some(target)) = (&mut self.selection, target) {
                selection.head = point(target);
            }
            // Dragging past an edge scrolls toward it.
            if let Some(p) = pointer {
                let delta = vec2(
                    if view.wrap { 0.0 } else { overshoot(p.x, area.x_range()) },
                    overshoot(p.y, area.y_range()),
                );
                if delta != Vec2::ZERO {
                    ui.scroll_with_delta_animation(delta, style::ScrollAnimation::none());
                    ui.ctx().request_repaint();
                }
            }
        }
        if !down {
            self.selecting = false;
        }
        let clicks = if response.triple_clicked() {
            3
        } else if response.double_clicked() {
            2
        } else {
            0
        };
        if clicks > 0
            && let Some((at, char)) = target
        {
            let text = shown[at].3.text();
            let (from, to) = if clicks == 3 {
                (0, text.chars().count())
            } else {
                let c = CCursor::new(char);
                (
                    ccursor_previous_word(text, c).index.0,
                    ccursor_next_word(text, c).index.0,
                )
            };
            self.selection = Some(Selection {
                anchor: point((at, from)),
                head: point((at, to)),
                formatted: view.formatted,
            });
        }
        if !self.selecting && self.selection.is_some_and(|s| s.is_empty()) {
            self.selection = None;
        }
    }
}
/// How far a point lies outside a range, as a scroll delta toward it.
fn overshoot(at: f32, range: Rangef) -> f32 {
    if at < range.min {
        range.min - at
    } else if at > range.max {
        range.max - at
    } else {
        0.0
    }
}
/// The parts a line shows: its fields when formatted, else the text as received.
fn shown_parts(line: &Line, formatted: bool) -> Vec<Part> {
    if formatted && !line.system {
        parse(&line.text)
    } else {
        vec![Part {
            text: line.text.clone(),
            tone: if line.system { Tone::Muted } else { Tone::Text },
        }]
    }
}
/// The characters of `range` in the shown parts. A gap between fields inside
/// the range copies as one space.
fn slice(parts: &[Part], range: Range<usize>) -> String {
    let mut text = String::new();
    let mut at = 0;
    for part in parts {
        if let Tone::Gap(_) = part.tone {
            if at > range.start && at < range.end && !text.ends_with(' ') {
                text.push(' ');
            }
            continue;
        }
        for c in part.text.chars() {
            if range.contains(&at) {
                text.push(c);
            }
            at += 1;
        }
    }
    text
}
fn targets(resource: &ResourceType, pods: &[ResourceRow], container: Option<&str>) -> Vec<LogTarget> {
    pods.iter()
        .flat_map(|p| {
            model::containers(p)
                .into_iter()
                .filter(|c| {
                    (resource.kind == "Pod" || !c.init)
                        && !model::not_started(&c.state)
                        && container.is_none_or(|name| c.name == name)
                })
                .map(|c| LogTarget {
                    namespace: p.namespace.clone().unwrap_or_else(|| "default".into()),
                    pod: p.name.clone(),
                    container: c.name,
                })
        })
        .collect()
}
impl App {
    pub(super) fn logs_panel(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        resource: &ResourceType,
        pods: &[ResourceRow],
        container: Option<&str>,
    ) {
        let t = self.t;
        if !self.logs.initialized {
            self.logs.token = self.id();
            self.logs.follow = true;
            self.logs.initialized = true;
        }
        let targets = targets(resource, pods, container);
        let wanted: HashSet<_> = targets.iter().map(target_key).collect();
        let gone: Vec<_> = self
            .logs
            .sessions
            .keys()
            .filter(|key| !wanted.contains(*key))
            .cloned()
            .collect();
        for key in gone {
            if let Some(id) = self.logs.sessions.remove(&key) {
                self.data.sessions.stop(id);
            }
        }
        for target in &targets {
            let key = target_key(target);
            if !self.logs.sessions.contains_key(&key) {
                if !self.logs.sources.contains(&key) {
                    self.logs.sources.push(key.clone());
                }
                let id = self.data.logs(
                    self.logs.token,
                    self.context.clone(),
                    target.clone(),
                    (1000 / targets.len().max(1)).max(50) as i64,
                );
                self.logs.sessions.insert(key, id);
            }
        }
        let sr = rect(r.left() + 24.0, r.top(), 260.0, 28.0);
        let response = search(ui, sr, &mut self.logs.filter, "Filter lines", "log-filter", t);
        self.icons.paint(
            ui,
            "search",
            rect(sr.left() + 8.0, sr.center().y - 6.5, 13.0, 13.0),
            t.muted,
        );
        if self.focus_search {
            response.request_focus();
            self.focus_search = false;
        }
        if response.has_focus() {
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                if self.logs.filter.is_empty() {
                    response.surrender_focus();
                } else {
                    self.logs.filter.clear();
                }
            }
            if ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::ArrowDown)) {
                response.surrender_focus();
            }
        }
        let mut x = sr.right() + 12.0;
        for (label, value) in [
            ("Timestamps", &mut self.prefs.timestamps),
            ("Wrap lines", &mut self.prefs.wrap_logs),
            ("Format JSON", &mut self.prefs.format_logs),
        ] {
            let w = text_width(ui, label, 12.0, "sans") + 18.0;
            ui.scope(|ui| {
                // The browser's controls fit the 16.8px text line. egui's
                // default 24px minimum pushes these below the filter's center.
                ui.spacing_mut().interact_size.y = 16.8;
                ui.spacing_mut().icon_width = 12.0;
                ui.spacing_mut().icon_width_inner = 8.0;
                ui.place(
                    rect(x + 6.0, r.top() + 5.6, w, 16.8),
                    Checkbox::new(value, RichText::new(label).font(font(12.0, "sans")).color(t.muted)),
                );
            });
            x += w + 12.0;
        }
        let filter = self.logs.filter.to_lowercase();
        let mut visible: Vec<_> = self
            .logs
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| filter.is_empty() || l.text.to_lowercase().contains(&filter))
            .map(|(i, _)| i)
            .collect();
        let clear = rect(r.right() - 82.0, r.top() + 3.0, 58.0, 22.8);
        if button(ui, clear, "clear-logs", "Clear", 12.0, t.text, None, Some(t.strong), t).clicked() {
            self.logs.lines.clear();
            self.logs.selection = None;
            visible.clear();
        }
        let info = format!(
            "{} · {} lines",
            if targets.is_empty() {
                "No running containers".into()
            } else {
                format!(
                    "{} container{}",
                    targets.len(),
                    if targets.len() == 1 { "" } else { "s" }
                )
            },
            visible.len()
        );
        let iw = text_width(ui, &info, 12.0, "sans");
        label(
            ui,
            rect(clear.left() - 12.0 - iw, r.top() + 5.6, iw, 16.8),
            &info,
            12.0,
            "sans",
            t.muted,
        );
        let body = rect(r.left(), r.top() + 36.0, r.width(), r.height() - 36.0);
        line(ui, body.left_top(), body.right_top(), t.line);
        let labels = source_labels(&targets);
        let view = View {
            timestamps: self.prefs.timestamps,
            wrap: self.prefs.wrap_logs,
            formatted: self.prefs.format_logs,
            labels: (targets.len() > 1).then_some(&labels),
        };
        let keyboard_free = self.overlay.is_none()
            && self.delete.is_none()
            && self.label_draft.is_none()
            && !self.shell_focus
            && !ui.ctx().text_edit_focused();
        self.logs.show(ui, body, &visible, &view, keyboard_free, t);
        if !self.logs.follow && !self.logs.selecting && !visible.is_empty() {
            let rr = rect(body.right() - 110.0, body.bottom() - 52.0, 78.0, 28.8);
            if button(
                ui,
                rr,
                "follow-logs",
                "Follow",
                12.0,
                t.text,
                Some(t.raised),
                Some(t.strong),
                t,
            )
            .clicked()
            {
                self.logs.follow = true;
            }
        }
    }
}
fn target_key(t: &LogTarget) -> String {
    format!("{}/{}/{}", t.namespace, t.pod, t.container)
}
fn log_time(s: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Local).format("%H:%M:%S%.3f").to_string())
        .unwrap_or_else(|_| s.into())
}
fn source_label<'a>(sources: &'a [String], labels: &'a HashMap<String, String>, source: usize) -> &'a str {
    let source = sources.get(source).map(String::as_str).unwrap_or_default();
    labels
        .get(source)
        .map(String::as_str)
        .unwrap_or_else(|| source.split('/').nth(1).unwrap_or_default())
}
fn source_labels(targets: &[LogTarget]) -> HashMap<String, String> {
    let pods: HashSet<_> = targets.iter().map(|t| t.pod.as_str()).collect();
    let containers: HashSet<_> = targets.iter().map(|t| t.container.as_str()).collect();
    let mut prefix = String::new();
    if pods.len() > 1 {
        let first = targets.first().unwrap().pod.clone();
        let mut len = first.len();
        for p in &pods {
            len = first
                .bytes()
                .zip(p.bytes())
                .take(len)
                .take_while(|(a, b)| a == b)
                .count();
        }
        if let Some(at) = first[..len].rfind('-') {
            prefix = first[..at + 1].into();
        }
    }
    targets
        .iter()
        .map(|t| {
            let pod = t.pod.strip_prefix(&prefix).unwrap_or(&t.pod);
            let label = if pods.len() > 1 && containers.len() > 1 {
                format!("{pod}/{}", t.container)
            } else if pods.len() > 1 {
                pod.into()
            } else {
                t.container.clone()
            };
            (target_key(t), label)
        })
        .collect()
}
fn level(v: &serde_json::Value) -> Option<Tone> {
    if let Some(n) = v.as_f64() {
        return Some(if n >= 50.0 {
            Tone::Error
        } else if n >= 40.0 {
            Tone::Warn
        } else if n >= 30.0 {
            Tone::Info
        } else {
            Tone::Debug
        });
    }
    match v.as_str()?.trim().to_lowercase().as_str() {
        "fatal" | "panic" | "critical" | "crit" | "emerg" | "alert" | "error" | "err" => Some(Tone::Error),
        "warning" | "warn" => Some(Tone::Warn),
        "notice" | "info" => Some(Tone::Info),
        "debug" | "trace" => Some(Tone::Debug),
        _ => None,
    }
}
fn parse(text: &str) -> Vec<Part> {
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) {
        let level_keys = [
            "level",
            "severity",
            "lvl",
            "levelname",
            "loglevel",
            "log.level",
            "severity_text",
        ];
        let message_keys = ["message", "msg", "log", "event", "text"];
        let time_keys = ["timestamp", "time", "ts", "@timestamp", "datetime", "asctime", "date"];
        let level_entry = map
            .iter()
            .find(|(k, _)| level_keys.contains(&k.to_lowercase().as_str()));
        let message_entry = map
            .iter()
            .find(|(k, _)| message_keys.contains(&k.to_lowercase().as_str()));
        let mut parts = vec![];
        if let Some((_, v)) = level_entry {
            parts.push(Part {
                text: model::value_text(v).to_uppercase(),
                tone: level(v).unwrap_or(Tone::Muted),
            });
            parts.push(Part {
                text: String::new(),
                tone: Tone::Gap(8.0),
            });
        }
        if let Some((_, v)) = message_entry {
            parts.push(Part {
                text: if let Some(s) = v.as_str() {
                    s.into()
                } else {
                    v.to_string()
                },
                tone: Tone::Text,
            });
            parts.push(Part {
                text: String::new(),
                tone: Tone::Gap(12.0),
            });
        }
        for (k, v) in &map {
            if Some(k) == level_entry.map(|(k, _)| k)
                || Some(k) == message_entry.map(|(k, _)| k)
                || time_keys.contains(&k.to_lowercase().as_str())
            {
                continue;
            }
            parts.push(Part {
                text: k.clone(),
                tone: Tone::Key,
            });
            parts.push(Part {
                text: "=".into(),
                tone: Tone::Faint,
            });
            parts.push(Part {
                text: if let Some(s) = v.as_str() {
                    s.into()
                } else {
                    v.to_string()
                },
                tone: if v.is_null() {
                    Tone::Faint
                } else if v.is_string() {
                    Tone::String
                } else if v.is_number() || v.is_boolean() {
                    Tone::Number
                } else {
                    Tone::Muted
                },
            });
            parts.push(Part {
                text: String::new(),
                tone: Tone::Gap(12.0),
            });
        }
        return parts;
    }
    static TEXT_LEVEL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = TEXT_LEVEL.get_or_init(|| {
        regex::Regex::new(r"\b(FATAL|PANIC|CRITICAL|ERROR|ERR|WARNING|WARN|INFO|NOTICE|DEBUG|TRACE)\b").unwrap()
    });
    if let Some(m) = re.find(text) {
        vec![
            Part {
                text: text[..m.start()].into(),
                tone: Tone::Text,
            },
            Part {
                text: m.as_str().into(),
                tone: level(&serde_json::Value::String(m.as_str().into())).unwrap_or(Tone::Text),
            },
            Part {
                text: text[m.end()..].into(),
                tone: Tone::Text,
            },
        ]
    } else {
        vec![Part {
            text: text.into(),
            tone: Tone::Text,
        }]
    }
}
fn log_job(parts: &[Part], filter: &str, t: Tokens, width: f32) -> eframe::egui::text::LayoutJob {
    let mut job = eframe::egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    job.wrap.break_anywhere = true;
    let pattern = (!filter.is_empty()).then(|| {
        regex::RegexBuilder::new(&regex::escape(filter))
            .case_insensitive(true)
            .build()
            .unwrap()
    });
    let mut leading_space = 0.0;
    for part in parts {
        if let Tone::Gap(width) = part.tone {
            leading_space += width;
            continue;
        }
        let color = match part.tone {
            Tone::Text => t.text,
            Tone::Muted => t.muted,
            Tone::Faint | Tone::Debug => t.faint,
            Tone::Key => t.key,
            Tone::String => t.string,
            Tone::Number => t.number,
            Tone::Error => t.failed,
            Tone::Warn => t.progress,
            Tone::Info => t.ok,
            Tone::Gap(_) => unreachable!(),
        };
        let format = TextFormat {
            font_id: font(
                12.0,
                if matches!(part.tone, Tone::Error | Tone::Warn | Tone::Info | Tone::Debug) {
                    "mono-bold"
                } else {
                    "mono"
                },
            ),
            color,
            line_height: Some(19.0),
            ..Default::default()
        };
        let mut append = |text: &str, format: TextFormat| {
            if !text.is_empty() {
                job.append(text, leading_space, format);
                leading_space = 0.0;
            }
        };
        if let Some(re) = &pattern {
            let mut at = 0;
            for m in re.find_iter(&part.text) {
                append(&part.text[at..m.start()], format.clone());
                append(
                    m.as_str(),
                    TextFormat {
                        background: t.progress.gamma_multiply(0.3),
                        ..format.clone()
                    },
                );
                at = m.end();
            }
            append(&part.text[at..], format);
        } else {
            append(&part.text, format);
        }
    }
    job
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_container_logs_exclude_every_other_container() {
        let mut pod = super::super::regression_tests::two_container_pod();
        let mut init = pod.fields["containers"][0].clone();
        init["name"] = "init".into();
        init["init"] = true.into();
        init["state"] = "Completed".into();
        pod.fields["containers"].as_array_mut().unwrap().push(init);
        let pods = [pod];
        let resource = model::resource("Pod");
        for name in ["application", "sidecar", "init"] {
            let selected = targets(&resource, &pods, Some(name));
            assert_eq!(selected.len(), 1);
            assert_eq!(selected[0].container, name);
            assert_eq!(selected[0].pod, "two-containers");
            assert_eq!(selected[0].namespace, "test");
        }
        assert_eq!(targets(&resource, &pods, None).len(), 3);
        assert_eq!(targets(&model::resource("Deployment"), &pods, None).len(), 2);
        assert!(targets(&resource, &pods, Some("missing")).is_empty());
    }
    #[test]
    fn structured_and_plain_logs() {
        let p = parse(r#"{"level":"warn","message":"hello","timestamp":"today","code":42}"#);
        assert_eq!(
            p.iter().map(|p| p.text.as_str()).collect::<String>(),
            "WARNhellocode=42"
        );
        let job = log_job(&p, "hello", Tokens::new(true), f32::INFINITY);
        assert_eq!(job.sections.iter().map(|s| s.leading_space).sum::<f32>(), 20.0);
        let p = parse("a ERROR bad");
        assert!(matches!(p[1].tone, Tone::Error));
    }
    #[test]
    fn select_all_copies_the_shown_columns() {
        let ts = "2026-10-17T10:00:00.123Z";
        let state = State {
            sources: vec!["test/api-1/app".into(), "test/api-2/app".into()],
            lines: vec![
                Line::new(0, Some(ts.into()), r#"{"msg":"first"}"#.into(), false),
                Line::new(1, Some(ts.into()), "second".into(), false),
                Line::new(1, None, "api-2/app: log stream ended".into(), true),
            ],
            ..Default::default()
        };
        assert_eq!(
            state.plain_text(&[0, 2], false, None),
            "{\"msg\":\"first\"}\napi-2/app: log stream ended"
        );
        let labels = HashMap::from([
            ("test/api-1/app".into(), "1".into()),
            ("test/api-2/app".into(), "2".into()),
        ]);
        let time = log_time(ts);
        assert_eq!(
            state.plain_text(&[1, 2], true, Some(&labels)),
            format!("{time} 2 second\n2 api-2/app: log stream ended")
        );
    }
    #[test]
    fn a_selection_copies_what_the_panel_shows() {
        let parts = parse(r#"{"level":"warn","message":"hello","code":42}"#);
        // Shown as "WARN hello code=42", with gaps instead of spaces.
        assert_eq!(slice(&parts, 0..16), "WARN hello code=42");
        assert_eq!(slice(&parts, 2..7), "RN hel");
        assert_eq!(slice(&parts, 4..9), "hello");
        let mut state = State::default();
        for text in ["first line", r#"{"msg":"second"}"#, "third line"] {
            state.push(Line::new(0, None, text.into(), false));
        }
        // Dragged upward, from inside the third line to inside the first.
        let selection = Selection {
            anchor: Point { line: 2, char: 5 },
            head: Point { line: 0, char: 6 },
            formatted: true,
        };
        assert_eq!(selection.range(1, 6), Some(0..6));
        assert_eq!(selection.range(3, 6), None);
        assert_eq!(state.selected_text(&[0, 1, 2], selection), "line\nsecond\nthird");
        // Lines the filter hides are not copied.
        assert_eq!(state.selected_text(&[0, 2], selection), "line\nthird");
        let raw = Selection {
            formatted: false,
            ..selection
        };
        assert_eq!(
            state.selected_text(&[0, 1, 2], raw),
            "line\n{\"msg\":\"second\"}\nthird"
        );
    }
    #[test]
    fn a_drag_copies_the_lines_it_selected_while_logs_stream() {
        let ctx = Context::default();
        install_fonts(&ctx);
        let t = Tokens::new(true);
        t.apply(&ctx, model::Theme::Dark);
        let body = rect(0.0, 0.0, 600.0, 200.0);
        let mut state = State {
            follow: true,
            ..Default::default()
        };
        let stream = |state: &mut State, count: usize| {
            for _ in 0..count {
                let n = state.next_id;
                state.push(Line::new(0, None, format!("line {n:03}"), false));
            }
        };
        let clock = std::cell::Cell::new(0.0);
        let run = |state: &mut State, events: Vec<Event>| {
            clock.set(clock.get() + 1.0 / 60.0);
            let visible: Vec<_> = (0..state.lines.len()).collect();
            let view = View {
                timestamps: false,
                wrap: true,
                formatted: true,
                labels: None,
            };
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(body),
                    time: Some(clock.get()),
                    events,
                    ..Default::default()
                },
                |ui| state.show(ui, body, &visible, &view, true, t),
            );
            output.textures_delta.clear();
            output.platform_output.commands.into_iter().find_map(|c| match c {
                OutputCommand::CopyText(text) => Some(text),
                _ => None,
            })
        };
        let press = |pos: Pos2, pressed: bool| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        stream(&mut state, 100);
        for _ in 0..3 {
            run(&mut state, vec![]);
        }
        // Following, 100 lines of 19 points end at the bottom of the 200-point body,
        // so lines 92 and 94 sit at 57 and 95.
        let (from, to) = (pos2(5.0, 57.0), pos2(590.0, 95.0));
        stream(&mut state, 1);
        run(&mut state, vec![Event::PointerMoved(from), press(from, true)]);
        for step in 1..=4 {
            stream(&mut state, 1);
            run(&mut state, vec![Event::PointerMoved(from.lerp(to, step as f32 / 4.0))]);
        }
        stream(&mut state, 1);
        run(&mut state, vec![press(to, false)]);
        // The selection outlives its lines leaving the screen.
        state.follow = true;
        for _ in 0..5 {
            stream(&mut state, 10);
            run(&mut state, vec![]);
        }
        assert_eq!(
            run(&mut state, vec![Event::Copy]).as_deref(),
            Some("line 092\nline 093\nline 094")
        );
        // A triple click selects the whole line, a double click a word.
        let word = pos2(30.0, 57.0);
        let mut clicks = |count| {
            clock.set(clock.get() + 1.0);
            for _ in 0..count {
                run(&mut state, vec![Event::PointerMoved(word), press(word, true)]);
                run(&mut state, vec![press(word, false)]);
            }
            run(&mut state, vec![Event::Copy]).unwrap_or_default()
        };
        let line = clicks(3);
        assert!(line.starts_with("line ") && line.len() == 8, "{line}");
        assert_eq!(clicks(2), "line");
    }
}
