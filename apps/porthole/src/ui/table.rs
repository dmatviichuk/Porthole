use super::style::*;
use crate::{
    model::{self, Prefs},
    resources::ResourceType,
    summary::{Health, ResourceRow},
};
use eframe::egui::*;
use std::{cmp::Ordering, collections::HashMap};
#[derive(Clone)]
pub struct Column {
    pub id: String,
    pub title: String,
    pub min: f32,
    pub fr: f32,
    pub numeric: bool,
}
impl Column {
    pub fn new(id: &str, title: &str, min: f32, fr: f32, numeric: bool) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            min,
            fr,
            numeric,
        }
    }
}
#[derive(Clone)]
pub enum SortValue {
    Text(String),
    Number(f64),
    None,
}
#[derive(Clone)]
pub struct Cell {
    pub text: String,
    pub sort: SortValue,
    pub health: Option<Health>,
    pub status: bool,
    pub gauge: Option<(Option<f64>, bool)>,
    pub suffix: Option<String>,
}
impl Cell {
    pub fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            sort: SortValue::Text(text.clone()),
            text,
            health: None,
            status: false,
            gauge: None,
            suffix: None,
        }
    }
    pub fn number(text: impl Into<String>, value: Option<f64>) -> Self {
        let mut c = Self::text(text);
        c.sort = value.map(SortValue::Number).unwrap_or(SortValue::None);
        c
    }
    pub fn status(text: impl Into<String>, health: Option<Health>) -> Self {
        let mut c = Self::text(text);
        c.status = true;
        c.health = health;
        c
    }
    pub fn gauge(text: impl Into<String>, value: Option<f64>, fill: Option<f64>, cpu: bool) -> Self {
        let mut c = Self::number(text, value);
        if value.is_some() {
            c.gauge = Some((fill, cpu));
        }
        c
    }
}
#[derive(Clone)]
pub struct Row {
    pub id: String,
    pub resource: Option<ResourceType>,
    pub object: ResourceRow,
    pub container: Option<String>,
    pub cells: HashMap<String, Cell>,
}
impl Row {
    pub fn new(r: ResourceType, object: ResourceRow) -> Self {
        Self {
            id: object.uid.clone(),
            resource: Some(r),
            object,
            container: None,
            cells: HashMap::new(),
        }
    }
    pub fn put(&mut self, id: &str, c: Cell) {
        self.cells.insert(id.into(), c);
    }
}
#[derive(Default)]
pub struct TableState {
    pub sort: String,
    pub desc: bool,
    pub cursor: Option<String>,
    pub keyboard: bool,
    pub search: String,
    pub dragging: Option<String>,
    pub drag_offset: Vec2,
    pub initialized: bool,
    pub reveal: bool,
    pub last_index: usize,
    pub reset_scroll: bool,
    pub scroll_offset: f32,
}
pub enum Event {
    Open(usize),
    Menu(usize, Pos2),
    Logs(usize),
    Yaml(usize),
    Shell(usize),
    Copy(usize),
    ResetColumns,
}
pub struct Options<'a> {
    pub id: &'a str,
    pub primary: bool,
    pub fit: bool,
    pub height: f32,
    pub empty: &'a str,
    pub search: &'a str,
    pub initial_sort: &'a str,
    pub menus: bool,
    pub open: bool,
}
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    r: Rect,
    cols: &[Column],
    rows: &[Row],
    state: &mut TableState,
    prefs: &mut Prefs,
    icons: &mut Icons,
    t: Tokens,
    options: Options<'_>,
) -> Vec<Event> {
    let mut events = vec![];
    let mut drag_header = None;
    let mut drag_rows = Vec::new();
    if state.dragging.is_some() && ui.input(|i| i.key_pressed(Key::Escape)) {
        state.dragging = None;
        ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Escape);
        });
    }
    if !state.initialized {
        state.sort = options.initial_sort.into();
        state.initialized = true;
        state.reset_scroll = true;
    }
    if state.search != options.search {
        state.search = options.search.into();
        state.cursor = None;
        state.reveal = true;
    }
    let ordered_ids = prefs.columns.get(options.id).cloned().unwrap_or_default();
    let mut ordered: Vec<&Column> = ordered_ids
        .iter()
        .filter_map(|id| cols.iter().find(|c| c.id == *id))
        .collect();
    for c in cols {
        if !ordered.iter().any(|o| o.id == c.id) {
            ordered.push(c);
        }
    }
    let mut sorted: Vec<usize> = (0..rows.len()).collect();
    sorted.sort_by(|&a, &b| {
        compare(
            rows[a].cells.get(&state.sort),
            rows[b].cells.get(&state.sort),
            state.desc,
        )
    });
    let cursor = state
        .cursor
        .as_ref()
        .and_then(|id| sorted.iter().position(|&i| rows[i].id == *id))
        .unwrap_or(if state.cursor.is_some() { state.last_index } else { 0 })
        .min(rows.len().saturating_sub(1));
    state.last_index = cursor;
    if !rows.is_empty() {
        state.cursor = Some(rows[sorted[cursor]].id.clone());
    }
    let table_id = ui.id().with(options.id);
    let response = ui.interact(r, table_id, Sense::click());
    if options.primary && ui.ctx().memory(|m| m.focused().is_none()) {
        response.request_focus();
    }
    if response.clicked() {
        response.request_focus();
    }
    let focused = response.has_focus();
    if ui.input(|i| i.pointer.any_pressed()) {
        state.keyboard = false;
    }
    let mut next = cursor;
    let mut keyboard_menu = None;
    let page = ((r.height() - 32.0) / options.height).floor().max(1.0) as usize;
    if focused && !rows.is_empty() {
        let keys = ui.input(|i| i.events.clone());
        for e in keys {
            if let eframe::egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = e
            {
                if modifiers.alt {
                    continue;
                }
                match key {
                    Key::ArrowDown if !modifiers.command => next = (next + 1).min(rows.len() - 1),
                    Key::J if modifiers.is_none() => next = (next + 1).min(rows.len() - 1),
                    Key::ArrowUp if !modifiers.command => next = next.saturating_sub(1),
                    Key::K if modifiers.is_none() => next = next.saturating_sub(1),
                    Key::Home => next = 0,
                    Key::End => next = rows.len() - 1,
                    Key::ArrowUp if modifiers.command => next = 0,
                    Key::ArrowDown if modifiers.command => next = rows.len() - 1,
                    Key::PageDown => next = (next + page).min(rows.len() - 1),
                    Key::PageUp => next = next.saturating_sub(page),
                    Key::Enter if options.open => events.push(Event::Open(sorted[cursor])),
                    Key::M if modifiers.is_none() && options.menus => keyboard_menu = Some(cursor),
                    Key::F10 if modifiers.shift && options.menus => {
                        keyboard_menu = Some(cursor);
                    }
                    Key::L if modifiers.is_none() && options.menus => events.push(Event::Logs(sorted[cursor])),
                    Key::S if modifiers.is_none() && options.menus => events.push(Event::Shell(sorted[cursor])),
                    Key::Y if modifiers.is_none() && options.menus => events.push(Event::Yaml(sorted[cursor])),
                    Key::C if modifiers.is_none() && options.menus => events.push(Event::Copy(sorted[cursor])),
                    _ => continue,
                }
                state.keyboard = true;
                ui.input_mut(|i| {
                    i.consume_key(modifiers, key);
                });
            }
        }
        if next != cursor {
            state.cursor = Some(rows[sorted[next]].id.clone());
            state.reveal = true;
        }
    }
    let min_width = ordered.iter().map(|c| c.min).sum::<f32>() + 48.0;
    let gutter = if !options.fit && rows.len() as f32 * options.height + 32.0 > r.height() {
        12.0
    } else {
        0.0
    };
    let width = (r.width() - gutter).max(min_width);
    let tracks = tracks(&ordered, width - 48.0);
    ui.scope_builder(UiBuilder::new().id_salt(options.id).max_rect(r), |ui| {
        ui.set_clip_rect(r.intersect(ui.clip_rect()));
        ScrollArea::horizontal()
            .id_salt((options.id, "horizontal"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(width);
                let origin = ui.cursor().min;
                let header = rect(origin.x, origin.y, width, 32.0);
                ui.allocate_space(vec2(width, 32.0));
                ui.painter().rect_filled(header, 0, t.bg);
                line(
                    ui,
                    header.left_bottom() - vec2(0.0, 0.5),
                    header.right_bottom() - vec2(0.0, 0.5),
                    t.line,
                );
                let mut x = origin.x + 24.0;
                for (c, &w) in ordered.iter().zip(&tracks) {
                    let hr = rect(x, origin.y, w, 32.0);
                    let response = ui.interact(hr, ui.id().with(("column", &c.id)), Sense::click_and_drag());
                    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &c.title));
                    let is_sort = state.sort == c.id;
                    label(
                        ui,
                        rect(x, origin.y, w - 12.0, 32.0),
                        &c.title,
                        12.0,
                        if is_sort { "semibold" } else { "sans" },
                        if is_sort { t.text } else { t.muted },
                    );
                    if is_sort {
                        let tw = text_width(ui, &c.title, 12.0, "semibold");
                        icons.paint(
                            ui,
                            if state.desc { "chevron-down" } else { "chevron-up" },
                            rect((x + tw + 4.0).min(x + w - 12.0), origin.y + 10.0, 12.0, 12.0),
                            t.text,
                        );
                    }
                    if response.clicked() && !c.title.is_empty() {
                        if is_sort {
                            if state.desc == c.numeric {
                                state.desc = !c.numeric;
                            } else {
                                state.sort.clear();
                                state.desc = false;
                            }
                        } else {
                            state.sort = c.id.clone();
                            state.desc = c.numeric;
                        }
                        state.reveal = true;
                    }
                    if response.drag_started() {
                        state.dragging = Some(c.id.clone());
                        state.drag_offset = ui.input(|i| i.pointer.press_origin().unwrap_or(hr.center())) - hr.min;
                    }
                    if state.dragging.as_ref() == Some(&c.id) {
                        drag_header = Some((hr, *c));
                    }
                    if state.dragging.as_ref().is_some_and(|id| id != &c.id)
                        && hr.contains(ui.input(|i| i.pointer.hover_pos()).unwrap_or(Pos2::ZERO))
                    {
                        let after = ui.input(|i| i.pointer.hover_pos().unwrap_or(Pos2::ZERO).x) > hr.center().x;
                        let marker = if after { hr.right() } else { hr.left() };
                        line(ui, pos2(marker, hr.top()), pos2(marker, hr.bottom()), t.accent);
                        if ui.input(|i| i.pointer.any_released()) {
                            let from = state.dragging.take().unwrap();
                            let mut order: Vec<_> =
                                ordered.iter().map(|c| c.id.clone()).filter(|id| *id != from).collect();
                            let at = order.iter().position(|id| *id == c.id).unwrap_or(0) + usize::from(after);
                            order.insert(at, from);
                            prefs.columns.insert(options.id.into(), order);
                        }
                    }
                    response.context_menu(|ui| {
                        if ui.button("Reset column order").clicked() {
                            events.push(Event::ResetColumns);
                            ui.close();
                        }
                    });
                    x += w;
                }
                if ui.input(|i| i.pointer.any_released()) {
                    state.dragging = None;
                }
                let body_height = (r.height() - 32.0).max(0.0);
                if rows.is_empty() {
                    label(
                        ui,
                        rect(origin.x + 24.0, origin.y + 64.0, width - 48.0, 18.2),
                        options.empty,
                        13.0,
                        "sans",
                        t.muted,
                    );
                    ui.allocate_space(vec2(width, body_height));
                    return;
                }
                let mut render =
                    |ui: &mut Ui, index: usize, events: &mut Vec<Event>, state: &mut TableState, icons: &mut Icons| {
                        let row_index = sorted[index];
                        let row = &rows[row_index];
                        let (rr, _) = ui.allocate_exact_size(vec2(width, options.height), Sense::hover());
                        let rr = rr.translate(vec2(0.0, -0.5));
                        if state.dragging.is_some() && ui.is_rect_visible(rr) {
                            drag_rows.push((rr, row));
                        }
                        if keyboard_menu == Some(index) {
                            events.push(Event::Menu(row_index, rr.left_bottom()));
                        }
                        let response = ui.interact(rr, ui.id().with(("row", &row.id)), Sense::click());
                        response.widget_info(|| {
                            WidgetInfo::labeled(
                                WidgetType::Button,
                                true,
                                row.cells
                                    .get("name")
                                    .map(|c| c.text.as_str())
                                    .unwrap_or(&row.object.name),
                            )
                        });
                        if response.hovered() && options.open {
                            ui.painter().rect_filled(rr, 0, t.hover);
                        }
                        if focused && state.keyboard && state.cursor.as_deref() == Some(&row.id) {
                            ui.painter().rect_filled(rr, 0, t.selected);
                            ui.painter()
                                .rect_filled(rect(rr.left(), rr.top(), 2.0, rr.height()), 0, t.accent);
                        }
                        if response.clicked() {
                            state.cursor = Some(row.id.clone());
                            if options.open {
                                events.push(Event::Open(row_index));
                            }
                            response.request_focus();
                            ui.memory_mut(|m| m.request_focus(table_id));
                        }
                        if response.secondary_clicked() && options.menus {
                            state.cursor = Some(row.id.clone());
                            events.push(Event::Menu(
                                row_index,
                                ui.input(|i| i.pointer.hover_pos()).unwrap_or(rr.left_top()),
                            ));
                        }
                        if state.reveal && index == next {
                            ui.scroll_to_rect(rr, None);
                            state.reveal = false;
                        }
                        let mut x = rr.left() + 24.0;
                        for (c, &w) in ordered.iter().zip(&tracks) {
                            let cr = rect(x, rr.top(), w - 12.0, options.height);
                            line(ui, pos2(x, rr.bottom()), pos2(x + w, rr.bottom()), t.line);
                            if c.id == "actions" {
                                let anchor = rect(cr.left(), cr.center().y - 12.0, 24.0, 24.0);
                                let response = ui.interact(anchor, ui.id().with(("actions", &row.id)), Sense::click());
                                response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Actions"));
                                if response.hovered() {
                                    ui.painter().rect_filled(anchor, 4, t.selected);
                                }
                                icons.paint(
                                    ui,
                                    "more-horizontal",
                                    rect(cr.left() + 4.0, cr.center().y - 8.0, 16.0, 16.0),
                                    if response.hovered() { t.text } else { t.muted },
                                );
                                if response.clicked() {
                                    events.push(Event::Menu(row_index, anchor.left_bottom()));
                                }
                            } else if let Some(cell) = row.cells.get(&c.id) {
                                if cell.status {
                                    status(ui, icons, cr, cell.health, &cell.text, t);
                                } else if let Some((fill, cpu)) = cell.gauge {
                                    gauge(ui, cr, fill, cpu, &cell.text, t);
                                } else {
                                    let width = text_width(ui, &cell.text, 13.0, "sans");
                                    label(
                                        ui,
                                        cr,
                                        &cell.text,
                                        13.0,
                                        "sans",
                                        if cell.text == "—" { t.faint } else { t.text },
                                    );
                                    if let Some(suffix) = &cell.suffix {
                                        label(
                                            ui,
                                            rect(
                                                cr.left() + width + 8.0,
                                                cr.top(),
                                                (cr.width() - width - 8.0).max(0.0),
                                                cr.height(),
                                            ),
                                            suffix,
                                            12.0,
                                            "sans",
                                            t.muted,
                                        );
                                    }
                                }
                            }
                            x += w;
                        }
                    };
                if options.fit && rows.len() <= 1000 {
                    for index in 0..rows.len() {
                        render(ui, index, &mut events, state, icons);
                    }
                } else {
                    let mut scroll = ScrollArea::vertical()
                        .id_salt((options.id, "vertical"))
                        .max_height(body_height)
                        .auto_shrink([false, false]);
                    if state.reset_scroll || state.reveal {
                        let top = next as f32 * options.height;
                        let offset = if state.reset_scroll || top < state.scroll_offset {
                            top
                        } else if top + options.height > state.scroll_offset + body_height {
                            top + options.height - body_height
                        } else {
                            state.scroll_offset
                        };
                        scroll = scroll.vertical_scroll_offset(offset.max(0.0));
                        state.reset_scroll = false;
                        state.reveal = false;
                    }
                    let output = scroll.show_rows(ui, options.height, rows.len(), |ui, range| {
                        for index in range {
                            render(ui, index, &mut events, state, icons);
                        }
                    });
                    state.scroll_offset = output.state.offset.y;
                }
            });
    });
    if state.dragging.is_some()
        && let Some((header, column)) = drag_header
        && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
    {
        let lift = ui
            .ctx()
            .animate_bool_responsive(ui.id().with((options.id, "column-lift")), true);
        let origin = pointer - state.drag_offset + vec2(3.0 * lift, -4.0 * lift);
        let shift = origin - header.min;
        let bottom = drag_rows.last().map_or(header.bottom(), |(r, _)| r.bottom());
        let panel = Rect::from_min_max(
            origin - vec2(8.0, 0.0),
            pos2(origin.x + header.width(), bottom + shift.y),
        );
        let mut ghost = ui.new_child(
            UiBuilder::new()
                .id_salt((options.id, "floating-column"))
                .layer_id(LayerId::new(
                    Order::Tooltip,
                    ui.id().with((options.id, "floating-column")),
                ))
                .max_rect(panel),
        );
        ghost.set_clip_rect(ui.ctx().content_rect());
        ghost.painter().with_clip_rect(ui.ctx().content_rect()).add(
            Shadow {
                offset: [0, 6],
                blur: 20,
                spread: 1,
                color: Color32::from_black_alpha((90.0 * lift) as u8),
            }
            .as_shape(panel, 6),
        );
        ghost.painter().rect_filled(panel, 6, t.raised);
        ghost
            .painter()
            .rect_stroke(panel, 6, Stroke::new(1.0, t.strong), StrokeKind::Inside);
        label(
            &ghost,
            Rect::from_min_size(origin, header.size() - vec2(12.0, 0.0)),
            &column.title,
            12.0,
            "semibold",
            t.text,
        );
        line(
            &ghost,
            pos2(panel.left(), origin.y + 32.0),
            pos2(panel.right(), origin.y + 32.0),
            t.line,
        );
        let mut body_ui = ghost.new_child(UiBuilder::new().id_salt("floating-column-body"));
        body_ui.set_clip_rect(
            Rect::from_min_max(pos2(panel.left(), origin.y + 32.0), panel.max).intersect(ui.ctx().content_rect()),
        );
        for (source, row) in drag_rows {
            let cr = rect(origin.x, source.top() + shift.y, header.width() - 12.0, source.height());
            if let Some(cell) = row.cells.get(&column.id) {
                paint_cell(&body_ui, icons, cr, cell, t);
            }
            line(
                &body_ui,
                pos2(panel.left(), cr.bottom()),
                pos2(panel.right(), cr.bottom()),
                t.line,
            );
        }
        ui.ctx().request_repaint();
    } else {
        ui.ctx()
            .animate_bool_responsive(ui.id().with((options.id, "column-lift")), false);
    }
    events
}
fn paint_cell(ui: &Ui, icons: &mut Icons, cr: Rect, cell: &Cell, t: Tokens) {
    if cell.status {
        status(ui, icons, cr, cell.health, &cell.text, t);
    } else if let Some((fill, cpu)) = cell.gauge {
        gauge(ui, cr, fill, cpu, &cell.text, t);
    } else {
        let width = text_width(ui, &cell.text, 13.0, "sans");
        label(
            ui,
            cr,
            &cell.text,
            13.0,
            "sans",
            if cell.text == "—" { t.faint } else { t.text },
        );
        if let Some(suffix) = &cell.suffix {
            label(
                ui,
                rect(
                    cr.left() + width + 8.0,
                    cr.top(),
                    (cr.width() - width - 8.0).max(0.0),
                    cr.height(),
                ),
                suffix,
                12.0,
                "sans",
                t.muted,
            );
        }
    }
}

