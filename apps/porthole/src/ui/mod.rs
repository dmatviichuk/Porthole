mod dialogs;
mod editor;
mod fonts;
mod icons;
mod logs;
mod platform;
mod style;
mod table;
mod terminal;
mod views;
mod yaml;
use crate::{
    data::{Data, Message},
    model::{self, ClusterLabel, Prefs, Tab, Theme, View},
    resources::ResourceType,
    summary::ResourceRow,
};
use eframe::egui::{self, *};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use style::*;

#[derive(Clone)]
enum Action {
    Go(View),
    Navigate(View),
    Replace(View),
    Namespace(Option<String>),
    Browse,
    Context(String),
    Theme(Theme),
    Basis(String, String),
    Reload,
    Rediscover,
    Back,
    Forward,
    Overlay(Overlay),
    Label(String),
    Delete(ResourceType, Option<String>, String),
    Copy(String),
    Shell(ResourceRow, String),
    ActiveShell(u64),
    CloseShell(u64),
    ResetColumns(String),
    Menu(Vec<MenuItem>, Pos2),
}
#[derive(Clone)]
struct MenuItem {
    label: String,
    action: Option<Action>,
    enabled: bool,
    children: Vec<MenuItem>,
    checked: Option<bool>,
}
fn shell_menu(pod: &ResourceRow, container: Option<&str>) -> Option<MenuItem> {
    let containers = model::containers(pod);
    if let Some(name) = container {
        let selected = containers.iter().find(|c| c.name == name)?;
        let mut item = MenuItem::action("Open shell", Action::Shell(pod.clone(), name.into()));
        item.enabled = selected.state == "Running";
        return Some(item);
    }
    let running: Vec<_> = containers.into_iter().filter(|c| c.state == "Running").collect();
    match running.as_slice() {
        [] => None,
        [only] => Some(MenuItem::action(
            "Open shell",
            Action::Shell(pod.clone(), only.name.clone()),
        )),
        _ => Some(MenuItem {
            label: "Open shell".into(),
            action: None,
            enabled: true,
            checked: None,
            children: running
                .iter()
                .map(|c| MenuItem::action(&c.name, Action::Shell(pod.clone(), c.name.clone())))
                .collect(),
        }),
    }
}
impl MenuItem {
    fn action(label: impl Into<String>, action: Action) -> Self {
        Self {
            label: label.into(),
            action: Some(action),
            enabled: true,
            children: vec![],
            checked: None,
        }
    }
    fn separator() -> Self {
        Self {
            label: String::new(),
            action: None,
            enabled: false,
            children: vec![],
            checked: None,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Overlay {
    Palette,
    Shortcuts,
}
struct Delete {
    context: String,
    resource: ResourceType,
    namespace: Option<String>,
    name: String,
    typed: String,
    busy: bool,
}
struct LabelDraft {
    context: String,
    draft: ClusterLabel,
}
struct Notice {
    text: String,
    error: bool,
    until: Instant,
}
struct Shell {
    key: u64,
    context: String,
    namespace: String,
    pod: String,
    container: String,
    session: u64,
    parser: vt100::Parser,
    ended: bool,
    cols: u16,
    rows: u16,
    selection: Option<((u16, u16), (u16, u16))>,
}
pub struct App {
    platform: platform::Platform,
    data: Data,
    prefs: Prefs,
    context: String,
    namespace: Option<String>,
    history: Vec<View>,
    index: usize,
    search: String,
    type_query: String,
    icons: Icons,
    t: Tokens,
    tables: HashMap<String, table::TableState>,
    actions: Vec<Action>,
    menu: Option<(Vec<MenuItem>, Pos2)>,
    overlay: Option<Overlay>,
    palette_query: String,
    palette_cursor: usize,
    overlay_fresh: bool,
    label_draft: Option<LabelDraft>,
    delete: Option<Delete>,
    notices: Vec<Notice>,
    shells: Vec<Shell>,
    active_shell: Option<u64>,
    shell_focus: bool,
    next_id: u64,
    basis: HashMap<String, String>,
    expanded: HashSet<String>,
    logs: logs::State,
    yaml: yaml::State,
    page_key: String,
    focus_search: bool,
    prefs_json: String,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        crate::preferences::import_window(cc);
        let prefs = cc
            .storage
            .and_then(|s| s.get_string("porthole.preferences"))
            .and_then(|s| serde_json::from_str::<Prefs>(&s).ok())
            .or_else(crate::preferences::legacy)
            .unwrap_or_default();
        let data = Data::new(cc.egui_ctx.clone());
        let context = data
            .contexts
            .as_ref()
            .ok()
            .and_then(|c| {
                [
                    prefs.context.as_ref(),
                    c.current.as_ref(),
                    c.contexts.first().map(|c| &c.name),
                ]
                .into_iter()
                .flatten()
                .find(|name| c.contexts.iter().any(|c| &c.name == *name))
                .cloned()
            })
            .unwrap_or_default();
        let namespace = prefs.namespaces.get(&context).cloned().flatten();
        let t = Tokens::new(match prefs.theme {
            Theme::Light => false,
            Theme::Dark => true,
            Theme::Auto => cc.egui_ctx.system_theme() == Some(egui::Theme::Dark),
        });
        let mut app = Self {
            platform: platform::Platform::new(cc),
            data,
            prefs,
            context,
            namespace,
            history: vec![View::Applications],
            index: 0,
            search: String::new(),
            type_query: String::new(),
            icons: Icons::new(),
            t,
            tables: HashMap::new(),
            actions: vec![],
            menu: None,
            overlay: None,
            palette_query: String::new(),
            palette_cursor: 0,
            overlay_fresh: false,
            label_draft: None,
            delete: None,
            notices: vec![],
            shells: vec![],
            active_shell: None,
            shell_focus: false,
            next_id: 0,
            basis: HashMap::new(),
            expanded: HashSet::new(),
            logs: logs::State::default(),
            yaml: yaml::State::default(),
            page_key: String::new(),
            focus_search: false,
            prefs_json: String::new(),
        };
        if !app.context.is_empty() {
            app.data.warm(&app.context);
        }
        app
    }
    fn view(&self) -> View {
        self.history[self.index].clone()
    }
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
    fn navigate(&mut self, v: View) {
        self.history.truncate(self.index + 1);
        self.history.push(v);
        self.index += 1;
        self.search.clear();
        self.shell_focus = false;
    }
    fn resource_view(r: ResourceType, row: &ResourceRow, tab: Tab) -> View {
        View::Resource {
            resource: r,
            namespace: row.namespace.clone(),
            object: row.name.clone(),
            tab,
            log_container: None,
        }
    }
    fn row_logs(row: &table::Row) -> Option<View> {
        let resource = row.resource.as_ref()?;
        if !model::has_logs(&resource.kind) {
            return None;
        }
        Some(View::Resource {
            resource: resource.clone(),
            namespace: row.object.namespace.clone(),
            object: row.object.name.clone(),
            tab: Tab::Logs,
            log_container: row.container.clone(),
        })
    }
    fn notify(&mut self, text: String, error: bool) {
        if self.notices.len() >= 4 {
            self.notices.remove(0);
        }
        self.notices.push(Notice {
            text,
            error,
            until: Instant::now() + Duration::from_secs(if error { 8 } else { 4 }),
        });
    }
    fn execute(&mut self, ctx: &Context, action: Action) {
        match action {
            Action::Go(v) => {
                if std::mem::discriminant(&v) == std::mem::discriminant(&self.view()) {
                    self.shell_focus = false;
                    ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                } else {
                    self.navigate(v);
                }
            }
            Action::Navigate(v) => self.navigate(v),
            Action::Replace(v) => {
                self.history[self.index] = v;
                self.shell_focus = false;
            }
            Action::Back => {
                self.index = self.index.saturating_sub(1);
                self.search.clear();
                self.shell_focus = false;
            }
            Action::Forward => {
                self.index = (self.index + 1).min(self.history.len() - 1);
                self.search.clear();
                self.shell_focus = false;
            }
            Action::Namespace(ns) => {
                self.namespace = ns.clone();
                self.prefs.namespaces.insert(self.context.clone(), ns);
                if !matches!(
                    self.view(),
                    View::Applications | View::AllResources | View::Resources(_)
                ) {
                    self.navigate(View::Applications);
                }
            }
            Action::Browse => {
                self.namespace = None;
                self.prefs.namespaces.insert(self.context.clone(), None);
                if self.view() != View::Applications {
                    self.navigate(View::Applications);
                } else {
                    self.search.clear();
                }
            }
            Action::Context(c) => {
                self.context = c;
                self.prefs.context = Some(self.context.clone());
                self.namespace = self.prefs.namespaces.get(&self.context).cloned().flatten();
                self.history = vec![View::Applications];
                self.index = 0;
                self.search.clear();
                self.shell_focus = false;
                self.data.warm(&self.context);
            }
            Action::Theme(theme) => self.prefs.theme = theme,
            Action::Basis(id, value) => {
                self.basis.insert(id, value);
            }
            Action::Reload => {
                self.data.reload();
                if let Ok(c) = &self.data.contexts
                    && !c.contexts.iter().any(|c| c.name == self.context)
                {
                    self.context = c
                        .current
                        .clone()
                        .or_else(|| c.contexts.first().map(|c| c.name.clone()))
                        .unwrap_or_default();
                }
                if !self.context.is_empty() {
                    self.data.warm(&self.context);
                }
            }
            Action::Rediscover => self.data.discover(&self.context, true),
            Action::Overlay(o) => {
                self.overlay = Some(o);
                self.overlay_fresh = true;
                self.palette_query.clear();
                self.palette_cursor = 0;
            }
            Action::Label(c) => {
                self.label_draft = Some(LabelDraft {
                    draft: self.prefs.labels.get(&c).cloned().unwrap_or_default(),
                    context: c,
                });
                self.overlay_fresh = true;
            }
            Action::Delete(r, ns, name) => {
                self.delete = Some(Delete {
                    context: self.context.clone(),
                    resource: r,
                    namespace: ns,
                    name,
                    typed: String::new(),
                    busy: false,
                });
                self.overlay_fresh = true;
            }
            Action::Copy(name) => {
                ctx.copy_text(name.clone());
                self.notify(format!("Copied {name}"), false);
            }
            Action::Shell(pod, container) => {
                let key = self.id();
                let namespace = pod.namespace.clone().unwrap_or_else(|| "default".into());
                let session = self.data.shell(
                    key,
                    self.context.clone(),
                    namespace.clone(),
                    pod.name.clone(),
                    container.clone(),
                    120,
                    12,
                );
                self.shells.push(Shell {
                    key,
                    context: self.context.clone(),
                    namespace,
                    pod: pod.name,
                    container,
                    session,
                    parser: vt100::Parser::new(12, 120, 10000),
                    ended: false,
                    cols: 120,
                    rows: 12,
                    selection: None,
                });
                self.active_shell = Some(key);
                self.shell_focus = true;
                ctx.memory_mut(|m| m.request_focus(Id::new(("terminal", key))));
            }
            Action::ActiveShell(key) => {
                self.active_shell = Some(key);
                self.shell_focus = true;
                ctx.memory_mut(|m| m.request_focus(Id::new(("terminal", key))));
            }
            Action::CloseShell(key) => {
                if let Some(at) = self.shells.iter().position(|s| s.key == key) {
                    let shell = self.shells.remove(at);
                    self.data.sessions.stop(shell.session);
                    if self.active_shell == Some(key) {
                        self.active_shell = self
                            .shells
                            .get(at.min(self.shells.len().saturating_sub(1)))
                            .map(|s| s.key);
                    }
                    if self.shells.is_empty() {
                        self.shell_focus = false;
                    } else if self.shell_focus
                        && let Some(key) = self.active_shell
                    {
                        ctx.memory_mut(|m| m.request_focus(Id::new(("terminal", key))));
                    }
                }
            }
            Action::ResetColumns(id) => {
                self.prefs.columns.remove(&id);
            }
            Action::Menu(items, pos) => {
                if cfg!(target_os = "macos") {
                    if let Some(a) = self.platform.popup(ctx, &items, pos) {
                        self.execute(ctx, a);
                    }
                } else {
                    self.menu = Some((items, pos));
                }
            }
        }
    }
    fn process(&mut self) {
        self.data.drain();
        for msg in std::mem::take(&mut self.data.pending) {
            match msg {
                Message::Yaml(token, r) if token == self.yaml.token => match r {
                    Ok(yaml) => {
                        self.yaml.loaded = Some(yaml.clone());
                        self.yaml.draft = yaml;
                        self.yaml.error = None;
                    }
                    Err(e) => self.yaml.error = Some(e),
                },
                Message::Saved(token, r) if token == self.yaml.token => {
                    self.yaml.saving = false;
                    match r {
                        Ok(()) => {
                            if let View::Resource {
                                resource,
                                namespace,
                                object,
                                ..
                            } = self.view()
                            {
                                self.notify(format!("Saved {} {object}", resource.kind.to_lowercase()), false);
                                self.reload_yaml(resource, namespace, object);
                            }
                        }
                        Err(e) => self.yaml.error = Some(e),
                    }
                }
                Message::Deleted(name, r) => {
                    if let Some(d) = self.delete.as_mut() {
                        d.busy = false;
                    }
                    match r {
                        Ok(()) => {
                            self.delete = None;
                            self.notify(format!("Deleted {name}"), false);
                        }
                        Err(e) => self.notify(format!("Could not delete {name}: {e}"), true),
                    }
                }
                Message::Log(token, key, event) if token == self.logs.token => self.logs.receive(key, event),
                Message::ShellBytes(key, bytes) => {
                    if let Some(s) = self.shells.iter_mut().find(|s| s.key == key) {
                        s.parser.process(&bytes);
                    }
                }
                Message::ShellEvent(key, event) => {
                    if let Some(s) = self.shells.iter_mut().find(|s| s.key == key) {
                        let message = match event {
                            crate::exec::ExecEvent::Connected => None,
                            crate::exec::ExecEvent::Exited { code, message } => Some(
                                code.map(|c| format!("Shell exited with code {c}."))
                                    .unwrap_or_else(|| format!("Shell ended. {}", message.unwrap_or_default())),
                            ),
                            crate::exec::ExecEvent::Error { message } => {
                                Some(format!("Could not open a shell: {message}"))
                            }
                        };
                        if let Some(message) = message {
                            s.ended = true;
                            s.parser.process(format!("\r\n\x1b[2m{message}\x1b[0m\r\n").as_bytes());
                        }
                    }
                }
                _ => {}
            }
        }
        self.notices.retain(|n| Instant::now() < n.until);
    }
    fn shortcuts(&mut self, ctx: &Context) {
        if self.overlay.is_some() || self.delete.is_some() || self.label_draft.is_some() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let typing = ctx.text_edit_focused();
        for e in events {
            match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let action = if modifiers.ctrl && !modifiers.mac_cmd && key == Key::Backtick {
                        self.shell_focus = !self.shell_focus && self.active_shell.is_some();
                        if let Some(key) = self.active_shell {
                            ctx.memory_mut(|m| {
                                let id = Id::new(("terminal", key));
                                if self.shell_focus {
                                    m.request_focus(id);
                                } else {
                                    m.surrender_focus(id);
                                }
                            });
                        }
                        None
                    } else if modifiers.ctrl && !modifiers.mac_cmd && key == Key::Tab {
                        self.step_tab(modifiers.shift);
                        None
                    } else if modifiers.command && !modifiers.shift && !modifiers.alt && !self.shell_focus {
                        match key {
                            Key::K => Some(Action::Overlay(Overlay::Palette)),
                            Key::Num1 => Some(Action::Go(View::Applications)),
                            Key::Num2 => Some(Action::Go(View::AllResources)),
                            Key::Num3 => Some(Action::Go(View::Overview)),
                            Key::OpenBracket if !matches!(self.view(), View::Resource { tab: Tab::Yaml, .. }) => {
                                Some(Action::Back)
                            }
                            Key::CloseBracket if !matches!(self.view(), View::Resource { tab: Tab::Yaml, .. }) => {
                                Some(Action::Forward)
                            }
                            Key::F => {
                                self.focus_search = true;
                                None
                            }
                            _ => None,
                        }
                    } else if !typing && modifiers.is_none() && !self.shell_focus && key == Key::Slash {
                        self.focus_search = true;
                        None
                    } else {
                        None
                    };
                    if let Some(a) = action {
                        self.actions.push(a);
                        ctx.input_mut(|i| {
                            i.consume_key(modifiers, key);
                        });
                    }
                }
                egui::Event::Text(text) if text == "?" && !typing && !self.shell_focus => {
                    self.actions.push(Action::Overlay(Overlay::Shortcuts))
                }
                _ => {}
            }
        }
    }
    fn step_tab(&mut self, previous: bool) {
        match self.view() {
            View::Applications => self.navigate(View::AllResources),
            View::AllResources | View::Resources(_) => self.navigate(View::Applications),
            View::Resource {
                resource,
                namespace,
                object,
                tab,
                ..
            } => {
                let tabs = if model::has_logs(&resource.kind) {
                    vec![Tab::Overview, Tab::Logs, Tab::Events, Tab::Yaml]
                } else {
                    vec![Tab::Overview, Tab::Events, Tab::Yaml]
                };
                let at = tabs.iter().position(|t| *t == tab).unwrap_or(0);
                let next = if previous {
                    (at + tabs.len() - 1) % tabs.len()
                } else {
                    (at + 1) % tabs.len()
                };
                self.history[self.index] = View::Resource {
                    resource,
                    namespace,
                    object,
                    tab: tabs[next],
                    log_container: None,
                };
            }
            _ => {}
        }
    }
    fn row_menu(&self, row: &table::Row) -> Vec<MenuItem> {
        let Some(r) = row.resource.as_ref() else {
            return vec![];
        };
        let obj = &row.object;
        let mut list = vec![];
        if row.container.is_none() {
            list.push(MenuItem::action(
                "Open",
                Action::Navigate(Self::resource_view(r.clone(), obj, Tab::Overview)),
            ));
        }
        if let Some(view) = Self::row_logs(row) {
            list.push(MenuItem::action("View logs", Action::Navigate(view)));
        }
        if r.kind == "Pod"
            && let Some(item) = shell_menu(obj, row.container.as_deref())
        {
            list.push(item);
        }
        if row.container.is_some() {
            return list;
        }
        list.push(MenuItem::action(
            "Edit YAML",
            Action::Navigate(Self::resource_view(r.clone(), obj, Tab::Yaml)),
        ));
        list.push(MenuItem::separator());
        list.push(MenuItem::action("Copy name", Action::Copy(obj.name.clone())));
        list.push(MenuItem::separator());
        let mut delete = MenuItem::action(
            format!("Delete {}…", dialogs::kind_label(&r.kind)),
            Action::Delete(r.clone(), obj.namespace.clone(), obj.name.clone()),
        );
        delete.enabled = self
            .data
            .context_data
            .get(&self.context)
            .and_then(|d| {
                d.types
                    .iter()
                    .find(|t| model::type_key(&t.resource) == model::type_key(r))
            })
            .is_none_or(|t| t.verbs.iter().any(|v| v == "delete"));
        list.push(delete);
        list
    }
    #[allow(clippy::too_many_arguments)]
    fn show_table(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        id: &str,
        cols: &[table::Column],
        rows: &[table::Row],
        primary: bool,
        fit: bool,
        height: f32,
        empty: &str,
        menus: bool,
        open: bool,
    ) {
        let search = self.search.clone();
        let events = table::show(
            ui,
            r,
            cols,
            rows,
            self.tables.entry(id.into()).or_default(),
            &mut self.prefs,
            &mut self.icons,
            self.t,
            table::Options {
                id,
                primary,
                fit,
                height,
                empty,
                search: &search,
                initial_sort: if id.contains("events") { "lastSeen" } else { "name" },
                menus,
                open,
            },
        );
        for event in events {
            match event {
                table::Event::ResetColumns => self.actions.push(Action::ResetColumns(id.into())),
                table::Event::Menu(i, pos) => self.actions.push(Action::Menu(self.row_menu(&rows[i]), pos)),
                table::Event::Copy(i) => self.actions.push(Action::Copy(rows[i].object.name.clone())),
                table::Event::Open(i) => {
                    if let Some(r) = &rows[i].resource {
                        self.actions.push(Action::Navigate(Self::resource_view(
                            r.clone(),
                            &rows[i].object,
                            Tab::Overview,
                        )));
                    }
                }
                table::Event::Logs(i) => {
                    if let Some(view) = Self::row_logs(&rows[i]) {
                        self.actions.push(Action::Navigate(view));
                    }
                }
                table::Event::Yaml(i) => {
                    if let Some(r) = &rows[i].resource {
                        self.actions.push(Action::Navigate(Self::resource_view(
                            r.clone(),
                            &rows[i].object,
                            Tab::Yaml,
                        )));
                    }
                }
                table::Event::Shell(i) => {
                    let menu = self.row_menu(&rows[i]);
                    if let Some(item) = menu.into_iter().find(|m| m.label == "Open shell" && m.enabled) {
                        if let Some(a) = item.action {
                            self.actions.push(a);
                        } else {
                            self.actions.push(Action::Menu(item.children, r.left_top()));
                        }
                    }
                }
            }
        }
    }
    fn topbar(&mut self, ui: &mut Ui, r: Rect) {
        let t = self.t;
        let drag = ui.interact(r, ui.id().with("titlebar"), Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(
                !ui.input(|i| i.viewport().maximized.unwrap_or(false)),
            ));
        }
        for (i, (icon, enabled, action)) in [
            ("chevron-left", self.index > 0, Action::Back),
            ("chevron-right", self.index + 1 < self.history.len(), Action::Forward),
        ]
        .into_iter()
        .enumerate()
        {
            let rr = rect(r.left() + 16.0 + i as f32 * 29.0, r.top() + 13.5, 25.0, 25.0);
            let response = ui.interact(rr, ui.id().with(("navigation", i)), Sense::click());
            self.icons.paint(
                ui,
                icon,
                rr.shrink(4.0),
                if enabled { t.muted } else { t.muted.gamma_multiply(0.35) },
            );
            if enabled && response.clicked() {
                self.actions.push(action);
            }
        }
        let view = self.view();
        let tabs: Vec<(&str, Action, bool)> = match &view {
            View::Applications | View::AllResources | View::Resources(_) => vec![
                (
                    "Applications",
                    Action::Navigate(View::Applications),
                    matches!(view, View::Applications),
                ),
                (
                    "All Resources",
                    Action::Navigate(View::AllResources),
                    matches!(view, View::AllResources | View::Resources(_)),
                ),
            ],
            View::Resource {
                resource,
                namespace,
                object,
                tab,
                ..
            } => [Tab::Overview, Tab::Logs, Tab::Events, Tab::Yaml]
                .into_iter()
                .filter(|t| *t != Tab::Logs || model::has_logs(&resource.kind))
                .map(|t| {
                    let name = match t {
                        Tab::Overview => "Overview",
                        Tab::Logs => "Logs",
                        Tab::Events => "Events",
                        Tab::Yaml => "YAML",
                    };
                    (
                        name,
                        Action::Replace(View::Resource {
                            resource: resource.clone(),
                            namespace: namespace.clone(),
                            object: object.clone(),
                            tab: t,
                            log_container: None,
                        }),
                        t == *tab,
                    )
                })
                .collect(),
            _ => vec![],
        };
        if !tabs.is_empty() {
            let width = tabs
                .iter()
                .map(|(n, _, active)| text_width(ui, n, 12.5, if *active { "medium" } else { "sans" }) + 32.0)
                .sum::<f32>()
                + 6.0;
            let rr = rect(r.center().x - width / 2.0, r.top() + 10.75, width, 30.5);
            ui.painter().rect_filled(rr, 8, t.raised);
            ui.painter()
                .rect_stroke(rr, 8, Stroke::new(1.0, t.line), StrokeKind::Inside);
            let mut x = rr.left() + 3.0;
            for (name, action, active) in tabs {
                let family = if active { "medium" } else { "sans" };
                let w = text_width(ui, name, 12.5, family) + 32.0;
                let response = button_with_font(
                    ui,
                    rect(x, rr.top() + 3.0, w, 24.5),
                    ("tab", name),
                    name,
                    12.5,
                    family,
                    if active { t.text } else { t.muted },
                    active.then_some(t.selected),
                    None,
                    t,
                );
                if response.clicked() {
                    self.actions.push(action);
                }
                x += w;
            }
        }
        if matches!(view, View::Applications | View::AllResources | View::Resources(_)) {
            let rr = rect(r.right() - 236.0, r.top() + 12.0, 220.0, 28.0);
            let response = search(ui, rr, &mut self.search, "Search", "main-search", t);
            self.icons.paint(
                ui,
                "search",
                rect(rr.left() + 8.0, rr.center().y - 6.5, 13.0, 13.0),
                t.muted,
            );
            if self.focus_search {
                response.request_focus();
                self.focus_search = false;
            }
            if response.has_focus() {
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    if self.search.is_empty() {
                        response.surrender_focus();
                    } else {
                        self.search.clear();
                    }
                }
                if ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::ArrowDown)) {
                    response.surrender_focus();
                }
            }
        }
    }
    fn sidebar(&mut self, ui: &mut Ui, r: Rect) {
        let t = self.t;
        ui.painter().rect_filled(r, 0, t.sidebar);
        line(
            ui,
            r.right_top() - vec2(0.5, 0.0),
            r.right_bottom() - vec2(0.5, 0.0),
            t.line,
        );
        let r = Rect::from_min_max(r.min, r.max - vec2(1.0, 0.0));
        let handle = ui
            .interact(
                rect(r.right() - 4.0, r.top(), 8.0, r.height()),
                ui.id().with("sidebar-resize"),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(CursorIcon::ResizeHorizontal);
        if handle.dragged() {
            self.prefs.sidebar_width = (self.prefs.sidebar_width + ui.input(|i| i.pointer.delta().x))
                .clamp(200.0, 560.0)
                .round();
        }
        if handle.double_clicked() {
            self.prefs.sidebar_width = 236.0;
        }
        if handle.has_focus() {
            if ui.input(|i| i.key_pressed(Key::ArrowLeft)) {
                self.prefs.sidebar_width = (self.prefs.sidebar_width - 16.0).max(200.0);
            }
            if ui.input(|i| i.key_pressed(Key::ArrowRight)) {
                self.prefs.sidebar_width = (self.prefs.sidebar_width + 16.0).min(560.0);
            }
        }
        let traffic = ui.interact(
            rect(r.left(), r.top(), r.width(), 48.0),
            ui.id().with("sidebar-titlebar"),
            Sense::drag(),
        );
        if traffic.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        let ns = self
            .data
            .rows(&self.context, &model::resource("Namespace"), None, None, false);
        let cluster = rect(r.left() + 10.0, r.top() + 48.0, r.width() - 20.0, 47.6);
        let response = ui.interact(cluster, ui.id().with("clusters"), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &self.context));
        if response.hovered() {
            ui.painter().rect_filled(cluster, 6, t.selected);
        }
        label(
            ui,
            rect(cluster.left() + 10.0, cluster.top() + 6.0, cluster.width() - 44.0, 18.2),
            if self.context.is_empty() {
                "No cluster"
            } else {
                &self.context
            },
            13.0,
            "semibold",
            t.text,
        );
        let (state, color) = if let Some(e) = &ns.error {
            if e.to_lowercase().contains("forbidden") {
                ("Connected", t.ok)
            } else {
                ("Can't connect", t.failed)
            }
        } else if ns.synced {
            ("Connected", t.ok)
        } else {
            ("Connecting…", t.faint)
        };
        ui.painter()
            .circle_filled(pos2(cluster.left() + 13.0, cluster.top() + 33.7), 3.0, color);
        label(
            ui,
            rect(cluster.left() + 22.0, cluster.top() + 26.2, 92.0, 15.4),
            state,
            11.0,
            "sans",
            t.muted,
        );
        if let Some(l) = self.prefs.labels.get(&self.context)
            && !l.tag.is_empty()
        {
            label(
                ui,
                rect(
                    cluster.left() + 22.0 + text_width(ui, state, 11.0, "sans") + 6.0,
                    cluster.top() + 26.2,
                    6.0,
                    15.4,
                ),
                "·",
                11.0,
                "sans",
                t.faint,
            );
            label(
                ui,
                rect(
                    cluster.left()
                        + 22.0
                        + text_width(ui, state, 11.0, "sans")
                        + 6.0
                        + text_width(ui, "·", 11.0, "sans")
                        + 6.0,
                    cluster.top() + 26.2,
                    cluster.width() - 123.0,
                    15.4,
                ),
                &l.tag,
                11.0,
                "medium",
                label_color(&l.color),
            );
        }
        self.icons.paint(
            ui,
            "chevrons-up-down",
            rect(cluster.right() - 24.0, cluster.center().y - 7.0, 14.0, 14.0),
            t.muted,
        );
        if response.clicked() || response.secondary_clicked() {
            let mut menu = vec![];
            if let Ok(c) = &self.data.contexts {
                for c in &c.contexts {
                    let label = self
                        .prefs
                        .labels
                        .get(&c.name)
                        .filter(|l| !l.tag.is_empty())
                        .map(|l| format!("{}  ·  {}", c.name, l.tag))
                        .unwrap_or_else(|| c.name.clone());
                    let mut item = MenuItem::action(label, Action::Context(c.name.clone()));
                    item.checked = Some(c.name == self.context);
                    menu.push(item);
                }
            }
            menu.push(MenuItem::separator());
            if !self.context.is_empty() {
                menu.push(MenuItem::action(
                    if self.prefs.labels.contains_key(&self.context) {
                        "Edit label…"
                    } else {
                        "Label this cluster…"
                    },
                    Action::Label(self.context.clone()),
                ));
            }
            menu.push(MenuItem::action("Reload kubeconfig", Action::Reload));
            self.actions.push(Action::Menu(menu, cluster.left_bottom()));
        }
        let view = self.view();
        let y = r.top() + 111.6;
        self.nav_item(
            ui,
            rect(r.left() + 10.0, y, r.width() - 20.0, 28.2),
            "Browse",
            "layout-grid",
            view != View::Overview,
            Action::Browse,
        );
        self.nav_item(
            ui,
            rect(r.left() + 10.0, y + 30.2, r.width() - 20.0, 28.2),
            "Overview",
            "chart-no-axes-column",
            view == View::Overview,
            Action::Navigate(View::Overview),
        );
        label(
            ui,
            rect(r.left() + 20.0, y + 78.4, r.width() - 40.0, 15.4),
            "Namespaces",
            11.0,
            "medium",
            t.muted,
        );
        let all = rect(r.left() + 10.0, y + 100.0, r.width() - 20.0, 28.2);
        self.nav_item(
            ui,
            all,
            "All namespaces",
            "layers",
            self.namespace.is_none(),
            Action::Namespace(None),
        );
        let list = rect(
            r.left(),
            all.bottom() + 2.0,
            r.width(),
            (r.bottom() - 44.0 - all.bottom() - 2.0).max(0.0),
        );
        let mut names: Vec<_> = ns.rows.iter().map(|r| r.name.clone()).collect();
        names.sort();
        ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
            ui.set_clip_rect(list);
            ScrollArea::vertical()
                .id_salt("namespaces")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for name in names {
                        let origin = ui.cursor().min;
                        let width = ui.available_width();
                        let rr = rect(origin.x + 10.0, origin.y, width - 20.0, 28.2);
                        ui.allocate_space(vec2(width, 28.2));
                        let response = self.nav_item(
                            ui,
                            rr,
                            &name,
                            "folder",
                            self.namespace.as_ref() == Some(&name),
                            Action::Namespace(Some(name.clone())),
                        );
                        if response.secondary_clicked() {
                            self.actions.push(Action::Menu(
                                vec![
                                    MenuItem::action("Show only this namespace", Action::Namespace(Some(name.clone()))),
                                    MenuItem::action("Copy name", Action::Copy(name.clone())),
                                    MenuItem::separator(),
                                    MenuItem::action(
                                        "Delete namespace…",
                                        Action::Delete(model::resource("Namespace"), None, name),
                                    ),
                                ],
                                rr.left_bottom(),
                            ));
                        }
                    }
                    ui.allocate_space(vec2(ui.available_width(), 12.0));
                });
        });
        let footer = rect(r.left(), r.bottom() - 44.0, r.width(), 44.0);
        line(ui, footer.left_top(), footer.right_top(), t.line);
        let mut x = r.left() + 12.0;
        ui.painter().rect_filled(
            rect(x, footer.top() + 8.0, 174.0, 27.4),
            6,
            t.selected.gamma_multiply(0.6),
        );
        for (theme, name, icon, w) in [
            (Theme::Light, "Light", "sun", 57.0),
            (Theme::Dark, "Dark", "moon", 57.0),
            (Theme::Auto, "Auto", "monitor", 56.0),
        ] {
            let rr = rect(x + 2.0, footer.top() + 10.0, w, 23.4);
            let response = ui.interact(rr, ui.id().with(("theme", name)), Sense::click());
            response
                .widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, self.prefs.theme == theme, name));
            if self.prefs.theme == theme {
                ui.painter().rect_filled(rr, 4, t.bg);
            }
            self.icons.paint(
                ui,
                icon,
                rect(rr.left() + 8.0, rr.center().y - 6.0, 12.0, 12.0),
                if self.prefs.theme == theme { t.text } else { t.muted },
            );
            label(
                ui,
                rect(rr.left() + 24.0, rr.top(), w - 26.0, rr.height()),
                name,
                11.0,
                "sans",
                if self.prefs.theme == theme { t.text } else { t.muted },
            );
            if response.clicked() {
                self.actions.push(Action::Theme(theme));
            }
            x += w;
        }
        for (n, icon, a) in [
            (0, "rotate-cw", Action::Reload),
            (1, "keyboard", Action::Overlay(Overlay::Shortcuts)),
        ] {
            let rr = rect(r.right() - 12.0 - 26.0 * (n + 1) as f32, footer.top() + 9.0, 26.0, 26.0);
            let response = ui.interact(rr, ui.id().with(("footer", icon)), Sense::click());
            let title = if n == 0 {
                "Reload kubeconfig"
            } else {
                "Keyboard shortcuts"
            };
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, title));
            let response = response.on_hover_text(title);
            if response.hovered() {
                ui.painter().rect_filled(rr, 6, t.selected);
            }
            self.icons.paint(ui, icon, rr.shrink(6.0), t.muted);
            if response.clicked() {
                self.actions.push(a);
            }
        }
    }
    fn nav_item(&mut self, ui: &mut Ui, r: Rect, name: &str, icon: &str, active: bool, action: Action) -> Response {
        let t = self.t;
        let response = ui.interact(r, ui.id().with(("nav", name)), Sense::click());
        if active || response.hovered() {
            ui.painter()
                .rect_filled(r, 6, if active { t.selected } else { t.hover });
        }
        self.icons.paint(
            ui,
            icon,
            rect(r.left() + 10.0, r.center().y - 7.0, 14.0, 14.0),
            if active { t.text } else { t.muted },
        );
        label(
            ui,
            rect(r.left() + 34.0, r.top(), r.width() - 44.0, r.height()),
            name,
            13.0,
            if active { "medium" } else { "sans" },
            t.text,
        );
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name));
        if response.clicked() {
            self.actions.push(action);
        }
        response
    }
}
impl eframe::App for App {
    fn logic(&mut self, _ctx: &Context, _frame: &mut eframe::Frame) {
        self.process();
    }
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.platform.process_edits(&ctx);
        let dark = match self.prefs.theme {
            Theme::Dark => true,
            Theme::Light => false,
            Theme::Auto => ctx.system_theme() == Some(egui::Theme::Dark),
        };
        self.t = Tokens::new(dark);
        self.t.apply(&ctx, self.prefs.theme);
        self.shortcuts(&ctx);
        let r = ui.max_rect();
        ui.painter().rect_filled(r, 0, self.t.bg);
        let sidebar = rect(r.left(), r.top(), self.prefs.sidebar_width, r.height());
        self.sidebar(ui, sidebar);
        let main = rect(sidebar.right(), r.top(), r.width() - sidebar.width(), r.height());
        self.topbar(ui, rect(main.left(), main.top(), main.width(), 52.0));
        let dock = if self.shells.is_empty() {
            0.0
        } else {
            self.prefs.dock_height.min(r.height() * 0.8)
        };
        let body = rect(
            main.left(),
            main.top() + 52.0,
            main.width(),
            main.height() - 52.0 - dock,
        );
        if let Err(e) = &self.data.contexts {
            views::placeholder(
                ui,
                body,
                &format!("Could not read your kubeconfig: {e}. Porthole reads $KUBECONFIG or ~/.kube/config."),
                self.t,
            );
        } else if self.data.contexts.as_ref().is_ok_and(|c| c.contexts.is_empty()) {
            views::placeholder(
                ui,
                body,
                "No contexts in your kubeconfig. Add a cluster with kubectl, then reload.",
                self.t,
            );
        } else {
            self.current_view(ui, body);
        }
        if dock > 0.0 {
            self.dock(ui, rect(main.left(), r.bottom() - dock, main.width(), dock));
        }
        self.show_menu(&ctx);
        self.dialogs(&ctx);
        self.show_notices(&ctx, r);
        for a in std::mem::take(&mut self.actions) {
            self.execute(&ctx, a);
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        let json = serde_json::to_string(&self.prefs).unwrap_or_default();
        if json != self.prefs_json {
            self.prefs_json = json;
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(json) = serde_json::to_string(&self.prefs) {
            storage.set_string("porthole.preferences", json);
        }
    }
    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(5)
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    pub(super) fn two_container_pod() -> ResourceRow {
        serde_json::from_value(serde_json::json!({
            "uid": "pod-uid", "name": "two-containers", "namespace": "test",
            "created": null, "owner": null, "labels": {}, "status": "Running",
            "health": "ok", "fields": {"containers": [
                {"name": "application", "image": "app:1", "init": false, "ready": true,
                 "restarts": 0, "state": "Running", "requests": {"cpu": 0, "memory": 0}, "limits": {"cpu": 0, "memory": 0}},
                {"name": "sidecar", "image": "sidecar:1", "init": false, "ready": true,
                 "restarts": 0, "state": "Running", "requests": {"cpu": 0, "memory": 0}, "limits": {"cpu": 0, "memory": 0}}
            ]}
        })).unwrap()
    }

    #[test]
    fn container_log_action_retains_the_selected_container() {
        let mut row = table::Row::new(model::resource("Pod"), two_container_pod());
        for container in [None, Some("application"), Some("sidecar")] {
            row.container = container.map(String::from);
            match App::row_logs(&row).unwrap() {
                View::Resource {
                    namespace,
                    object,
                    tab,
                    log_container,
                    ..
                } => {
                    assert_eq!(namespace.as_deref(), Some("test"));
                    assert_eq!(object, "two-containers");
                    assert_eq!(tab, Tab::Logs);
                    assert_eq!(log_container.as_deref(), container);
                }
                _ => panic!("logs must open the resource's Logs tab"),
            }
        }
    }

    #[test]
    fn container_shell_targets_selected_container_without_navigation() {
        let pod = two_container_pod();
        for name in ["application", "sidecar"] {
            let menu = shell_menu(&pod, Some(name)).unwrap();
            assert!(menu.enabled && menu.children.is_empty());
            match menu.action.unwrap() {
                Action::Shell(target, container) => {
                    assert_eq!(target.name, "two-containers");
                    assert_eq!(target.namespace.as_deref(), Some("test"));
                    assert_eq!(container, name);
                }
                _ => panic!("container shell must start exec, not navigate to pod logs"),
            }
        }
        let menu = shell_menu(&pod, None).unwrap();
        assert!(menu.action.is_none());
        assert_eq!(menu.children.len(), 2);
    }

    #[test]
    fn stopped_container_shell_is_visible_and_disabled() {
        let mut pod = two_container_pod();
        pod.fields["containers"][1]["state"] = "Completed".into();
        let menu = shell_menu(&pod, Some("sidecar")).unwrap();
        assert_eq!(menu.label, "Open shell");
        assert!(!menu.enabled);
        assert!(shell_menu(&pod, Some("missing")).is_none());
    }
}
