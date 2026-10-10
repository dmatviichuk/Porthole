use super::{App, style::*};
use crate::resources::ResourceType;
use eframe::egui::*;
#[derive(Default)]
pub(super) struct State {
    pub token: u64,
    pub loaded: Option<String>,
    pub draft: String,
    pub error: Option<String>,
    pub saving: bool,
    pub find: bool,
    pub query: String,
    pub replacement: String,
    pub match_case: bool,
    pub regexp: bool,
    pub whole_word: bool,
    pub current: usize,
    pub editor: super::editor::Editor,
    pub find_fresh: bool,
    pub goto_line: Option<String>,
    pub goto_fresh: bool,
}
impl App {
    pub(super) fn reload_yaml(&mut self, r: ResourceType, namespace: Option<String>, name: String) {
        self.yaml = State::default();
        self.yaml.token = self.id();
        self.data
            .yaml(self.yaml.token, self.context.clone(), r, namespace, name);
    }
    pub(super) fn yaml_panel(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        resource: ResourceType,
        namespace: Option<String>,
        name: String,
    ) {
        let t = self.t;
        if self.yaml.token == 0 {
            self.reload_yaml(resource.clone(), namespace.clone(), name.clone());
        }
        let dirty = self.yaml.loaded.as_ref().is_some_and(|s| *s != self.yaml.draft);
        let mut save = false;
        let mut reload = false;
        let sr = rect(r.left() + 32.0, r.top() + 1.0, 107.0, 25.5);
        ui.add_enabled_ui(dirty && !self.yaml.saving, |ui| {
            if button_with_font(
                ui,
                sr,
                "save-yaml",
                if self.yaml.saving { "Saving…" } else { "Save changes" },
                12.5,
                "medium",
                Color32::WHITE,
                Some(t.accent),
                None,
                t,
            )
            .clicked()
            {
                save = true;
            }
        });
        ui.add_enabled_ui(dirty, |ui| {
            if button(
                ui,
                rect(sr.right() + 8.0, r.top(), 70.0, 27.5),
                "discard-yaml",
                "Discard",
                12.5,
                t.text,
                None,
                Some(t.strong),
                t,
            )
            .clicked()
                && let Some(s) = &self.yaml.loaded
            {
                self.yaml.draft = s.clone();
            }
        });
        let reload_rect = rect(
            sr.right() + 86.0,
            sr.top(),
            text_width(ui, "Reload", 12.5, "sans") + 32.0,
            25.5,
        );
        let response = button(ui, reload_rect, "reload-yaml", "", 12.5, t.muted, None, None, t);
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Reload"));
        if response.clicked() {
            reload = true;
        }
        self.icons.paint(
            ui,
            "rotate-cw",
            rect(reload_rect.left() + 8.0, reload_rect.center().y - 6.0, 12.0, 12.0),
            t.muted,
        );
        label(
            ui,
            rect(reload_rect.left() + 24.0, reload_rect.top(), 48.0, reload_rect.height()),
            "Reload",
            12.5,
            "sans",
            t.muted,
        );
        let hint = if dirty {
            "Unsaved changes. Saving replaces the live object and fails if it changed since you loaded it."
        } else {
            "Edit and save to update the live object."
        };
        let w = text_width(ui, hint, 12.0, "sans");
        label(
            ui,
            rect((r.right() - 32.0 - w).max(sr.right() + 164.0), r.top(), w, 25.5),
            hint,
            12.0,
            "sans",
            if dirty { t.progress } else { t.muted },
        );
        let mut y = r.top() + 36.0;
        if let Some(error) = self.yaml.error.clone() {
            y += self.banner(ui, rect(r.left(), y, r.width(), r.bottom() - y), &error, false);
        }
        let body = rect(r.left(), y, r.width(), r.bottom() - y);
        line(ui, body.left_top(), body.right_top(), t.line);
        if self.yaml.loaded.is_none() && self.yaml.error.is_none() {
            super::views::placeholder(ui, body, "Loading YAML…", t);
            return;
        }
        if self.focus_search {
            if let Some(selection) = self.yaml.editor.selections.last() {
                let range = selection.range();
                if !range.is_empty() && range.len() <= 100 {
                    self.yaml.query = self.yaml.draft[range].replace('\n', "\\n");
                }
            }
            self.yaml.find = true;
            self.yaml.find_fresh = true;
            self.focus_search = false;
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(Key::F)) {
            self.yaml.find = true;
            self.yaml.find_fresh = true;
            ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F));
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(Key::S)) {
            save = true;
            ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S));
        }
        if ui.input(|i| i.modifiers.command && i.modifiers.alt && i.key_pressed(Key::G)) {
            let head = self.yaml.editor.selections.last().map(|s| s.head).unwrap_or(0);
            self.yaml.goto_line =
                Some((self.yaml.draft[..head].bytes().filter(|b| *b == b'\n').count() + 1).to_string());
            self.yaml.goto_fresh = true;
            ui.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::ALT, Key::G));
        } else if ui.input(|i| (i.modifiers.command && i.key_pressed(Key::G)) || i.key_pressed(Key::F3)) {
            if find_pattern(&self.yaml).is_none() {
                self.yaml.find = true;
                self.yaml.find_fresh = true;
            }
            let backwards = ui.input(|i| i.modifiers.shift);
            jump(&mut self.yaml, !backwards);
            ui.input_mut(|i| {
                i.consume_key(
                    if backwards {
                        Modifiers::COMMAND | Modifiers::SHIFT
                    } else {
                        Modifiers::COMMAND
                    },
                    Key::G,
                );
            });
        }
        let find_h = if self.yaml.find { 49.0 } else { 0.0 };
        let goto_h = if self.yaml.goto_line.is_some() { 26.5 } else { 0.0 };
        let editor = rect(body.left(), body.top(), body.width(), body.height() - find_h - goto_h);
        let matches: Vec<_> = find_pattern(&self.yaml)
            .map(|re| find_matches(&self.yaml, &re))
            .unwrap_or_default();
        self.yaml.editor.show(ui, editor, &mut self.yaml.draft, &matches, t);
        if self.yaml.find {
            let panel = rect(body.left(), body.bottom() - find_h - goto_h, body.width(), find_h);
            ui.painter().rect_filled(panel, 0, t.raised);
            line(ui, panel.left_top(), panel.right_top(), Color32::from_gray(180));
            let x = panel.left() + 6.0;
            let y = panel.top() + 5.5;
            let find = find_input(ui, rect(x, y, 126.0, 17.5), &mut self.yaml.query, "Find", "yaml-find");
            if self.yaml.find_fresh {
                find.request_focus();
                let mut state = text_edit::TextEditState::default();
                state.cursor.set_char_range(Some(text::CCursorRange::two(
                    text::CCursor::new(0),
                    text::CCursor::new(self.yaml.query.chars().count()),
                )));
                state.store(ui.ctx(), Id::new("yaml-find"));
                self.yaml.find_fresh = false;
            }
            let enabled = !self.yaml.query.is_empty();
            if find_button(ui, rect(x + 132.0, y, 38.0, 17.5), "next", enabled, t) {
                jump(&mut self.yaml, true);
            }
            if find_button(ui, rect(x + 176.0, y, 56.0, 17.5), "previous", enabled, t) {
                jump(&mut self.yaml, false);
            }
            if find_button(ui, rect(x + 238.0, y, 28.0, 17.5), "all", enabled, t) {
                self.yaml.editor.select_all_matches(&matches);
            }
            find_checkbox(
                ui,
                rect(x + 272.0, y, 72.0, 17.5),
                &mut self.yaml.match_case,
                "match case",
                t,
            );
            find_checkbox(ui, rect(x + 348.0, y, 50.0, 17.5), &mut self.yaml.regexp, "regexp", t);
            find_checkbox(
                ui,
                rect(x + 402.0, y, 56.0, 17.5),
                &mut self.yaml.whole_word,
                "by word",
                t,
            );
            let replace = find_input(
                ui,
                rect(x, y + 20.0, 126.0, 17.5),
                &mut self.yaml.replacement,
                "Replace",
                "yaml-replace",
            );
            if find_button(ui, rect(x + 132.0, y + 20.0, 52.0, 17.5), "replace", enabled, t)
                || (replace.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
            {
                let range = self.yaml.editor.selections.last().map(|s| s.range()).unwrap_or(0..0);
                if let Some(re) = find_pattern(&self.yaml)
                    && find_matches(&self.yaml, &re).contains(&range)
                {
                    let value = replacement_for(&self.yaml, &re, range);
                    self.yaml.editor.replace(&mut self.yaml.draft, &value);
                }
                jump(&mut self.yaml, true);
            }
            if find_button(ui, rect(x + 190.0, y + 20.0, 66.0, 17.5), "replace all", enabled, t)
                && let Some(re) = find_pattern(&self.yaml)
            {
                let mut value = self.yaml.draft.clone();
                for range in find_matches(&self.yaml, &re).into_iter().rev() {
                    value.replace_range(range.clone(), &replacement_for(&self.yaml, &re, range));
                }
                self.yaml.editor.select(0..self.yaml.draft.len());
                self.yaml.editor.replace(&mut self.yaml.draft, &value);
            }
            let close = rect(panel.right() - 17.0, panel.top() + 4.0, 12.0, 17.0);
            label(ui, close, "×", 12.5, "sans", t.text);
            if ui.interact(close, Id::new("yaml-find-close"), Sense::click()).clicked()
                || ui.input(|i| i.key_pressed(Key::Escape))
            {
                self.yaml.find = false;
                ui.memory_mut(|m| m.request_focus(Id::new("yaml-editor")));
            }
            if find.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                jump(&mut self.yaml, !ui.input(|i| i.modifiers.shift));
            }
        }
        if let Some(mut value) = self.yaml.goto_line.take() {
            let panel = rect(body.left(), body.bottom() - goto_h, body.width(), goto_h);
            ui.painter().rect_filled(panel, 0, t.raised);
            line(ui, panel.left_top(), panel.right_top(), Color32::from_gray(180));
            label(
                ui,
                rect(panel.left() + 6.0, panel.top(), 51.0, goto_h),
                "Go to line:",
                10.4,
                "sans",
                t.text,
            );
            let input = find_input(
                ui,
                rect(panel.left() + 59.0, panel.top() + 7.0, 104.0, 14.5),
                &mut value,
                "",
                "yaml-goto",
            );
            if self.yaml.goto_fresh {
                input.request_focus();
                let count = value.chars().count();
                let mut state = text_edit::TextEditState::default();
                state.cursor.set_char_range(Some(text::CCursorRange::two(
                    text::CCursor::new(0),
                    text::CCursor::new(count),
                )));
                state.store(ui.ctx(), Id::new("yaml-goto"));
                self.yaml.goto_fresh = false;
            }
            let submit = find_button(
                ui,
                rect(panel.left() + 167.0, panel.top() + 5.0, 30.0, 17.5),
                "go",
                true,
                t,
            ) || (input.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
            let close = rect(panel.right() - 12.0, panel.top() + 5.0, 12.0, 17.0);
            label(ui, close, "×", 12.5, "sans", t.text);
            let cancel = ui.interact(close, Id::new("yaml-goto-close"), Sense::click()).clicked()
                || ui.input(|i| i.key_pressed(Key::Escape));
            if submit
                && let Some(at) = goto_position(
                    &self.yaml.draft,
                    self.yaml.editor.selections.last().map(|s| s.head).unwrap_or(0),
                    &value,
                )
            {
                self.yaml.editor.select(at..at);
            }
            if submit || cancel {
                ui.memory_mut(|m| m.request_focus(Id::new("yaml-editor")));
            } else {
                self.yaml.goto_line = Some(value);
            }
        }
        if reload {
            self.reload_yaml(resource.clone(), namespace.clone(), name.clone());
        }
        if save && dirty && !self.yaml.saving {
            self.yaml.saving = true;
            self.yaml.error = None;
            self.data.save_yaml(
                self.yaml.token,
                self.context.clone(),
                resource,
                namespace,
                name,
                self.yaml.draft.clone(),
            );
        }
    }
}
fn goto_position(text: &str, head: usize, value: &str) -> Option<usize> {
    let re = regex::Regex::new(r"^([+-])?(\d+)?(:\d+)?(%)?$").unwrap();
    let captures = re.captures(value)?;
    let starts: Vec<_> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let current = starts.partition_point(|&at| at <= head) as f64;
    let number = captures.get(2).map(|m| m.as_str().parse::<f64>()).transpose().ok()?;
    let sign = captures.get(1).map(|m| if m.as_str() == "-" { -1.0 } else { 1.0 });
    let line = number
        .map(|n| {
            let n = if captures.get(4).is_some() {
                (starts.len() as f64 * n / 100.0).round()
            } else {
                n
            };
            sign.map(|sign| current + sign * n).unwrap_or(n)
        })
        .unwrap_or(current)
        .clamp(1.0, starts.len() as f64) as usize;
    let start = starts[line - 1];
    let end = starts.get(line).map(|at| at - 1).unwrap_or(text.len());
    let column = captures
        .get(3)
        .and_then(|m| m.as_str()[1..].parse::<usize>().ok())
        .unwrap_or(0);
    Some(
        start
            + text[start..end]
                .char_indices()
                .nth(column)
                .map(|(at, _)| at)
                .unwrap_or(end - start),
    )
}
fn jump(state: &mut State, forward: bool) {
    let Some(re) = find_pattern(state) else {
        return;
    };
    let matches = find_matches(state, &re);
    if matches.is_empty() {
        return;
    }
    let selected = state.editor.selections.last().map(|s| s.range()).unwrap_or(0..0);
    let index = if forward {
        matches
            .iter()
            .position(|m| m.start >= selected.end && *m != selected)
            .unwrap_or(0)
    } else {
        matches
            .iter()
            .rposition(|m| m.end <= selected.start && *m != selected)
            .unwrap_or(matches.len() - 1)
    };
    state.current = index;
    state.editor.select(matches[index].clone());
}
fn find_input(ui: &mut Ui, r: Rect, value: &mut String, hint: &str, id: &str) -> Response {
    ui.painter().rect_filled(r, 0, Color32::WHITE);
    let mut input = ui.new_child(UiBuilder::new().max_rect(r.shrink2(vec2(3.0, 1.0))));
    input.visuals_mut().weak_text_color = Some(Color32::from_gray(100));
    let response = input.put(
        r.shrink2(vec2(3.0, 1.0)),
        TextEdit::singleline(value)
            .id(Id::new(id))
            .font(font(11.0, "sans"))
            .text_color(Color32::BLACK)
            .hint_text(RichText::new(hint).color(Color32::from_gray(100)))
            .frame(Frame::NONE)
            .margin(0),
    );
    ui.painter().rect_stroke(
        r,
        0,
        Stroke::new(
            if response.has_focus() { 2.0 } else { 1.0 },
            if response.has_focus() {
                hex("#3d8bff")
            } else {
                Color32::from_gray(120)
            },
        ),
        StrokeKind::Inside,
    );
    response
}
fn find_button(ui: &mut Ui, r: Rect, text: &str, enabled: bool, t: Tokens) -> bool {
    let response = ui.interact(r, Id::new(("yaml-find-button", text)), Sense::click());
    ui.painter().rect_filled(r, 1, Color32::from_gray(220));
    ui.painter()
        .rect_stroke(r, 1, Stroke::new(1.0, Color32::from_gray(140)), StrokeKind::Inside);
    let width = text_width(ui, text, 11.0, "sans");
    label(
        ui,
        rect(r.center().x - width / 2.0, r.top(), width, r.height()),
        text,
        11.0,
        "sans",
        if enabled {
            Color32::BLACK
        } else {
            t.text.gamma_multiply(0.4)
        },
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, text));
    enabled && response.clicked()
}
fn find_checkbox(ui: &mut Ui, r: Rect, value: &mut bool, text: &str, t: Tokens) {
    let response = ui.interact(r, Id::new(("yaml-find-checkbox", text)), Sense::click());
    if response.clicked() {
        *value = !*value;
    }
    let box_rect = rect(r.left(), r.center().y - 5.5, 11.0, 11.0);
    ui.painter()
        .rect_stroke(box_rect, 3, Stroke::new(1.5, t.muted), StrokeKind::Inside);
    if *value {
        ui.painter().rect_filled(box_rect, 3, t.accent);
        ui.painter().add(Shape::line(
            vec![
                box_rect.left_top() + vec2(2.0, 5.5),
                box_rect.left_top() + vec2(4.5, 8.0),
                box_rect.left_top() + vec2(9.0, 3.0),
            ],
            Stroke::new(1.5, Color32::WHITE),
        ));
    }
    label(
        ui,
        rect(r.left() + 13.0, r.top(), r.width() - 13.0, r.height()),
        text,
        10.0,
        "sans",
        t.text,
    );
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, *value, text));
}
fn unquote(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next) = chars.peek()
        {
            let escaped = match next {
                'n' => Some('\n'),
                'r' => Some('\r'),
                't' => Some('\t'),
                '\\' => Some('\\'),
                _ => None,
            };
            if let Some(value) = escaped {
                out.push(value);
                chars.next();
                continue;
            }
        }
        out.push(c);
    }
    out
}
fn replacement_for(s: &State, re: &fancy_regex::Regex, range: std::ops::Range<usize>) -> String {
    let value = unquote(&s.replacement);
    if !s.regexp {
        return value;
    }
    let Ok(Some(captures)) = re.captures_from_pos(&s.draft, range.start) else {
        return value;
    };
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                Some('$') => {
                    chars.next();
                    out.push('$');
                    continue;
                }
                Some('&') => {
                    chars.next();
                    out.push_str(captures.get(0).map(|m| m.as_str()).unwrap_or(""));
                    continue;
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut index = String::new();
                    while chars.peek().is_some_and(char::is_ascii_digit) {
                        index.push(chars.next().unwrap());
                    }
                    if let Some(value) = index.parse::<usize>().ok().and_then(|i| captures.get(i)) {
                        out.push_str(value.as_str());
                    }
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}
fn find_matches(s: &State, re: &fancy_regex::Regex) -> Vec<std::ops::Range<usize>> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    re.find_iter(&s.draft)
        .flatten()
        .map(|m| m.range())
        .filter(|range| {
            if !s.whole_word || range.is_empty() {
                return true;
            }
            let value = &s.draft[range.clone()];
            let starts_inside_word =
                s.draft[..range.start].chars().next_back().is_some_and(word) && value.chars().next().is_some_and(word);
            let ends_inside_word =
                value.chars().next_back().is_some_and(word) && s.draft[range.end..].chars().next().is_some_and(word);
            !(starts_inside_word || ends_inside_word)
        })
        .collect()
}
fn find_pattern(s: &State) -> Option<fancy_regex::Regex> {
    if s.query.is_empty() {
        return None;
    }
    let pattern = if s.regexp {
        s.query.clone()
    } else {
        regex::escape(&unquote(&s.query))
    };
    fancy_regex::RegexBuilder::new(&pattern)
        .case_insensitive(!s.match_case)
        .backtrack_limit(100_000)
        .build()
        .ok()
}
pub(super) fn yaml_job(text: &str, t: Tokens) -> eframe::egui::text::LayoutJob {
    let mut job = eframe::egui::text::LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let mut at = 0;
    while at < text.len() {
        let c = text[at..].chars().next().unwrap();
        let start = at;
        let color;
        if c == '#' && (at == 0 || text[..at].chars().next_back().is_some_and(char::is_whitespace)) {
            at = text[at..].find('\n').map(|i| at + i).unwrap_or(text.len());
            color = t.meta;
        } else if c == '\'' || c == '"' {
            at += c.len_utf8();
            let quote = c;
            let mut escaped = false;
            while at < text.len() {
                let c = text[at..].chars().next().unwrap();
                at += c.len_utf8();
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' && quote == '"' {
                    escaped = true;
                } else if c == quote {
                    break;
                }
            }
            color = if text[at..].starts_with(':') { t.key } else { t.string };
        } else if c.is_whitespace() {
            at += c.len_utf8();
            color = t.text;
        } else if "{}[],".contains(c)
            || (c == ':' && text[at + 1..].chars().next().is_none_or(|c| c.is_whitespace()))
            || (c == '-' && text[at + 1..].starts_with(' '))
        {
            at += c.len_utf8();
            color = t.meta;
        } else {
            while at < text.len() {
                let c = text[at..].chars().next().unwrap();
                if c.is_whitespace()
                    || "{}[],".contains(c)
                    || (c == ':'
                        && text[at + 1..]
                            .chars()
                            .next()
                            .is_none_or(|c| c.is_whitespace() || "{[".contains(c)))
                {
                    break;
                }
                at += c.len_utf8();
            }
            if at == start {
                at += c.len_utf8();
            }
            let token = &text[start..at];
            color = if text[at..].starts_with(':') {
                t.key
            } else if token.starts_with(['&', '*', '!']) {
                t.number
            } else if matches!(token, "---" | "...") {
                t.meta
            } else {
                t.string
            };
        }
        let mut format = text_format(12.5, "mono", color);
        format.line_height = Some(19.375);
        job.append(&text[start..at], 0.0, format);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_keeps_original_escape_case_and_word_rules() {
        let mut state = State {
            draft: "Name: café\nname: cafe\nrename: café\n-".into(),
            query: "name".into(),
            whole_word: true,
            ..Default::default()
        };
        assert_eq!(find_matches(&state, &find_pattern(&state).unwrap()), vec![0..4, 12..16]);
        state.match_case = true;
        assert_eq!(find_matches(&state, &find_pattern(&state).unwrap()), vec![12..16]);
        state.query = "\\n-".into();
        assert_eq!(find_matches(&state, &find_pattern(&state).unwrap()).len(), 1);
        state.replacement = "$1\\n\\t\\\\".into();
        assert_eq!(
            replacement_for(&state, &find_pattern(&state).unwrap(), 0..4),
            "$1\n\t\\"
        );
    }
    #[test]
    fn regex_supports_lookaround_backreferences_and_original_replacements() {
        let state = State {
            draft: "name: café café\nname: tea".into(),
            query: r"(?<=name: )(\w+) \1".into(),
            replacement: "$1-$&-$$-$99".into(),
            regexp: true,
            ..Default::default()
        };
        let re = find_pattern(&state).unwrap();
        let matches = find_matches(&state, &re);
        assert_eq!(matches, vec![6..17]);
        assert_eq!(replacement_for(&state, &re, matches[0].clone()), "café-café café-$-");
    }
    #[test]
    fn find_wraps_in_both_directions() {
        let mut state = State {
            draft: "one two one".into(),
            query: "one".into(),
            ..Default::default()
        };
        jump(&mut state, true);
        assert_eq!(state.editor.selections[0].range(), 0..3);
        jump(&mut state, true);
        assert_eq!(state.editor.selections[0].range(), 8..11);
        jump(&mut state, true);
        assert_eq!(state.editor.selections[0].range(), 0..3);
        jump(&mut state, false);
        assert_eq!(state.editor.selections[0].range(), 8..11);
    }
    #[test]
    fn go_to_line_supports_offsets_columns_percentages_and_bounds() {
        let text = "first\ncafé\nthird\nlast";
        assert_eq!(goto_position(text, 0, "2:3"), Some(9));
        assert_eq!(goto_position(text, 6, "+1"), Some(12));
        assert_eq!(goto_position(text, 6, "-1"), Some(0));
        assert_eq!(goto_position(text, 0, "50%"), Some(6));
        assert_eq!(goto_position(text, 0, "1000:100"), Some(text.len()));
        assert_eq!(goto_position(text, 0, "invalid"), None);
    }
}