pub(super) fn tracks(cols: &[&Column], width: f32) -> Vec<f32> {
    let mut values: Vec<f32> = cols.iter().map(|c| c.min).collect();
    let mut free: Vec<usize> = cols
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (c.fr > 0.0).then_some(i))
        .collect();
    let mut remaining = width - cols.iter().filter(|c| c.fr == 0.0).map(|c| c.min).sum::<f32>();
    loop {
        let total = free.iter().map(|&i| cols[i].fr).sum::<f32>();
        let small: Vec<_> = free
            .iter()
            .copied()
            .filter(|&i| remaining * cols[i].fr / total < cols[i].min)
            .collect();
        if small.is_empty() {
            for &i in &free {
                values[i] = (remaining * cols[i].fr / total).max(cols[i].min);
            }
            break;
        }
        for i in small {
            remaining -= cols[i].min;
            free.retain(|j| *j != i);
        }
        if free.is_empty() {
            break;
        }
    }
    values
}
fn compare(a: Option<&Cell>, b: Option<&Cell>, desc: bool) -> Ordering {
    let a = a.map(|c| &c.sort).unwrap_or(&SortValue::None);
    let b = b.map(|c| &c.sort).unwrap_or(&SortValue::None);
    let order = match (a, b) {
        (SortValue::None, SortValue::None) => Ordering::Equal,
        (SortValue::None, _) => return Ordering::Greater,
        (_, SortValue::None) => return Ordering::Less,
        (SortValue::Number(a), SortValue::Number(b)) => a.total_cmp(b),
        (SortValue::Text(a), SortValue::Text(b)) => natural(a, b),
        _ => Ordering::Equal,
    };
    if desc { order.reverse() } else { order }
}
fn natural(a: &str, b: &str) -> Ordering {
    static PARTS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\d+|\D+").unwrap());
    let re = &*PARTS;
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let mut aa = re.find_iter(&a);
    let mut bb = re.find_iter(&b);
    loop {
        match (aa.next(), bb.next()) {
            (Some(x), Some(y)) => {
                let x = x.as_str();
                let y = y.as_str();
                let cmp = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => x.cmp(y),
                };
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
        }
    }
}
pub fn columns(kind: &str, ns: bool) -> Vec<Column> {
    let name = Column::new("name", "Name", 220.0, 3.0, false);
    let namespace = Column::new("namespace", "Namespace", 120.0, 1.2, false);
    let status = Column::new("status", "Status", 140.0, 1.3, false);
    let age = Column::new("age", "Age", 72.0, 0.0, true);
    let mut c = if kind == "Event" {
        vec![
            Column::new("lastSeen", "Last seen", 100.0, 0.0, true),
            Column::new("reason", "Reason", 130.0, 1.0, false),
            Column::new("object", "Object", 160.0, 1.4, false),
        ]
    } else {
        vec![name]
    };
    if ns {
        c.push(namespace);
    }
    let specs: Vec<(&str, &str, f32, f32, bool)> = match kind {
        "Pod" => vec![
            ("ready", "Ready", 84.0, 0.0, true),
            ("restarts", "Restarts", 84.0, 0.0, true),
            ("status", "Status", 140.0, 1.3, false),
            ("node", "Node", 140.0, 1.4, false),
            ("ip", "IP", 120.0, 0.0, false),
        ],
        "Deployment" => vec![
            ("ready", "Ready", 84.0, 0.0, true),
            ("upToDate", "Up to date", 96.0, 0.0, true),
            ("available", "Available", 90.0, 0.0, true),
            ("status", "Status", 140.0, 1.3, false),
        ],
        "StatefulSet" | "ReplicaSet" => vec![
            ("ready", "Ready", 84.0, 0.0, true),
            ("status", "Status", 140.0, 1.3, false),
        ],
        "DaemonSet" => vec![
            ("desired", "Desired", 80.0, 0.0, true),
            ("ready", "Ready", 84.0, 0.0, true),
            ("upToDate", "Up to date", 96.0, 0.0, true),
            ("available", "Available", 90.0, 0.0, true),
            ("status", "Status", 140.0, 1.3, false),
        ],
        "Job" => vec![
            ("completions", "Completions", 84.0, 0.0, true),
            ("duration", "Duration", 90.0, 0.0, true),
            ("status", "Status", 140.0, 1.3, false),
        ],
        "CronJob" => vec![
            ("schedule", "Schedule", 120.0, 1.0, false),
            ("active", "Active", 72.0, 0.0, true),
            ("lastSchedule", "Last schedule", 100.0, 0.0, true),
            ("status", "Status", 140.0, 1.3, false),
        ],
        "Service" => vec![
            ("type", "Type", 110.0, 0.0, false),
            ("clusterIP", "Cluster IP", 130.0, 0.0, false),
            ("external", "External", 140.0, 1.4, false),
            ("ports", "Ports", 120.0, 1.2, false),
        ],
        "Ingress" => vec![
            ("class", "Class", 110.0, 0.0, false),
            ("hosts", "Hosts", 160.0, 2.0, false),
            ("address", "Address", 140.0, 1.4, false),
        ],
        "ConfigMap" => vec![("keys", "Keys", 72.0, 0.0, true)],
        "Secret" => vec![("type", "Type", 160.0, 1.5, false), ("keys", "Keys", 72.0, 0.0, true)],
        "Namespace" => vec![("status", "Status", 140.0, 1.3, false)],
        "Node" => vec![
            ("status", "Status", 140.0, 1.3, false),
            ("roles", "Roles", 110.0, 1.0, false),
            ("version", "Version", 110.0, 0.0, false),
            ("internalIP", "Internal IP", 130.0, 0.0, false),
            ("instanceType", "Instance type", 130.0, 0.0, false),
        ],
        "PersistentVolumeClaim" => vec![
            ("status", "Status", 140.0, 1.3, false),
            ("capacity", "Capacity", 90.0, 0.0, false),
            ("storageClass", "Storage class", 130.0, 0.0, false),
            ("volume", "Volume", 160.0, 1.5, false),
        ],
        "PersistentVolume" => vec![
            ("status", "Status", 140.0, 1.3, false),
            ("capacity", "Capacity", 90.0, 0.0, false),
            ("claim", "Claim", 160.0, 1.5, false),
            ("storageClass", "Storage class", 130.0, 0.0, false),
            ("reclaimPolicy", "Reclaim", 90.0, 0.0, false),
        ],
        "Event" => vec![
            ("message", "Message", 260.0, 4.0, false),
            ("count", "Count", 64.0, 0.0, true),
        ],
        _ => vec![],
    };
    for (id, title, min, fr, numeric) in specs {
        c.push(Column::new(id, title, min, fr, numeric));
    }
    if kind != "Event" {
        if c.len() == 1 + usize::from(ns) {
            c.push(status);
        }
        c.push(age);
    }
    c
}
pub fn resource_row(r: ResourceType, obj: ResourceRow, cols: &[Column], now: i64) -> Row {
    let mut row = Row::new(r, obj);
    for c in cols {
        let cell = match c.id.as_str() {
            "name" => Cell::text(&row.object.name),
            "namespace" => Cell::text(row.object.namespace.as_deref().unwrap_or_default()),
            "status" | "reason" => Cell::status(
                row.object.status.as_deref().unwrap_or_default(),
                if c.id == "reason" && row.object.health != Some(Health::Failed) {
                    None
                } else {
                    row.object.health
                },
            ),
            "age" => Cell::number(
                model::age(row.object.created, now),
                row.object.created.map(|t| -t as f64),
            ),
            "lastSeen" | "lastSchedule" => Cell::number(
                model::age(row.object.fields.get(&c.id).and_then(|v| v.as_i64()), now),
                row.object.fields.get(&c.id).and_then(|v| v.as_f64()).map(|v| -v),
            ),
            "duration" => Cell::number(
                row.object
                    .fields
                    .get("duration")
                    .and_then(|v| v.as_i64())
                    .map(model::duration)
                    .unwrap_or_default(),
                row.object.fields.get("duration").and_then(|v| v.as_f64()),
            ),
            "ready" | "completions" => {
                let text = model::text(&row.object, &c.id);
                let ratio = text
                    .split_once('/')
                    .and_then(|(a, b)| Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?));
                Cell::number(text, ratio)
            }
            _ => {
                let text = model::text(&row.object, &c.id);
                if c.numeric {
                    Cell::number(text, row.object.fields.get(&c.id).and_then(|v| v.as_f64()))
                } else {
                    Cell::text(text)
                }
            }
        };
        row.put(&c.id, cell);
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_scroll_extent_matches_the_last_section() {
        let ctx = Context::default();
        install_fonts(&ctx);
        Tokens::new(true).apply(&ctx, model::Theme::Dark);
        let mut state = TableState::default();
        let mut prefs = Prefs::default();
        let mut icons = Icons::new();
        let cols = [Column::new("name", "Name", 100.0, 1.0, false)];
        let rows = (0..30)
            .map(|n| {
                let object = super::super::regression_tests::two_container_pod();
                let mut row = Row::new(model::resource("Pod"), object);
                row.id = n.to_string();
                row.put("name", Cell::text(format!("pod-{n}")));
                row
            })
            .collect::<Vec<_>>();
        let mut content_height = 0.0;
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(rect(0.0, 0.0, 1000.0, 700.0)),
                ..Default::default()
            },
            |ui| {
                content_height = ScrollArea::vertical()
                    .show(ui, |ui| {
                        let origin = ui.cursor().min;
                        let height = 32.0 + rows.len() as f32 * 44.0;
                        show(
                            ui,
                            rect(origin.x, origin.y + 60.0, 900.0, height),
                            &cols,
                            &rows,
                            &mut state,
                            &mut prefs,
                            &mut icons,
                            Tokens::new(true),
                            Options {
                                id: "fitting-table",
                                primary: false,
                                fit: true,
                                height: 44.0,
                                empty: "No pods",
                                search: "",
                                initial_sort: "name",
                                menus: false,
                                open: false,
                            },
                        );
                        super::super::views::finish_scroll_content(
                            ui,
                            rect(origin.x, origin.y, 900.0, 60.0 + height + 40.0),
                        );
                    })
                    .content_size
                    .y;
            },
        );
        output.textures_delta.clear();
        assert!(
            (content_height - 1452.0).abs() < 1.0,
            "content height was {content_height}"
        );
    }

    #[test]
    fn dragging_a_column_lifts_it_and_persists_the_drop_order() {
        let ctx = Context::default();
        install_fonts(&ctx);
        let t = Tokens::new(true);
        t.apply(&ctx, model::Theme::Dark);
        let mut state = TableState::default();
        let mut prefs = Prefs::default();
        let mut icons = Icons::new();
        let cols = [
            Column::new("name", "Name", 100.0, 1.0, false),
            Column::new("kind", "Kind", 100.0, 1.0, false),
            Column::new("age", "Age", 100.0, 1.0, true),
        ];
        let mut frame = 0;
        let mut run = |events: Vec<eframe::egui::Event>| {
            frame += 1;
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(rect(0.0, 0.0, 600.0, 300.0)),
                    time: Some(frame as f64 / 60.0),
                    events,
                    ..Default::default()
                },
                |ui| {
                    show(
                        ui,
                        rect(0.0, 0.0, 600.0, 200.0),
                        &cols,
                        &[],
                        &mut state,
                        &mut prefs,
                        &mut icons,
                        t,
                        Options {
                            id: "drag-test",
                            primary: false,
                            fit: true,
                            height: 36.0,
                            empty: "No resources",
                            search: "",
                            initial_sort: "name",
                            menus: false,
                            open: false,
                        },
                    );
                },
            );
            output.textures_delta.clear();
            let floating = output.shapes.iter().find_map(|shape| match &shape.shape {
                epaint::Shape::Rect(r) if r.fill == t.raised && r.corner_radius == CornerRadius::same(6) => {
                    Some(r.rect)
                }
                _ => None,
            });
            (
                floating,
                state.dragging.clone(),
                prefs.columns.get("drag-test").cloned(),
            )
        };
        let pointer = |x, pressed| eframe::egui::Event::PointerButton {
            pos: pos2(x, 16.0),
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        run(vec![]);
        run(vec![
            eframe::egui::Event::PointerMoved(pos2(40.0, 16.0)),
            pointer(40.0, true),
        ]);
        let (floating, dragging, order) = run(vec![eframe::egui::Event::PointerMoved(pos2(300.0, 16.0))]);
        assert_eq!(dragging.as_deref(), Some("name"));
        assert!(order.is_none());
        let first = floating.expect("the held column should paint a raised preview");
        let (floating, _, _) = run(vec![eframe::egui::Event::PointerMoved(pos2(520.0, 16.0))]);
        assert!(floating.unwrap().left() > first.left() + 200.0);
        let (floating, dragging, order) = run(vec![pointer(520.0, false)]);
        assert!(floating.is_none() && dragging.is_none());
        assert_eq!(order.unwrap(), ["kind", "age", "name"]);
    }
}
