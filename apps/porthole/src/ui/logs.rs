use super::{App, style::*};
use crate::{
    logs::{LogEvent, LogTarget},
    model,
    resources::ResourceType,
    summary::ResourceRow,
};
use eframe::egui::*;
use std::collections::{HashMap, HashSet};
#[derive(Default)]
pub(super) struct State {
    pub token: u64,
    pub sessions: HashMap<String, u64>,
    pub sources: Vec<String>,
    pub lines: Vec<Line>,
    filter: String,
    follow: bool,
    initialized: bool,
    cache: HashMap<String, Vec<Part>>,
}
pub(super) struct Line {
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
impl State {
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
                    self.lines
                        .push(Line::new(source, l.ts, re.replace_all(&l.text, "").into_owned(), false));
                }
            }
            LogEvent::Ended { .. } => self.lines.push(Line::new(
                source,
                None,
                format!(
                    "{}: log stream ended",
                    key.split('/').skip(1).collect::<Vec<_>>().join("/")
                ),
                true,
            )),
            LogEvent::Error { message, .. } => self.lines.push(Line::new(
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
        let multiple = targets.len() > 1;
        let char_w = text_width(ui, "0000000000", 12.0, "mono") / 10.0;
        let prefix =
            24.0 + if self.prefs.timestamps {
                12.0 * char_w + 12.0
            } else {
                0.0
            } + if multiple { 162.0 } else { 0.0 };
        let text_width = (body.width() - prefix - 24.0).max(120.0);
        let labels = source_labels(&targets);
        let formatted = self.prefs.format_logs;
        let wrap = self.prefs.wrap_logs;
        let layout_key = (if wrap { text_width.to_bits() } else { 0 }, formatted);
        let mut heights = Vec::with_capacity(visible.len() + 1);
        heights.push(0.0);
        for &i in &visible {
            let line = &self.logs.lines[i];
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
        if self.logs.cache.len() > 5000 {
            self.logs.cache.clear();
        }
        let total = *heights.last().unwrap_or(&0.0);
        let mut at_bottom = self.logs.follow;
        let mut measured_heights = Vec::new();
        ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
            ui.set_clip_rect(body);
            let scroll =
                ScrollArea::new([!wrap, true])
                    .id_salt("logs-scroll")
                    .auto_shrink([false, false])
                    .stick_to_bottom(self.logs.follow)
                    .show_viewport(ui, |ui, viewport| {
                        ui.set_min_size(vec2(
                            if wrap {
                                body.width()
                            } else {
                                body.width().max(
                                    prefix
                                        + visible
                                            .iter()
                                            .map(|&i| self.logs.lines[i].raw_length as f32 * char_w)
                                            .fold(0.0, f32::max)
                                        + 24.0,
                                )
                            },
                            total,
                        ));
                        let origin = ui.min_rect().min;
                        let first = heights.partition_point(|h| *h < viewport.top()).saturating_sub(2);
                        let last = heights
                            .partition_point(|h| *h < viewport.bottom() + 40.0)
                            .min(visible.len());
                        for shown in first..last {
                            let index = visible[shown];
                            let line = &self.logs.lines[index];
                            let y = origin.y + heights[shown];
                            let mut x = origin.x + 24.0;
                            if self.prefs.timestamps {
                                let timestamp = line.ts.as_deref().map(log_time).unwrap_or_default();
                                label(ui, rect(x, y, 12.0 * char_w, 19.0), &timestamp, 12.0, "mono", t.faint);
                                x += 12.0 * char_w + 12.0;
                            }
                            if multiple {
                                let source = self
                                    .logs
                                    .sources
                                    .get(line.source)
                                    .map(String::as_str)
                                    .unwrap_or_default();
                                let text = labels
                                    .get(source)
                                    .map(String::as_str)
                                    .unwrap_or_else(|| source.split('/').nth(1).unwrap_or_default());
                                label(
                                    ui,
                                    rect(x, y, 150.0, 19.0),
                                    text,
                                    12.0,
                                    "mono",
                                    hex(["#4f9cf9", "#e5a03c", "#3cc97c", "#c47cf0", "#f06f8a", "#3cc3c9"]
                                        [line.source % 6]),
                                );
                                x += 162.0;
                            }
                            let parts = if formatted && !line.system {
                                self.logs
                                    .cache
                                    .entry(line.text.clone())
                                    .or_insert_with(|| parse(&line.text))
                                    .clone()
                            } else {
                                vec![Part {
                                    text: line.text.clone(),
                                    tone: if line.system { Tone::Muted } else { Tone::Text },
                                }]
                            };
                            let job = log_job(
                                &parts,
                                &self.logs.filter,
                                t,
                                if wrap { text_width } else { f32::INFINITY },
                            );
                            let galley = ui.painter().layout_job(job);
                            let h = galley.size().y;
                            if line.layout.is_none_or(|(key, height)| key != layout_key || height != h) {
                                measured_heights.push((index, h));
                            }
                            let leading = galley
                                .rows
                                .first()
                                .and_then(|r| r.glyphs.first())
                                .map_or(0.0, |glyph| (glyph.line_height - glyph.font_height).max(0.0) / 2.0);
                            // A pre-laid-out galley keeps every message left-aligned. `put`
                            // centers a LayoutJob in its cell, including each wrapped row.
                            ui.place(
                                Rect::from_min_size(pos2(x, y + leading), galley.size()),
                                Label::new(galley).selectable(true).show_tooltip_when_elided(false),
                            );
                        }
                    });
            at_bottom = scroll.state.offset.y + body.height() >= total - 24.0;
        });
        if !measured_heights.is_empty() {
            for (index, height) in measured_heights {
                self.logs.lines[index].layout = Some((layout_key, height));
            }
            ui.ctx().request_repaint();
        }
        self.logs.follow = at_bottom;
        if !self.logs.follow && !visible.is_empty() {
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
}
