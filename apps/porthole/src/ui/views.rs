use super::{
    Action, App,
    style::*,
    table::{self, Cell, Column, Row},
};
use crate::{
    data::Snapshot,
    model::{self, Amounts, Tab, View},
    resources::{ResourceInfo, ResourceType},
    summary::{Health, ResourceRow},
};
use eframe::egui::*;
use std::collections::HashSet;
pub fn placeholder(ui: &Ui, r: Rect, text: &str, t: Tokens) {
    let mut job =
        eframe::egui::text::LayoutJob::simple(text.into(), font(13.0, "sans"), t.muted, (r.width() - 64.0).max(50.0));
    job.halign = Align::Center;
    let g = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0),
        g,
        t.muted,
    );
}
pub fn wrap(ui: &Ui, r: Rect, text: &str, size: f32, family: &str, color: Color32) -> f32 {
    let g = ui.painter().layout_job(eframe::egui::text::LayoutJob::simple(
        text.into(),
        font(size, family),
        color,
        r.width(),
    ));
    let height = g.size().y;
    ui.painter().galley(r.min, g, color);
    height
}
fn page_scroll(ui: &mut Ui, r: Rect, name: &str) -> ScrollArea {
    let focus = ui.interact(r, ui.id().with((name, "focus")), Sense::click());
    if focus.clicked() || ui.memory(|m| m.focused().is_none()) {
        focus.request_focus();
    }
    let mut scroll = ScrollArea::vertical().id_salt(name).auto_shrink([false, false]);
    if focus.has_focus() {
        let old = scroll_area::State::load(ui.ctx(), ui.make_persistent_id(name))
            .unwrap_or_default()
            .offset
            .y;
        let offset = ui.input_mut(|i| {
            for (key, delta) in [
                (Key::ArrowDown, 36.0),
                (Key::ArrowUp, -36.0),
                (Key::PageDown, r.height() * 0.9),
                (Key::PageUp, -r.height() * 0.9),
            ] {
                if i.consume_key(Modifiers::NONE, key) {
                    return Some((old + delta).max(0.0));
                }
            }
            if i.consume_key(Modifiers::NONE, Key::Home) || i.consume_key(Modifiers::COMMAND, Key::ArrowUp) {
                Some(0.0)
            } else if i.consume_key(Modifiers::NONE, Key::End) || i.consume_key(Modifiers::COMMAND, Key::ArrowDown) {
                Some(1e9)
            } else {
                None
            }
        });
        if let Some(offset) = offset {
            scroll = scroll.vertical_scroll_offset(offset);
        }
    }
    scroll
}
pub(super) fn finish_scroll_content(ui: &mut Ui, bounds: Rect) {
    // These pages paint sections at absolute positions. set_min_height reserves
    // space after the current cursor in egui 0.36, counting earlier tables twice.
    ui.expand_to_include_rect(bounds);
}
#[derive(Default, Clone)]
struct Totals {
    allocatable: Amounts,
    usage: Option<Amounts>,
    requests: Amounts,
    limits: Amounts,
    pods: usize,
    pod_capacity: f64,
    nodes: Vec<NodeTotals>,
}
#[derive(Clone)]
struct NodeTotals {
    row: ResourceRow,
    allocatable: Amounts,
    usage: Option<Amounts>,
    requests: Amounts,
    limits: Amounts,
    pods: usize,
    pod_capacity: f64,
}
impl App {
    pub(super) fn current_view(&mut self, ui: &mut Ui, r: Rect) {
        let view = self.view();
        let key = format!("{}:{view:?}", self.context);
        if key != self.page_key {
            for id in self.logs.sessions.values() {
                self.data.sessions.stop(*id);
            }
            self.logs = super::logs::State::default();
            self.yaml = super::yaml::State::default();
            self.page_key = key;
            self.expanded.clear();
            self.menu = None;
            ui.memory_mut(|m| {
                m.stop_text_input();
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            for state in self.tables.values_mut() {
                state.reset_scroll = true;
                state.keyboard = false;
            }
        }
        if self.context.is_empty() {
            placeholder(
                ui,
                r,
                if view == View::Overview {
                    "Choose a cluster to see its overview."
                } else {
                    "Choose a cluster to browse."
                },
                self.t,
            );
            return;
        }
        match view {
            View::Applications => self.applications(ui, r),
            View::AllResources => self.resources(ui, r, None),
            View::Resources(resource) => self.resources(ui, r, Some(resource)),
            View::Overview => self.overview(ui, r),
            View::Resource {
                resource,
                namespace,
                object,
                tab,
                log_container,
            } => self.resource_page(ui, r, resource, namespace, object, tab, log_container),
        }
    }
    pub(super) fn banner(&mut self, ui: &mut Ui, r: Rect, text: &str, warn: bool) -> f32 {
        let t = self.t;
        let body = rect(r.left() + 24.0, r.top(), r.width() - 48.0, 100.0);
        let height = ui
            .painter()
            .layout_job(eframe::egui::text::LayoutJob::simple(
                text.into(),
                font(13.0, "sans"),
                t.text,
                body.width() - 50.0,
            ))
            .size()
            .y
            .max(18.2)
            + 16.0;
        let b = rect(body.left(), body.top(), body.width(), height);
        let tone = if warn { t.progress } else { t.failed };
        ui.painter()
            .rect_filled(b, 6, tone.gamma_multiply(if warn { 0.10 } else { 0.08 }));
        ui.painter()
            .rect_stroke(b, 6, Stroke::new(1.0, tone.gamma_multiply(0.3)), StrokeKind::Inside);
        self.icons.paint(
            ui,
            "triangle-alert",
            rect(b.left() + 12.0, b.top() + 10.0, 14.0, 14.0),
            tone,
        );
        wrap(
            ui,
            rect(b.left() + 34.0, b.top() + 8.0, b.width() - 46.0, height - 16.0),
            text,
            13.0,
            "sans",
            t.text,
        );
        height + 8.0
    }
    fn metrics_warning(&mut self, ui: &mut Ui, r: Rect, error: Option<String>) -> f32 {
        let Some(e) = error else { return 0.0 };
        let lower = e.to_lowercase();
        let text = if e.contains("could not find the requested resource") || e.contains("NotFound") || e.contains("404")
        {
            "metrics-server isn't installed on this cluster, so CPU and memory usage can't be shown. Requests and limits are still shown.".into()
        } else if lower.contains("forbidden") || lower.contains("cannot list") || lower.contains("cannot get") {
            "Your account can't read metrics (metrics.k8s.io) on this cluster, so CPU and memory usage can't be shown. Requests and limits are still shown.".into()
        } else {
            format!("metrics-server isn't responding, so CPU and memory usage can't be shown right now: {e}")
        };
        self.banner(ui, r, &text, true)
    }
    pub(super) fn snapshot(&mut self, kind: &str, ns: Option<&str>) -> Snapshot {
        self.data.rows(&self.context, &model::resource(kind), ns, None, false)
    }
    fn pod_sample(&self, pod: &ResourceRow) -> Option<&crate::metrics::PodUsage> {
        self.data
            .context_data
            .get(&self.context)?
            .pods
            .as_ref()?
            .iter()
            .find(|s| Some(s.namespace.as_str()) == pod.namespace.as_deref() && s.name == pod.name)
    }
    fn used(&self, pods: &[ResourceRow]) -> Option<Amounts> {
        let mut total = Amounts::default();
        let mut found = false;
        for p in pods {
            if let Some(sample) = self.pod_sample(p) {
                total.add(Amounts {
                    cpu: sample.cpu,
                    memory: sample.memory,
                });
                found = true;
            }
        }
        found.then_some(total)
    }
    fn applications(&mut self, ui: &mut Ui, r: Rect) {
        let ns = self.namespace.clone();
        let mut snapshots = vec![self.snapshot("Pod", ns.as_deref())];
        let mut workloads = vec![];
        for kind in model::WORKLOADS {
            let s = self.snapshot(kind, ns.as_deref());
            workloads.push((model::resource(kind), s.rows.clone()));
            snapshots.push(s);
        }
        let apps = model::applications(workloads, snapshots[0].rows.clone());
        let loading = snapshots.iter().any(|s| !s.synced);
        let error = snapshots.iter().find_map(|s| s.error.clone());
        let metrics_error = self
            .data
            .context_data
            .get(&self.context)
            .and_then(|d| d.pod_error.clone());
        let h = if let Some(error) = error {
            self.banner(ui, r, &error, false)
        } else {
            self.metrics_warning(ui, r, metrics_error)
        };
        let mut cols = vec![
            Column::new("name", "Name", 220.0, 3.0, false),
            Column::new("kind", "Kind", 100.0, 1.0, false),
        ];
        if ns.is_none() {
            cols.push(Column::new("namespace", "Namespace", 120.0, 1.2, false));
        }
        cols.extend([
            Column::new("pods", "Pods", 84.0, 0.0, true),
            Column::new("cpu", "CPU Usage", 120.0, 0.0, true),
            Column::new("memory", "Mem Usage", 120.0, 0.0, true),
            Column::new("status", "Status", 160.0, 1.6, false),
            Column::new("age", "Age", 72.0, 0.0, true),
        ]);
        let now = chrono::Local::now().timestamp();
        let mut rows = vec![];
        for app in apps {
            if !model::matches(
                &self.search,
                &format!(
                    "{} {} {} {}",
                    app.row.name,
                    app.resource.kind,
                    app.row.namespace.as_deref().unwrap_or_default(),
                    app.row.status.as_deref().unwrap_or_default()
                ),
            ) {
                continue;
            }
            let used = self.used(&app.pods);
            let active: Vec<_> = app.pods.iter().filter(|p| model::active(p)).collect();
            let mut req = Amounts::default();
            let mut lim = Amounts::default();
            for p in active {
                req.add(model::requests(p));
                lim.add(model::limits(p));
            }
            let mut row = table::resource_row(app.resource.clone(), app.row, &cols, now);
            row.id = model::app_key(&app.resource.kind, row.object.namespace.as_deref(), &row.object.name);
            row.put("kind", Cell::text(&app.resource.kind));
            row.put("pods", Cell::number(app.pods_label, Some(app.ratio)));
            for (cpu, id) in [(true, "cpu"), (false, "memory")] {
                let value = used.map(|u| u.get(cpu));
                let reference = if lim.get(cpu) > 0.0 { lim.get(cpu) } else { req.get(cpu) };
                row.put(
                    id,
                    Cell::gauge(
                        value.map(|v| model::amount(cpu, v)).unwrap_or_else(|| "—".into()),
                        value,
                        value.and_then(|v| (reference > 0.0).then(|| model::percent(v, reference))),
                        cpu,
                    ),
                );
            }
            rows.push(row);
        }
        let empty = if loading {
            "Loading applications…".into()
        } else if !self.search.is_empty() {
            format!("No applications match \"{}\".", self.search)
        } else if let Some(ns) = ns {
            format!("No applications in {ns}.")
        } else {
            "No applications in this cluster.".into()
        };
        self.show_table(
            ui,
            rect(r.left(), r.top() + h, r.width(), r.height() - h),
            "applications",
            &cols,
            &rows,
            true,
            false,
            36.0,
            &empty,
            true,
            true,
        );
    }
    fn resources(&mut self, ui: &mut Ui, r: Rect, resource: Option<ResourceType>) {
        let t = self.t;
        let nav = rect(r.left(), r.top(), 210.0, r.height());
        line(ui, nav.right_top(), nav.right_bottom(), t.line);
        let types = self
            .data
            .context_data
            .get(&self.context)
            .map(|d| d.types.clone())
            .unwrap_or_default();
        let groups = groups(&types, &self.type_query);
        let query_rect = rect(nav.left() + 12.0, nav.top() + 31.4, 186.0, 28.0);
        label(
            ui,
            rect(nav.left() + 16.0, nav.top() + 4.0, 160.0, 15.4),
            "Resource types",
            11.0,
            "medium",
            t.muted,
        );
        let refresh = ui.interact(
            rect(nav.right() - 32.0, nav.top(), 20.0, 24.0),
            ui.id().with("rediscover"),
            Sense::click(),
        );
        self.icons.paint(
            ui,
            "rotate-cw",
            rect(nav.right() - 28.0, nav.top() + 6.0, 12.0, 12.0),
            t.muted,
        );
        if refresh.clicked() {
            self.actions.push(Action::Rediscover);
        }
        let response = search(ui, query_rect, &mut self.type_query, "Find a type", "type-search", t);
        self.icons.paint(
            ui,
            "search",
            rect(query_rect.left() + 8.0, query_rect.top() + 8.0, 12.0, 12.0),
            t.muted,
        );
        if response.has_focus() {
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.type_query.clear();
            }
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                if model::matches(&self.type_query, "All types") {
                    self.actions.push(Action::Replace(View::AllResources));
                    response.surrender_focus();
                } else if let Some((_, items)) = groups.first()
                    && let Some(first) = items.first()
                {
                    self.actions
                        .push(Action::Replace(View::Resources(first.resource.clone())));
                    response.surrender_focus();
                }
            }
            if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                response.surrender_focus();
            }
        }
        let list = rect(
            nav.left(),
            query_rect.bottom() + 8.0,
            nav.width(),
            nav.height() - query_rect.height() - 47.4,
        );
        ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
            ui.set_clip_rect(list);
            ScrollArea::vertical()
                .id_salt("types")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let error = self
                        .data
                        .context_data
                        .get(&self.context)
                        .and_then(|d| d.discovery_error.clone());
                    if let Some(e) = error {
                        ui.label(RichText::new(e).color(t.failed));
                    }
                    if groups.is_empty()
                        && !self.type_query.is_empty()
                        && !model::matches(&self.type_query, "All types")
                    {
                        ui.label(format!("No types match \"{}\".", self.type_query));
                    }
                    if model::matches(&self.type_query, "All types") {
                        let o = ui.cursor().min;
                        let rr = rect(o.x + 8.0, o.y, 194.0, 24.2);
                        ui.allocate_space(vec2(210.0, 32.2));
                        let active = resource.is_none();
                        let response = ui.interact(rr, ui.id().with("all-types"), Sense::click());
                        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "All types"));
                        if active || response.hovered() {
                            ui.painter()
                                .rect_filled(rr, 6, if active { t.selected } else { t.hover });
                        }
                        label(
                            ui,
                            rr.shrink2(vec2(8.0, 0.0)),
                            "All types",
                            13.0,
                            if active { "medium" } else { "sans" },
                            t.text,
                        );
                        if response.clicked() {
                            self.actions.push(Action::Replace(View::AllResources));
                        }
                    }
                    for (title, items) in groups {
                        let o = ui.cursor().min;
                        label(ui, rect(o.x + 16.0, o.y, 178.0, 15.4), &title, 11.0, "sans", t.faint);
                        ui.allocate_space(vec2(210.0, 19.4));
                        for info in items {
                            let o = ui.cursor().min;
                            let rr = rect(o.x + 8.0, o.y, 194.0, 24.2);
                            ui.allocate_space(vec2(210.0, 24.2));
                            let active = resource
                                .as_ref()
                                .is_some_and(|r| model::type_key(&info.resource) == model::type_key(r));
                            let response =
                                ui.interact(rr, ui.id().with(model::type_key(&info.resource)), Sense::click());
                            if active || response.hovered() {
                                ui.painter()
                                    .rect_filled(rr, 6, if active { t.selected } else { t.hover });
                            }
                            label(
                                ui,
                                rect(rr.left() + 8.0, rr.top(), 178.0, 24.2),
                                &model::plural(&info.resource.kind),
                                13.0,
                                if active { "medium" } else { "sans" },
                                t.text,
                            );
                            if response.clicked() {
                                self.actions.push(Action::Replace(View::Resources(info.resource)));
                            }
                        }
                        ui.allocate_space(vec2(210.0, 12.0));
                    }
                    ui.allocate_space(vec2(210.0, 16.0));
                });
        });
        let area = rect(nav.right(), r.top(), r.width() - 210.0, r.height());
        let Some(resource) = resource else {
            self.all_resources(ui, area, &types);
            return;
        };
        let ns = self.namespace.clone();
        let snapshot = self.data.rows(&self.context, &resource, ns.as_deref(), None, false);
        let columns = table::columns(&resource.kind, resource.namespaced && ns.is_none());
        let rows: Vec<_> = snapshot
            .rows
            .into_iter()
            .filter(|row| {
                model::matches(
                    &self.search,
                    &format!(
                        "{} {} {} {}",
                        row.name,
                        row.namespace.as_deref().unwrap_or_default(),
                        row.status.as_deref().unwrap_or_default(),
                        model::text(row, "message")
                    ),
                )
            })
            .map(|row| table::resource_row(resource.clone(), row, &columns, chrono::Local::now().timestamp()))
            .collect();
        let title = model::plural(&resource.kind);
        let title_text = format!(
            "{title} {}",
            if snapshot.synced {
                rows.len().to_string()
            } else {
                String::new()
            }
        );
        label(
            ui,
            rect(area.left() + 24.0, area.top(), area.width() - 48.0, 21.0),
            &title_text,
            15.0,
            "semibold",
            t.text,
        );
        let mut h = 29.0;
        if let Some(e) = snapshot.error {
            h += self.banner(
                ui,
                rect(area.left(), area.top() + h, area.width(), area.height() - h),
                &e,
                false,
            );
        }
        let empty = if !snapshot.synced {
            format!("Loading {}…", title.to_lowercase())
        } else if !self.search.is_empty() {
            format!("No {} match \"{}\".", title.to_lowercase(), self.search)
        } else {
            format!(
                "No {}{}.",
                title.to_lowercase(),
                if resource.namespaced {
                    ns.map(|ns| format!(" in {ns}")).unwrap_or_default()
                } else {
                    String::new()
                }
            )
        };
        self.show_table(
            ui,
            rect(area.left(), area.top() + h, area.width(), area.height() - h),
            &format!("resources.{}", model::type_key(&resource)),
            &columns,
            &rows,
            true,
            false,
            36.0,
            &empty,
            true,
            true,
        );
    }
    fn all_resources(&mut self, ui: &mut Ui, area: Rect, types: &[ResourceInfo]) {
        let ns = self.namespace.clone();
        let mut columns = vec![
            Column::new("name", "Name", 220.0, 3.0, false),
            Column::new("kind", "Kind", 160.0, 1.4, false),
        ];
        if ns.is_none() {
            columns.push(Column::new("namespace", "Namespace", 120.0, 1.2, false));
        }
        columns.extend([
            Column::new("status", "Status", 160.0, 1.6, false),
            Column::new("age", "Age", 72.0, 0.0, true),
        ]);
        let now = chrono::Local::now().timestamp();
        let mut rows = Vec::new();
        let mut loading = types.is_empty();
        let mut error = self
            .data
            .context_data
            .get(&self.context)
            .and_then(|d| d.discovery_error.clone());
        for info in types.iter().filter(|info| ns.is_none() || info.resource.namespaced) {
            let resource = &info.resource;
            let snapshot = self.data.rows(&self.context, resource, ns.as_deref(), None, false);
            loading |= !snapshot.synced && snapshot.error.is_none();
            if let Some(message) = snapshot.error {
                error.get_or_insert_with(|| format!("Unable to load {}: {message}", model::plural(&resource.kind)));
            }
            for object in snapshot.rows {
                if !model::matches(
                    &self.search,
                    &format!(
                        "{} {} {} {} {}",
                        object.name,
                        resource.kind,
                        resource.group,
                        object.namespace.as_deref().unwrap_or_default(),
                        object.status.as_deref().unwrap_or_default()
                    ),
                ) {
                    continue;
                }
                let mut row = table::resource_row(resource.clone(), object, &columns, now);
                // UIDs alone can collide when the same object is served by more than one API group.
                row.id = format!("{}:{}", model::type_key(resource), row.id);
                row.put("kind", Cell::text(&resource.kind));
                rows.push(row);
            }
        }
        label(
            ui,
            rect(area.left() + 24.0, area.top(), area.width() - 48.0, 21.0),
            &format!("All resources {}", rows.len()),
            15.0,
            "semibold",
            self.t.text,
        );
        let mut h = 29.0;
        if let Some(error) = error {
            h += self.banner(
                ui,
                rect(area.left(), area.top() + h, area.width(), area.height() - h),
                &error,
                false,
            );
        }
        let empty = if loading {
            "Loading resources…".into()
        } else if !self.search.is_empty() {
            format!("No resources match \"{}\".", self.search)
        } else if let Some(ns) = ns {
            format!("No resources in {ns}.")
        } else {
            "No resources in this cluster.".into()
        };
        self.show_table(
            ui,
            rect(area.left(), area.top() + h, area.width(), area.height() - h),
            "resources.all",
            &columns,
            &rows,
            true,
            false,
            36.0,
            &empty,
            true,
            true,
        );
    }
    fn totals(&self, nodes: Vec<ResourceRow>, pods: &[ResourceRow]) -> Totals {
        let samples = self.data.context_data.get(&self.context).and_then(|d| d.nodes.as_ref());
        let mut totals = Totals::default();
        for row in nodes {
            let allocatable = Amounts {
                cpu: model::num(&row, "cpu"),
                memory: model::num(&row, "memory"),
            };
            let usage = samples
                .and_then(|ss| ss.iter().find(|s| s.name == row.name))
                .map(|s| Amounts {
                    cpu: s.cpu,
                    memory: s.memory,
                });
            let mut node = NodeTotals {
                pod_capacity: model::num(&row, "podCapacity"),
                row,
                allocatable,
                usage,
                requests: Amounts::default(),
                limits: Amounts::default(),
                pods: 0,
            };
            for p in pods {
                if model::active(p) && model::text(p, "node") == node.row.name {
                    node.pods += 1;
                    node.requests.add(model::requests(p));
                    node.limits.add(model::limits(p));
                }
            }
            totals.allocatable.add(node.allocatable);
            totals.requests.add(node.requests);
            totals.limits.add(node.limits);
            totals.pod_capacity += node.pod_capacity;
            if let Some(u) = node.usage {
                totals.usage.get_or_insert_default().add(u);
            }
            totals.nodes.push(node);
        }
        totals.pods = pods.iter().filter(|p| model::active(p)).count();
        totals
    }
    fn basis_select(&mut self, ui: &mut Ui, r: Rect, id: &str, available: bool) -> String {
        let chosen = self.basis.entry(id.into()).or_insert_with(|| "usage".into());
        if !available && chosen == "usage" {
            *chosen = "requests".into();
        }
        let next = chosen.clone();
        let t = self.t;
        let select = rect(r.right() - 89.0, r.top(), 89.0, 20.2);
        let label_width = text_width(ui, "Show CPU & Memory", 13.0, "sans");
        label(
            ui,
            rect(select.left() - 12.0 - label_width, r.top(), label_width, 20.2),
            "Show CPU & Memory",
            13.0,
            "sans",
            t.muted,
        );
        ui.painter().rect_filled(select, 6, t.raised);
        ui.painter()
            .rect_stroke(select, 6, Stroke::new(1.0, t.strong), StrokeKind::Inside);
        let title = match next.as_str() {
            "requests" => "Requests",
            "limits" => "Limits",
            _ => "Usage",
        };
        label(
            ui,
            rect(
                select.left() + 8.0,
                select.top(),
                select.width() - 28.0,
                select.height(),
            ),
            title,
            13.0,
            "sans",
            t.text,
        );
        self.icons.paint(
            ui,
            "chevrons-up-down",
            rect(select.right() - 15.0, select.center().y - 6.5, 13.0, 13.0),
            t.text,
        );
        let response = ui.interact(select, ui.id().with((id, "basis")), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::ComboBox, true, title));
        if response.clicked() {
            let items = [("usage", "Usage"), ("requests", "Requests"), ("limits", "Limits")]
                .into_iter()
                .map(|(value, title)| {
                    let mut item = super::MenuItem::action(title, Action::Basis(id.into(), value.into()));
                    item.enabled = value != "usage" || available;
                    item.checked = Some(value == next);
                    item
                })
                .collect();
            self.actions.push(Action::Menu(items, select.left_bottom()));
        }
        *self.basis.get_mut(id).unwrap() = next.clone();
        next
    }
    fn capacity(&mut self, ui: &mut Ui, r: Rect, totals: &Totals) {
        let t = self.t;
        ui.painter().rect_filled(r, 8, t.raised);
        let width = (r.width() - 80.0) / 2.0;
        for (i, cpu) in [true, false].into_iter().enumerate() {
            let x = r.left() + 24.0 + i as f32 * (width + 32.0);
            let title = if cpu { "CPU" } else { "Memory" };
            label(
                ui,
                rect(x, r.top() + 20.0, width, 18.2),
                title,
                13.0,
                "semibold",
                t.text,
            );
            let whole = totals.allocatable.get(cpu);
            let value = model::amount(cpu, whole);
            let w = text_width(ui, &value, 13.0, "semibold");
            label(
                ui,
                rect(x + width - w, r.top() + 20.0, w, 18.2),
                &value,
                13.0,
                "semibold",
                t.text,
            );
            for (j, (name, value)) in [
                ("Usage", totals.usage.map(|a| a.get(cpu))),
                ("Requests", Some(totals.requests.get(cpu))),
                ("Limits", Some(totals.limits.get(cpu))),
            ]
            .into_iter()
            .enumerate()
            {
                let y = r.top() + 50.2 + j as f32 * 42.2;
                label(ui, rect(x, y, width, 18.2), name, 13.0, "sans", t.muted);
                let text = value
                    .map(|v| format!("{} · {:.0}%", model::amount(cpu, v), model::percent(v, whole)))
                    .unwrap_or_else(|| "—".into());
                let w = text_width(ui, &text, 13.0, "sans");
                label(ui, rect(x + width - w, y, w, 18.2), &text, 13.0, "sans", t.text);
                bar(
                    ui,
                    rect(x, y + 24.2, width, 6.0),
                    value.map(|v| model::percent(v, whole)).unwrap_or(0.0),
                    Some(cpu),
                    t,
                );
            }
        }
    }
    fn overview(&mut self, ui: &mut Ui, r: Rect) {
        let nodes = self.snapshot("Node", None);
        let pods = self.snapshot("Pod", None);
        let totals = self.totals(nodes.rows.clone(), &pods.rows);
        let t = self.t;
        let error = self
            .data
            .context_data
            .get(&self.context)
            .and_then(|d| d.node_error.clone());
        let version = self
            .data
            .context_data
            .get(&self.context)
            .and_then(|d| d.version.clone())
            .unwrap_or_else(|| "…".into());
        ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
            ui.set_clip_rect(r);
            page_scroll(ui, r, "overview-page").show(ui, |ui| {
                let o = ui.cursor().min;
                let width = ui.available_width() - 64.0;
                let mut y = o.y + 8.0;
                let banner = rect(o.x, y, r.width(), 200.0);
                if let Some(e) = nodes.error.as_ref().or(pods.error.as_ref()) {
                    y += self.banner(ui, banner, e, false);
                } else {
                    y += self.metrics_warning(ui, banner, error.clone());
                }
                let two = r.width() + self.prefs.sidebar_width >= 1280.0;
                let card_w = if two { (width - 20.0) / 2.0 } else { width };
                label(
                    ui,
                    rect(o.x + 32.0, y, card_w, 21.0),
                    "Cluster",
                    15.0,
                    "semibold",
                    t.text,
                );
                let card = rect(o.x + 32.0, y + 33.0, card_w, 184.8);
                ui.painter().rect_filled(card, 8, t.raised);
                for (i, (name, value)) in [
                    ("Name", self.context.clone()),
                    ("Version", version),
                    (
                        "Nodes",
                        if nodes.synced {
                            totals.nodes.len().to_string()
                        } else {
                            "…".into()
                        },
                    ),
                    (
                        "Pods",
                        format!(
                            "{}/{:.0} · {:.0}%",
                            totals.pods,
                            totals.pod_capacity,
                            model::percent(totals.pods as f64, totals.pod_capacity)
                        ),
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let yy = card.top() + 20.0 + i as f32 * 28.2;
                    label(
                        ui,
                        rect(card.left() + 24.0, yy, 110.0, 18.2),
                        name,
                        13.0,
                        "sans",
                        t.muted,
                    );
                    label(
                        ui,
                        rect(card.left() + 158.0, yy, card.width() - 182.0, 18.2),
                        &value,
                        13.0,
                        "sans",
                        t.text,
                    );
                    if name == "Pods" {
                        let tw = text_width(ui, &value, 13.0, "sans");
                        bar(
                            ui,
                            rect(
                                card.left() + 174.0 + tw,
                                yy + 6.1,
                                (card.width() - 198.0 - tw).clamp(0.0, 176.0),
                                6.0,
                            ),
                            model::percent(totals.pods as f64, totals.pod_capacity),
                            None,
                            t,
                        );
                    }
                }
                let util_x = if two { card.right() + 20.0 } else { card.left() };
                let util_y = if two { y } else { card.bottom() + 20.0 };
                label(
                    ui,
                    rect(util_x, util_y, card_w, 21.0),
                    "Cluster Utilization",
                    15.0,
                    "semibold",
                    t.text,
                );
                self.capacity(ui, rect(util_x, util_y + 33.0, card_w, 184.8), &totals);
                y = util_y + 217.8 + 40.0;
                label(ui, rect(o.x + 32.0, y, width, 21.0), "Nodes", 15.0, "semibold", t.text);
                let basis = self.basis_select(ui, rect(o.x + 32.0, y, width, 28.0), "nodes", error.is_none());
                let title = match basis.as_str() {
                    "requests" => "Requests",
                    "limits" => "Limits",
                    _ => "Usage",
                };
                let cols = vec![
                    Column::new("name", "Name", 260.0, 4.0, false),
                    Column::new("age", "Age", 80.0, 0.0, true),
                    Column::new("cpu", &format!("CPU {title}"), 130.0, 0.0, true),
                    Column::new("memory", &format!("Mem {title}"), 130.0, 0.0, true),
                    Column::new("pods", "Pods", 90.0, 0.0, true),
                    Column::new("status", "Status", 130.0, 1.2, false),
                ];
                let mut rows = vec![];
                for n in &totals.nodes {
                    let mut row = table::resource_row(
                        model::resource("Node"),
                        n.row.clone(),
                        &cols,
                        chrono::Local::now().timestamp(),
                    );
                    for (cpu, id) in [(true, "cpu"), (false, "memory")] {
                        let value = match basis.as_str() {
                            "requests" => Some(n.requests.get(cpu)),
                            "limits" => Some(n.limits.get(cpu)),
                            _ => n.usage.map(|u| u.get(cpu)),
                        };
                        let fill = value.map(|v| model::percent(v, n.allocatable.get(cpu)));
                        row.put(
                            id,
                            Cell::gauge(
                                fill.map(|v| format!("{v:.2}%")).unwrap_or_else(|| "—".into()),
                                fill,
                                fill,
                                cpu,
                            ),
                        );
                    }
                    row.put(
                        "pods",
                        Cell::number(
                            format!("{}/{:.0}", n.pods, n.pod_capacity),
                            Some(model::percent(n.pods as f64, n.pod_capacity)),
                        ),
                    );
                    rows.push(row);
                }
                let height = 32.0 + (rows.len().max(1) as f32 * 44.0);
                self.show_table(
                    ui,
                    rect(o.x + 8.0, y + 33.0, ui.available_width() - 16.0, height),
                    "utilization.nodes",
                    &cols,
                    &rows,
                    false,
                    true,
                    44.0,
                    if nodes.synced { "No nodes." } else { "Loading nodes…" },
                    false,
                    true,
                );
                finish_scroll_content(ui, rect(o.x, o.y, width + 64.0, y - o.y + 33.0 + height + 40.0));
            });
        });
    }
    fn pods_of(&mut self, resource: &ResourceType, row: &ResourceRow) -> Vec<ResourceRow> {
        if resource.kind == "Pod" {
            return vec![row.clone()];
        }
        let pods = self.snapshot("Pod", row.namespace.as_deref()).rows;
        if resource.kind == "Node" {
            return pods
                .into_iter()
                .filter(|p| model::text(p, "node") == row.name)
                .collect();
        }
        if resource.kind == "ReplicaSet" {
            return pods
                .into_iter()
                .filter(|p| {
                    p.owner
                        .as_ref()
                        .is_some_and(|o| o.kind == "ReplicaSet" && o.name == row.name)
                })
                .collect();
        }
        if !model::has_logs(&resource.kind) {
            return vec![];
        }
        let mut workloads = vec![(resource.clone(), vec![row.clone()])];
        if resource.kind == "CronJob" {
            workloads.push((
                model::resource("Job"),
                self.snapshot("Job", row.namespace.as_deref()).rows,
            ));
        }
        model::applications(workloads, pods)
            .into_iter()
            .find(|a| a.resource.kind == resource.kind && a.row.name == row.name)
            .map(|a| a.pods)
            .unwrap_or_default()
    }
    #[allow(clippy::too_many_arguments)]
    fn resource_page(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        resource: ResourceType,
        namespace: Option<String>,
        object: String,
        tab: Tab,
        log_container: Option<String>,
    ) {
        let listed = self
            .data
            .rows(&self.context, &resource, namespace.as_deref(), None, false);
        let detail = self.data.rows(
            &self.context,
            &resource,
            namespace.as_deref(),
            Some(&format!("metadata.name={object}")),
            true,
        );
        let row = detail
            .rows
            .first()
            .cloned()
            .or_else(|| listed.rows.iter().find(|row| row.name == object).cloned());
        if detail.synced && row.is_none() {
            placeholder(
                ui,
                r,
                &format!(
                    "{} {object} no longer exists{}.",
                    resource.kind,
                    namespace.as_ref().map(|ns| format!(" in {ns}")).unwrap_or_default()
                ),
                self.t,
            );
            return;
        }
        let t = self.t;
        label(
            ui,
            rect(r.left() + 32.0, r.top() + 4.0, r.width() - 300.0, 30.8),
            &object,
            22.0,
            "bold",
            t.text,
        );
        label(
            ui,
            rect(r.left() + 32.0, r.top() + 34.8, r.width() - 64.0, 18.2),
            &resource.kind,
            13.0,
            "sans",
            t.muted,
        );
        let pods = row.as_ref().map(|row| self.pods_of(&resource, row)).unwrap_or_default();
        let delete = rect(r.right() - 112.0, r.top() + 4.0, 80.0, 27.5);
        let response = button(
            ui,
            delete,
            "delete-resource",
            "",
            12.5,
            t.failed,
            None,
            Some(t.strong),
            t,
        );
        self.icons.paint(
            ui,
            "trash-2",
            rect(delete.left() + 10.0, delete.center().y - 6.5, 13.0, 13.0),
            t.failed,
        );
        label(
            ui,
            rect(
                delete.left() + 29.0,
                delete.top(),
                delete.width() - 39.0,
                delete.height(),
            ),
            "Delete",
            12.5,
            "sans",
            t.failed,
        );
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Delete"));
        if response.clicked() {
            self.actions
                .push(Action::Delete(resource.clone(), namespace.clone(), object.clone()));
        }
        if let Some(row) = &row {
            let status_text = row.status.as_deref().unwrap_or_default();
            let w = text_width(ui, status_text, 13.0, "medium") + 19.0;
            status(
                ui,
                &mut self.icons,
                rect(delete.left() - 16.0 - w, r.top() + 4.0, w, 18.2),
                row.health,
                status_text,
                t,
            );
            let subtitle = if resource.kind == "Pod" {
                let containers: Vec<_> = model::containers(row).into_iter().filter(|c| !c.init).collect();
                Some(format!(
                    "{}/{} Containers",
                    containers.iter().filter(|c| c.ready).count(),
                    containers.len()
                ))
            } else if model::has_logs(&resource.kind) {
                Some(format!(
                    "{}/{} Pods",
                    pods.iter().filter(|p| p.health == Some(Health::Ok)).count(),
                    pods.len()
                ))
            } else {
                None
            };
            if let Some(sub) = subtitle {
                let w = text_width(ui, &sub, 13.0, "sans");
                label(
                    ui,
                    rect(delete.left() - 16.0 - w, r.top() + 22.2, w, 18.2),
                    &sub,
                    13.0,
                    "sans",
                    t.muted,
                );
            }
        }
        let mut y = r.top() + 73.0;
        if let Some(error) = detail.error {
            y += self.banner(ui, rect(r.left(), y, r.width(), r.bottom() - y), &error, false);
        }
        let body = rect(r.left(), y, r.width(), r.bottom() - y);
        let Some(row) = row else {
            placeholder(ui, body, "Loading…", t);
            return;
        };
        match tab {
            Tab::Logs => self.logs_panel(ui, body, &resource, &pods, log_container.as_deref()),
            Tab::Yaml => self.yaml_panel(ui, body, resource, namespace, object),
            Tab::Events => {
                let events = self.data.rows(
                    &self.context,
                    &model::resource("Event"),
                    if resource.namespaced {
                        row.namespace.as_deref()
                    } else {
                        None
                    },
                    Some(&format!("involvedObject.uid={}", row.uid)),
                    false,
                );
                let cols: Vec<_> = table::columns("Event", false)
                    .into_iter()
                    .filter(|c| c.id != "object")
                    .collect();
                let rows: Vec<_> = events
                    .rows
                    .into_iter()
                    .map(|row| {
                        table::resource_row(model::resource("Event"), row, &cols, chrono::Local::now().timestamp())
                    })
                    .collect();
                let h = events.error.map(|e| self.banner(ui, body, &e, false)).unwrap_or(0.0);
                self.show_table(
                    ui,
                    rect(body.left(), body.top() + h, body.width(), body.height() - h),
                    "resource.events",
                    &cols,
                    &rows,
                    true,
                    false,
                    36.0,
                    if events.synced {
                        "No events. Kubernetes keeps events for about an hour."
                    } else {
                        "Loading events…"
                    },
                    false,
                    false,
                );
            }
            Tab::Overview => self.resource_overview(ui, body, &resource, &row, &pods),
        }
    }
    fn resource_overview(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        resource: &ResourceType,
        row: &ResourceRow,
        pods: &[ResourceRow],
    ) {
        let t = self.t;
        let runs = model::has_logs(&resource.kind);
        let node = resource.kind == "Node";
        let metrics_error = self.data.context_data.get(&self.context).and_then(|d| {
            if node {
                d.node_error.clone().or_else(|| d.pod_error.clone())
            } else if runs {
                d.pod_error.clone()
            } else {
                None
            }
        });
        ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
            ui.set_clip_rect(r);
            page_scroll(ui, r, "resource-overview").show(ui, |ui| {
                let o = ui.cursor().min;
                let width = ui.available_width() - 64.0;
                let mut y = o.y;
                y += self.metrics_warning(ui, rect(o.x, y, r.width(), 200.0), metrics_error.clone());
                let two = (runs || node) && r.width() + self.prefs.sidebar_width >= 1280.0;
                let left_w = if two { (width - 20.0) * 1.3 / 2.3 } else { width };
                label(
                    ui,
                    rect(o.x + 32.0, y, left_w, 21.0),
                    "Overview",
                    15.0,
                    "semibold",
                    t.text,
                );
                let mut fields: Vec<(String, String, bool)> = vec![("Kind".into(), resource.kind.clone(), false)];
                if let Some(ns) = &row.namespace {
                    fields.push(("Namespace".into(), ns.clone(), false));
                }
                fields.push((
                    "Age".into(),
                    model::age(row.created, chrono::Local::now().timestamp()),
                    false,
                ));
                if node {
                    for (id, title) in [
                        ("roles", "Roles"),
                        ("version", "Version"),
                        ("instanceType", "Instance type"),
                        ("osImage", "OS Image"),
                    ] {
                        let value = model::text(row, id);
                        if !value.is_empty() {
                            fields.push((title.into(), value, false));
                        }
                    }
                    fields.push((
                        "OS Info".into(),
                        format!("{} · {}", model::text(row, "os"), model::text(row, "arch")),
                        false,
                    ));
                    fields.push((
                        "Pods".into(),
                        format!(
                            "{}/{:.0} · {:.0}%",
                            pods.iter().filter(|p| model::active(p)).count(),
                            model::num(row, "podCapacity"),
                            model::percent(
                                pods.iter().filter(|p| model::active(p)).count() as f64,
                                model::num(row, "podCapacity")
                            )
                        ),
                        false,
                    ));
                    let addresses = row
                        .fields
                        .get("addresses")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            let width = a
                                .iter()
                                .map(|v| v["type"].as_str().unwrap_or_default().chars().count())
                                .max()
                                .unwrap_or_default();
                            a.iter()
                                .map(|v| {
                                    format!(
                                        "{:<width$}    {}",
                                        v["type"].as_str().unwrap_or_default(),
                                        v["address"].as_str().unwrap_or_default()
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default();
                    fields.push(("IP Addresses".into(), addresses, true));
                } else {
                    let images = if resource.kind == "Pod" {
                        model::containers(row)
                            .into_iter()
                            .map(|c| format!("{} {}", c.name, c.image))
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else {
                        row.fields
                            .get("images")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .map(|v| {
                                        format!(
                                            "{} {}",
                                            v["name"].as_str().unwrap_or_default(),
                                            v["image"].as_str().unwrap_or_default()
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .unwrap_or_default()
                    };
                    if !images.is_empty() {
                        fields.push(("Images".into(), images, true));
                    }
                    let n = model::text(row, "node");
                    if !n.is_empty() {
                        fields.push(("Node".into(), n, false));
                    }
                    if let Some(owner) = &row.owner {
                        fields.push(("Owner".into(), format!("{} ({})", owner.name, owner.kind), false));
                    }
                    for c in table::columns(&resource.kind, false)
                        .into_iter()
                        .filter(|c| !matches!(c.id.as_str(), "name" | "age" | "status" | "node" | "ready" | "restarts"))
                    {
                        let value = if c.id == "duration" {
                            model::duration(model::num(row, "duration") as i64)
                        } else if c.id == "lastSchedule" {
                            model::age(
                                row.fields.get("lastSchedule").and_then(|v| v.as_i64()),
                                chrono::Local::now().timestamp(),
                            )
                        } else {
                            model::text(row, &c.id)
                        };
                        fields.push((c.title, value, false));
                    }
                }
                let value_width = left_w - 192.0;
                let mut heights = vec![];
                let mut galleys = vec![];
                for (title, value, mono) in &fields {
                    let h = if *mono {
                        let mut job = eframe::egui::text::LayoutJob::default();
                        job.wrap.max_width = value_width;
                        job.wrap.break_anywhere = true;
                        for (index, entry) in value.split('\n').enumerate() {
                            if index > 0 {
                                job.append("\n", 0.0, text_format(12.0, "mono", t.text));
                            }
                            let (name, rest) = if title == "Images" {
                                entry.split_once(' ').unwrap_or((entry, ""))
                            } else {
                                ("", entry)
                            };
                            for (text, family) in [(name, "mono-bold"), (rest, "mono")] {
                                let mut format = text_format(12.0, family, t.text);
                                format.line_height = Some(20.0);
                                if !text.is_empty() {
                                    job.append(
                                        text,
                                        if family == "mono" && !name.is_empty() { 7.2 } else { 0.0 },
                                        format,
                                    );
                                }
                            }
                        }
                        let galley = ui.painter().layout_job(job);
                        let height = galley.size().y.max(20.0);
                        galleys.push(Some(galley));
                        height
                    } else {
                        galleys.push(None);
                        18.2
                    };
                    heights.push(h);
                }
                let card_h = (40.0 + heights.iter().sum::<f32>() + 8.0 * fields.len().saturating_sub(1) as f32)
                    .max(if two { 180.6 } else { 0.0 });
                let card = rect(o.x + 32.0, y + 33.0, left_w, card_h);
                ui.painter().rect_filled(card, 8, t.raised);
                let mut fy = card.top() + 20.0;
                for (((title, value, mono), height), galley) in fields.iter().zip(&heights).zip(galleys) {
                    label(
                        ui,
                        rect(card.left() + 24.0, fy, 120.0, 18.2),
                        title,
                        13.0,
                        "sans",
                        t.muted,
                    );
                    let vr = rect(card.left() + 168.0, fy, value_width, *height);
                    if *mono {
                        if let Some(galley) = galley {
                            ui.painter().galley(vr.left_top() + vec2(0.0, 3.0), galley, t.text);
                        }
                    } else if title == "Node" {
                        label(ui, vr, value, 13.0, "sans", t.accent);
                        if ui.interact(vr, ui.id().with("node-link"), Sense::click()).clicked() {
                            self.actions.push(Action::Navigate(View::Resource {
                                resource: model::resource("Node"),
                                namespace: None,
                                object: value.clone(),
                                tab: Tab::Overview,
                                log_container: None,
                            }));
                        }
                    } else if title == "Owner" {
                        let owner = row.owner.as_ref().unwrap();
                        let r = model::resource(&owner.kind);
                        let clickable = matches!(
                            owner.kind.as_str(),
                            "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Job" | "CronJob" | "Node"
                        );
                        let name_width = text_width(ui, &owner.name, 13.0, "sans");
                        label(
                            ui,
                            vr,
                            &owner.name,
                            13.0,
                            "sans",
                            if clickable { t.accent } else { t.text },
                        );
                        label(
                            ui,
                            rect(
                                vr.left() + name_width + 4.0,
                                vr.top(),
                                (vr.width() - name_width - 4.0).max(0.0),
                                vr.height(),
                            ),
                            &format!("({})", owner.kind),
                            13.0,
                            "sans",
                            t.muted,
                        );
                        if clickable && ui.interact(vr, ui.id().with("owner-link"), Sense::click()).clicked() {
                            self.actions.push(Action::Navigate(View::Resource {
                                resource: r,
                                namespace: row.namespace.clone(),
                                object: owner.name.clone(),
                                tab: Tab::Overview,
                                log_container: None,
                            }));
                        }
                    } else {
                        label(ui, vr, value, 13.0, "sans", t.text);
                    }
                    fy += height + 8.0;
                }
                if runs || node {
                    let util_x = if two { card.right() + 20.0 } else { card.left() };
                    let util_y = if two { y } else { card.bottom() + 20.0 };
                    let util_w = if two { width - left_w - 20.0 } else { width };
                    label(
                        ui,
                        rect(util_x, util_y, util_w, 21.0),
                        "Utilization",
                        15.0,
                        "semibold",
                        t.text,
                    );
                    let ur = rect(util_x, util_y + 33.0, util_w, card_h.max(180.6));
                    if node {
                        let totals = self.totals(vec![row.clone()], pods);
                        self.capacity(ui, ur, &totals);
                    } else {
                        self.pod_utilization(ui, ur, pods);
                    }
                    y = ur.bottom() + 32.0;
                } else {
                    y = card.bottom() + 32.0;
                }
                if resource.kind == "Pod" {
                    y += self.containers_table(ui, rect(o.x + 32.0, y, width, 1000.0), row);
                } else if runs || node {
                    let shown: Vec<_> = if node {
                        pods.iter().filter(|p| model::active(p)).cloned().collect()
                    } else {
                        pods.to_vec()
                    };
                    y += self.pods_table(
                        ui,
                        rect(o.x + 32.0, y, width, 1000.0),
                        if node { "Scheduled Pods" } else { "Pods" },
                        &shown,
                        node,
                    );
                }
                y += self.details(ui, rect(o.x + 32.0, y, width, 1000.0), row, two);
                if runs {
                    y += self.related(ui, rect(o.x + 32.0, y, width, 10000.0), row.namespace.as_deref(), pods);
                }
                finish_scroll_content(ui, rect(o.x, o.y, width + 64.0, y - o.y + 40.0));
            });
        });
    }
    fn pod_utilization(&mut self, ui: &mut Ui, r: Rect, pods: &[ResourceRow]) {
        let t = self.t;
        ui.painter().rect_filled(r, 8, t.raised);
        let active: Vec<_> = pods.iter().filter(|p| model::active(p)).cloned().collect();
        let used = self.used(&active);
        let mut req = Amounts::default();
        let mut lim = Amounts::default();
        for p in &active {
            req.add(model::requests(p));
            lim.add(model::limits(p));
        }
        let w = (r.width() - 80.0) / 2.0;
        for (i, cpu) in [true, false].into_iter().enumerate() {
            let x = r.left() + 24.0 + i as f32 * (w + 32.0);
            let share = used.and_then(|u| {
                let basis = if req.get(cpu) > 0.0 { req.get(cpu) } else { lim.get(cpu) };
                (basis > 0.0).then(|| model::percent(u.get(cpu), basis))
            });
            let title = if cpu { "CPU %" } else { "Memory %" };
            label(ui, rect(x, r.top() + 20.0, w, 18.2), title, 13.0, "semibold", t.text);
            let text = share.map(|v| format!("{v:.2}%")).unwrap_or_else(|| "—".into());
            let tw = text_width(ui, &text, 13.0, "semibold");
            label(
                ui,
                rect(x + w - tw, r.top() + 20.0, tw, 18.2),
                &text,
                13.0,
                "semibold",
                t.text,
            );
            bar(ui, rect(x, r.top() + 46.2, w, 6.0), share.unwrap_or(0.0), Some(cpu), t);
            for (j, (title, value)) in [
                (
                    "Usage",
                    used.map(|u| model::amount(cpu, u.get(cpu)))
                        .unwrap_or_else(|| "—".into()),
                ),
                (
                    "Requested",
                    if req.get(cpu) > 0.0 {
                        model::amount(cpu, req.get(cpu))
                    } else {
                        "Not set".into()
                    },
                ),
                (
                    "Limit",
                    if lim.get(cpu) > 0.0 {
                        model::amount(cpu, lim.get(cpu))
                    } else {
                        "Not set".into()
                    },
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let y = r.top() + 68.2 + j as f32 * 22.2;
                label(ui, rect(x, y, w, 18.2), title, 13.0, "sans", t.muted);
                let tw = text_width(ui, &value, 13.0, "sans");
                label(ui, rect(x + w - tw, y, tw, 18.2), &value, 13.0, "sans", t.text);
            }
        }
    }
    fn pods_table(&mut self, ui: &mut Ui, r: Rect, title: &str, pods: &[ResourceRow], ns: bool) -> f32 {
        let t = self.t;
        let available = self
            .data
            .context_data
            .get(&self.context)
            .is_none_or(|d| d.pod_error.is_none());
        label(
            ui,
            rect(r.left(), r.top(), r.width(), 21.0),
            title,
            15.0,
            "semibold",
            t.text,
        );
        let basis = self.basis_select(ui, rect(r.left(), r.top(), r.width(), 28.0), "pods", available);
        let basis_title = match basis.as_str() {
            "requests" => "Requests",
            "limits" => "Limits",
            _ => "Usage",
        };
        let mut cols = vec![Column::new("name", "Name", 220.0, 3.0, false)];
        if ns {
            cols.push(Column::new("namespace", "Namespace", 120.0, 1.2, false));
        }
        cols.extend([
            Column::new("age", "Age", 72.0, 0.0, true),
            Column::new("containers", "Containers", 96.0, 0.0, true),
            Column::new("restarts", "Restarts", 84.0, 0.0, true),
            Column::new("cpu", &format!("CPU {basis_title}"), 130.0, 0.0, true),
            Column::new("memory", &format!("Mem {basis_title}"), 130.0, 0.0, true),
            Column::new("status", "Status", 140.0, 1.3, false),
            Column::new("actions", "", 40.0, 0.0, false),
        ]);
        let nodes = self.snapshot("Node", None).rows;
        let mut rows = vec![];
        for p in pods {
            let mut row = table::resource_row(
                model::resource("Pod"),
                p.clone(),
                &cols,
                chrono::Local::now().timestamp(),
            );
            row.put(
                "containers",
                Cell::number(
                    model::text(p, "ready"),
                    Some(model::num(p, "readyCount") / model::num(p, "total").max(1.0)),
                ),
            );
            for (cpu, id) in [(true, "cpu"), (false, "memory")] {
                let value = match basis.as_str() {
                    "requests" => Some(model::requests(p).get(cpu)),
                    "limits" => Some(model::limits(p).get(cpu)),
                    _ => self.pod_sample(p).map(|s| if cpu { s.cpu } else { s.memory }),
                };
                let reference = if basis == "usage" {
                    let lim = model::limits(p).get(cpu);
                    if lim > 0.0 { lim } else { model::requests(p).get(cpu) }
                } else {
                    nodes
                        .iter()
                        .find(|n| n.name == model::text(p, "node"))
                        .map(|n| model::num(n, if cpu { "cpu" } else { "memory" }))
                        .unwrap_or(0.0)
                };
                row.put(
                    id,
                    Cell::gauge(
                        value
                            .map(|v| {
                                if basis != "usage" && v == 0.0 {
                                    "Not set".into()
                                } else {
                                    model::amount(cpu, v)
                                }
                            })
                            .unwrap_or_else(|| "—".into()),
                        value,
                        value.and_then(|v| (reference > 0.0).then(|| model::percent(v, reference))),
                        cpu,
                    ),
                );
            }
            rows.push(row);
        }
        let height = 32.0 + 44.0 * rows.len().max(1) as f32;
        self.show_table(
            ui,
            rect(r.left() - 24.0, r.top() + 33.0, r.width() + 48.0, height),
            if ns { "node.pods" } else { "resource.pods" },
            &cols,
            &rows,
            false,
            true,
            44.0,
            "No pods",
            true,
            true,
        );
        33.0 + height + 32.0
    }
    fn containers_table(&mut self, ui: &mut Ui, r: Rect, pod: &ResourceRow) -> f32 {
        let t = self.t;
        label(
            ui,
            rect(r.left(), r.top(), r.width(), 21.0),
            "Containers",
            15.0,
            "semibold",
            t.text,
        );
        let cols = vec![
            Column::new("name", "Name", 220.0, 3.0, false),
            Column::new("restarts", "Restarts", 90.0, 0.0, true),
            Column::new("cpu", "CPU Usage", 130.0, 0.0, true),
            Column::new("memory", "Mem Usage", 130.0, 0.0, true),
            Column::new("status", "Status", 140.0, 1.4, false),
            Column::new("actions", "", 40.0, 0.0, false),
        ];
        let mut rows = vec![];
        let sample = self.pod_sample(pod);
        for c in model::containers(pod) {
            let mut row = Row::new(model::resource("Pod"), pod.clone());
            row.id = c.name.clone();
            row.container = Some(c.name.clone());
            let mut name = Cell::text(&c.name);
            name.suffix = c.init.then(|| "init".into());
            row.put("name", name);
            row.put(
                "restarts",
                Cell::number(c.restarts.to_string(), Some(c.restarts as f64)),
            );
            row.put(
                "status",
                Cell::status(
                    &c.state,
                    Some(if c.state == "Running" {
                        if c.ready { Health::Ok } else { Health::Progress }
                    } else if c.state == "Completed" {
                        Health::Done
                    } else if model::not_started(&c.state) {
                        Health::Progress
                    } else {
                        Health::Failed
                    }),
                ),
            );
            for (cpu, id) in [(true, "cpu"), (false, "memory")] {
                let value = sample.map(|s| {
                    s.containers
                        .iter()
                        .find(|s| s.name == c.name)
                        .map(|s| if cpu { s.cpu } else { s.memory })
                        .unwrap_or(0.0)
                });
                let lim = if cpu { c.limits.cpu } else { c.limits.memory };
                let reference = if lim > 0.0 {
                    lim
                } else if cpu {
                    c.requests.cpu
                } else {
                    c.requests.memory
                };
                row.put(
                    id,
                    Cell::gauge(
                        value.map(|v| model::amount(cpu, v)).unwrap_or_else(|| "—".into()),
                        value,
                        value.and_then(|v| (reference > 0.0).then(|| model::percent(v, reference))),
                        cpu,
                    ),
                );
            }
            rows.push(row);
        }
        let height = 32.0 + 44.0 * rows.len().max(1) as f32;
        self.show_table(
            ui,
            rect(r.left() - 24.0, r.top() + 33.0, r.width() + 48.0, height),
            "pod.containers",
            &cols,
            &rows,
            false,
            true,
            44.0,
            "No containers",
            true,
            false,
        );
        33.0 + height + 32.0
    }
    fn details(&mut self, ui: &mut Ui, r: Rect, row: &ResourceRow, two: bool) -> f32 {
        let t = self.t;
        label(
            ui,
            rect(r.left(), r.top(), r.width(), 21.0),
            "Details",
            15.0,
            "semibold",
            t.text,
        );
        let width = if two { (r.width() - 40.0) / 2.0 } else { r.width() };
        let lh = self.key_values(
            ui,
            rect(r.left(), r.top() + 33.0, width, 10000.0),
            "Labels",
            &row.labels,
        );
        let ah = if let Some(annotations) = &row.annotations {
            self.key_values(
                ui,
                rect(
                    if two { r.left() + width + 40.0 } else { r.left() },
                    if two {
                        r.top() + 33.0
                    } else {
                        r.top() + 33.0 + lh + 24.0
                    },
                    width,
                    10000.0,
                ),
                "Annotations",
                annotations,
            )
        } else {
            0.0
        };
        33.0 + if two { lh.max(ah) } else { lh + 24.0 + ah } + 32.0
    }
    fn key_values(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        title: &str,
        values: &std::collections::BTreeMap<String, String>,
    ) -> f32 {
        let t = self.t;
        label(
            ui,
            rect(r.left(), r.top() + 4.0, 110.0, 18.2),
            title,
            13.0,
            "sans",
            t.muted,
        );
        let x = r.left() + 126.0;
        let width = (r.width() - 126.0).max(20.0);
        if values.is_empty() {
            label(ui, rect(x, r.top() + 4.0, width, 18.2), "None", 13.0, "sans", t.faint);
            return 26.2;
        }
        let mut y = r.top();
        for (key, value) in values {
            let id = format!("{title}/{key}");
            let text = format!("{key}: {value}");
            let long = text.chars().count() > 120 || value.contains('\n');
            let open = self.expanded.contains(&id);
            let mut format = text_format(12.0, "mono", t.text);
            format.line_height = Some(20.0);
            let mut job = eframe::egui::text::LayoutJob::default();
            job.wrap.max_width = width - 20.0;
            job.wrap.break_anywhere = true;
            job.append(&text, 0.0, format);
            if long && !open {
                job.wrap.max_rows = 2;
            }
            let galley = ui.painter().layout_job(job);
            let height = galley.size().y.max(20.0);
            let badge_width = (galley.size().x + 20.0).min(width);
            let h = height + 8.0 + if long { 20.0 } else { 0.0 };
            ui.painter().rect_filled(rect(x, y, badge_width, h), 6, t.raised);
            let leading = galley
                .rows
                .first()
                .and_then(|r| r.glyphs.first())
                .map_or(0.0, |glyph| (glyph.line_height - glyph.font_height).max(0.0) / 2.0);
            ui.place(
                Rect::from_min_size(pos2(x + 10.0, y + 4.0 + leading), galley.size()),
                Label::new(galley).selectable(true).show_tooltip_when_elided(false),
            );
            if long {
                let rr = rect(x + 10.0, y + 4.0 + height, width - 20.0, 20.0);
                label(
                    ui,
                    rr,
                    if open { "Show less" } else { "Show more" },
                    12.0,
                    "mono-bold",
                    t.text,
                );
                if ui.interact(rr, ui.id().with(&id), Sense::click()).clicked() {
                    if open {
                        self.expanded.remove(&id);
                    } else {
                        self.expanded.insert(id);
                    }
                }
            }
            y += h + 6.0;
        }
        y - r.top() - 6.0
    }
    fn related(&mut self, ui: &mut Ui, r: Rect, ns: Option<&str>, pods: &[ResourceRow]) -> f32 {
        let services = self
            .snapshot("Service", ns)
            .rows
            .into_iter()
            .filter(|s| {
                s.fields
                    .get("selector")
                    .and_then(|v| v.as_object())
                    .is_some_and(|selector| {
                        !selector.is_empty()
                            && pods.iter().any(|p| {
                                selector
                                    .iter()
                                    .all(|(k, v)| p.labels.get(k).map(String::as_str) == v.as_str())
                            })
                    })
            })
            .collect::<Vec<_>>();
        let names: HashSet<_> = services.iter().map(|s| s.name.clone()).collect();
        let ingresses = self
            .snapshot("Ingress", ns)
            .rows
            .into_iter()
            .filter(|i| {
                i.fields
                    .get("backends")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| a.iter().any(|v| v.as_str().is_some_and(|s| names.contains(s))))
            })
            .collect::<Vec<_>>();
        let configs: std::collections::BTreeSet<String> = pods
            .iter()
            .flat_map(|p| {
                p.fields
                    .get("configMaps")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                    .map(str::to_owned)
            })
            .collect();
        let claims: HashSet<String> = pods
            .iter()
            .flat_map(|p| {
                p.fields
                    .get("pvcs")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                    .map(str::to_owned)
            })
            .collect();
        let pvcs = self
            .snapshot("PersistentVolumeClaim", ns)
            .rows
            .into_iter()
            .filter(|c| claims.contains(&c.name))
            .collect::<Vec<_>>();
        let mut y = r.top();
        y += self.mini_related(
            ui,
            rect(r.left(), y, r.width(), 10000.0),
            "Services",
            "No Services",
            "Service",
            &services,
        );
        y += self.mini_related(
            ui,
            rect(r.left(), y, r.width(), 10000.0),
            "Ingresses",
            "No Ingresses",
            "Ingress",
            &ingresses,
        );
        label(
            ui,
            rect(r.left(), y, r.width(), 21.0),
            "Config Maps",
            15.0,
            "semibold",
            self.t.text,
        );
        y += 33.0;
        if configs.is_empty() {
            label(
                ui,
                rect(r.left(), y, r.width(), 18.2),
                "No Config Maps",
                13.0,
                "sans",
                self.t.muted,
            );
            y += 18.2 + 32.0;
        } else {
            label(ui, rect(r.left(), y, 220.0, 24.8), "Name", 12.0, "sans", self.t.muted);
            label(
                ui,
                rect(r.left() + 220.0, y, r.width() - 220.0, 24.8),
                "Data",
                12.0,
                "sans",
                self.t.muted,
            );
            line(ui, pos2(r.left(), y + 24.8), pos2(r.right(), y + 24.8), self.t.line);
            y += 24.8;
            for name in configs {
                let snapshot = self.data.rows(
                    &self.context,
                    &model::resource("ConfigMap"),
                    ns,
                    Some(&format!("metadata.name={name}")),
                    true,
                );
                let row = snapshot.rows.first();
                let name_r = rect(r.left(), y + 12.0, 204.0, 18.2);
                label(ui, name_r, &name, 13.0, "sans", self.t.text);
                if let Some(row) = row
                    && ui
                        .interact(name_r, ui.id().with(("configmap", &name)), Sense::click())
                        .clicked()
                {
                    self.actions.push(Action::Navigate(Self::resource_view(
                        model::resource("ConfigMap"),
                        row,
                        Tab::Overview,
                    )));
                }
                let mut data_y = y + 12.0;
                if let Some(row) = row {
                    let entries = row.fields.get("data").and_then(|v| v.as_object());
                    let empty = entries.is_none_or(|v| v.is_empty());
                    if empty {
                        label(
                            ui,
                            rect(r.left() + 220.0, data_y, r.width() - 220.0, 18.2),
                            "No data",
                            13.0,
                            "sans",
                            self.t.muted,
                        );
                        data_y += 18.2;
                    }
                    if let Some(entries) = entries {
                        let mut sorted: Vec<_> = entries.iter().collect();
                        sorted.sort_by(|a, b| a.0.cmp(b.0));
                        for (key, v) in sorted {
                            let value = v.as_str().unwrap_or_default();
                            let id = format!("cm/{name}/{key}");
                            let long = value.len() > 1200 || value.lines().count() > 12;
                            let open = self.expanded.contains(&id);
                            let key_w = ((r.width() - 220.0) * 0.35).clamp(120.0, 240.0);
                            label(
                                ui,
                                rect(r.left() + 220.0, data_y, key_w, 20.0),
                                key,
                                12.0,
                                "mono-bold",
                                self.t.text,
                            );
                            let mut job = eframe::egui::text::LayoutJob::simple(
                                value.into(),
                                font(12.0, "mono"),
                                self.t.text,
                                r.width() - 236.0 - key_w,
                            );
                            if long && !open {
                                job.wrap.max_rows = 12;
                            }
                            let g = ui.painter().layout_job(job);
                            let h = g.size().y.max(20.0);
                            ui.painter()
                                .galley(pos2(r.left() + 236.0 + key_w, data_y), g, self.t.text);
                            data_y += h;
                            if long {
                                let rr = rect(r.left() + 236.0 + key_w, data_y, r.width() - 236.0 - key_w, 20.0);
                                label(
                                    ui,
                                    rr,
                                    if open { "Show less" } else { "Show more" },
                                    12.0,
                                    "mono",
                                    self.t.accent,
                                );
                                if ui.interact(rr, ui.id().with(&id), Sense::click()).clicked() {
                                    if open {
                                        self.expanded.remove(&id);
                                    } else {
                                        self.expanded.insert(id);
                                    }
                                }
                                data_y += 20.0;
                            }
                            data_y += 12.0;
                        }
                    }
                } else if snapshot.synced {
                    label(
                        ui,
                        rect(r.left() + 220.0, data_y, r.width() - 220.0, 18.2),
                        "Not found in this namespace",
                        13.0,
                        "sans",
                        self.t.failed,
                    );
                    data_y += 18.2;
                }
                y = data_y.max(y + 30.2) + 12.0;
                line(ui, pos2(r.left(), y), pos2(r.right(), y), self.t.line);
            }
            y += 32.0;
        }
        y += self.mini_related(
            ui,
            rect(r.left(), y, r.width(), 10000.0),
            "Persistent Volume Claims",
            "No Persistent Volume Claims",
            "PersistentVolumeClaim",
            &pvcs,
        );
        y - r.top()
    }
    fn mini_related(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        title: &str,
        empty: &str,
        kind: &str,
        objects: &[ResourceRow],
    ) -> f32 {
        label(
            ui,
            rect(r.left(), r.top(), r.width(), 21.0),
            title,
            15.0,
            "semibold",
            self.t.text,
        );
        if objects.is_empty() {
            label(
                ui,
                rect(r.left(), r.top() + 33.0, r.width(), 18.2),
                empty,
                13.0,
                "sans",
                self.t.muted,
            );
            return 83.2;
        }
        let columns = match kind {
            "Service" => vec![
                Column::new("name", "Name", 160.0, 1.0, false),
                Column::new("type", "Type", 120.0, 0.0, false),
                Column::new("ports", "Ports", 240.0, 3.0, false),
            ],
            "Ingress" => vec![
                Column::new("name", "Name", 160.0, 1.0, false),
                Column::new("class", "Class", 120.0, 0.0, false),
                Column::new("hosts", "Hosts", 200.0, 2.0, false),
                Column::new("address", "Address", 160.0, 1.5, false),
            ],
            _ => vec![
                Column::new("name", "Name", 180.0, 2.0, false),
                Column::new("status", "Status", 120.0, 0.0, false),
                Column::new("capacity", "Capacity", 100.0, 0.0, false),
                Column::new("storageClass", "Storage class", 140.0, 0.0, false),
                Column::new("age", "Age", 72.0, 0.0, false),
            ],
        };
        let tracks = table::tracks(&columns.iter().collect::<Vec<_>>(), r.width());
        let t = self.t;
        let mut y = r.top() + 33.0;
        let header_height = 25.8;
        let mut x = r.left();
        for (column, width) in columns.iter().zip(&tracks) {
            label(ui, rect(x, y, *width, 16.8), &column.title, 12.0, "sans", t.muted);
            x += width;
        }
        y += header_height;
        line(ui, pos2(r.left(), y - 0.5), pos2(r.right(), y - 0.5), t.line);
        for row in objects {
            let mut cells = vec![];
            let mut height = 44.0_f32;
            for (column, width) in columns.iter().zip(&tracks) {
                let mut job = text_job("", 13.0, "sans", t.text);
                job.wrap.max_width = width - 16.0;
                if column.id == "ports" {
                    if let Some(ports) = row.fields.get("portList").and_then(|v| v.as_array()) {
                        for (index, port) in ports.iter().enumerate() {
                            if index > 0 {
                                job.append("\n", 0.0, text_format(13.0, "sans", t.text));
                            }
                            let protocol = port["protocol"].as_str().unwrap_or("TCP");
                            let number = format!(
                                "{}{}",
                                model::value_text(&port["port"]),
                                if protocol == "TCP" {
                                    String::new()
                                } else {
                                    format!("/{protocol}")
                                }
                            );
                            for (index, (title, value)) in [
                                ("Port", Some(number)),
                                (
                                    "Target Port",
                                    (!port["targetPort"].is_null()).then(|| model::value_text(&port["targetPort"])),
                                ),
                                (
                                    "Node Port",
                                    (!port["nodePort"].is_null()).then(|| model::value_text(&port["nodePort"])),
                                ),
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                if let Some(value) = value {
                                    job.append(
                                        &format!("{title} "),
                                        if index == 0 { 0.0 } else { 24.0 },
                                        text_format(13.0, "sans", t.muted),
                                    );
                                    job.append(&value, 0.0, text_format(13.0, "mono", t.text));
                                }
                            }
                        }
                    }
                } else {
                    let value = match column.id.as_str() {
                        "name" => row.name.clone(),
                        "status" => row.status.clone().unwrap_or_default(),
                        "age" => model::age(row.created, chrono::Local::now().timestamp()),
                        id => model::text(row, id),
                    };
                    job.append(&value, 0.0, text_format(13.0, "sans", t.text));
                }
                for section in &mut job.sections {
                    section.format.line_height = Some(18.2);
                }
                let galley = ui.painter().layout_job(job);
                height = height.max(galley.size().y + 17.0);
                cells.push(galley);
            }
            let rr = rect(r.left(), y, r.width(), height);
            let response = ui.interact(rr, ui.id().with(("related", kind, &row.uid)), Sense::click());
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &row.name));
            if response.hovered() {
                // Keep every column aligned while giving the hover tint breathing room.
                let hover = rr.expand2(vec2(8.0, 0.0));
                ui.painter().add(
                    Shadow {
                        offset: [0, 1],
                        blur: 8,
                        spread: 0,
                        color: Color32::from_black_alpha(24),
                    }
                    .as_shape(hover, 4),
                );
                ui.painter().rect_filled(hover, 4, t.hover);
            }
            if response.clicked() || (response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                self.actions.push(Action::Navigate(Self::resource_view(
                    model::resource(kind),
                    row,
                    Tab::Overview,
                )));
            }
            let mut x = r.left();
            for ((column, width), galley) in columns.iter().zip(&tracks).zip(cells) {
                let cr = rect(x, y, *width - 16.0, height);
                if column.id == "status" {
                    status(
                        ui,
                        &mut self.icons,
                        cr,
                        row.health,
                        row.status.as_deref().unwrap_or(""),
                        t,
                    );
                } else {
                    ui.painter()
                        .galley(pos2(x, y + (height - galley.size().y) / 2.0), galley, t.text);
                }
                x += width;
            }
            y += height;
            line(ui, pos2(r.left(), y - 0.5), pos2(r.right(), y - 0.5), t.line);
        }
        y - r.top() + 32.0
    }
}
fn groups(types: &[ResourceInfo], query: &str) -> Vec<(String, Vec<ResourceInfo>)> {
    let spec: serde_json::Value = serde_json::from_str(include_str!("../../../../assets/kinds.json")).unwrap();
    let mut known = HashSet::new();
    let mut groups = vec![];
    for section in spec["sections"].as_array().unwrap() {
        let mut items = vec![];
        for key in section["kinds"].as_array().unwrap() {
            let key = key.as_str().unwrap();
            known.insert(key.to_owned());
            if let Some(t) = types.iter().find(|t| model::type_key(&t.resource) == key) {
                items.push(t.clone());
            }
        }
        groups.push((section["title"].as_str().unwrap().into(), items));
    }
    let builtins: HashSet<_> = spec["builtins"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    let mut custom = vec![];
    let mut other = vec![];
    for t in types {
        if known.contains(&model::type_key(&t.resource)) {
            continue;
        }
        if !builtins.contains(t.resource.group.as_str()) {
            custom.push(t.clone());
        } else if t.resource.group != "events.k8s.io" {
            other.push(t.clone());
        }
    }
    custom.sort_by(|a, b| {
        a.resource
            .group
            .cmp(&b.resource.group)
            .then(a.resource.kind.cmp(&b.resource.kind))
    });
    other.sort_by(|a, b| a.resource.kind.cmp(&b.resource.kind));
    groups.push(("Custom resources".into(), custom));
    groups.push(("Other".into(), other));
    for (_, items) in &mut groups {
        items.retain(|t| {
            model::matches(
                query,
                &format!(
                    "{} {} {} {} {}",
                    t.resource.kind,
                    model::plural(&t.resource.kind),
                    t.resource.plural,
                    t.resource.group,
                    spec["short"][&t.resource.plural].as_str().unwrap_or_default()
                ),
            )
        });
    }
    groups.retain(|(_, items)| !items.is_empty());
    groups
}
