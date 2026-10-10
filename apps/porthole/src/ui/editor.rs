//! Native YAML editing, including the selection and folding behavior of the original editor.
use super::style::*;
use eframe::egui::*;
use std::{collections::BTreeSet, ops::Range};

const LINE_HEIGHT: f32 = 19.375;
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Selection {
    pub anchor: usize,
    pub head: usize,
}
impl Selection {
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
    fn caret(at: usize) -> Self {
        Self { anchor: at, head: at }
    }
}
#[derive(Clone)]
struct Record {
    text: Option<String>,
    selections: Vec<Selection>,
    primary: Option<usize>,
    folds: BTreeSet<usize>,
    selections_after: Vec<(Vec<Selection>, Option<usize>)>,
}
#[derive(Default)]
pub(super) struct Editor {
    pub selections: Vec<Selection>,
    primary: Option<usize>,
    folds: BTreeSet<usize>,
    undo: Vec<Record>,
    redo: Vec<Record>,
    reveal: bool,
    column: Option<usize>,
    rectangle: Option<(usize, usize)>,
    drag_anchor: usize,
    preedit: String,
    last_input: f64,
    tab_focus: bool,
    typing_cursors: Vec<Selection>,
    typing_time: f64,
    edit_generation: u64,
    selection_time: f64,
    selection_origin: &'static str,
    drag_source: Option<Range<usize>>,
    linewise_copy: Option<String>,
}
fn lines(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}
fn line_at(starts: &[usize], at: usize) -> usize {
    starts.partition_point(|i| *i <= at).saturating_sub(1)
}
fn line_end(text: &str, starts: &[usize], row: usize) -> usize {
    starts.get(row + 1).map(|i| i - 1).unwrap_or(text.len())
}
fn position(text: &str, starts: &[usize], row: usize, column: usize) -> usize {
    let row = row.min(starts.len() - 1);
    let end = line_end(text, starts, row);
    let mut visual = 0;
    for (index, c) in text[starts[row]..end].char_indices() {
        let width = if c == '\t' { 2 - visual % 2 } else { 1 };
        if column < visual + width {
            return starts[row] + index;
        }
        visual += width;
    }
    end
}
fn visual_column(value: &str) -> usize {
    value
        .chars()
        .fold(0, |column, c| column + if c == '\t' { 2 - column % 2 } else { 1 })
}
fn special_placeholder(c: char) -> Option<char> {
    match c {
        '\n' => Some('\u{2424}'),
        '\u{0000}'..='\u{0008}' | '\u{000a}'..='\u{001f}' => char::from_u32(0x2400 + u32::from(c)),
        '\u{007f}'..='\u{009f}'
        | '\u{00ad}'
        | '\u{061c}'
        | '\u{200b}'
        | '\u{200e}'
        | '\u{200f}'
        | '\u{2028}'
        | '\u{2029}'
        | '\u{202d}'
        | '\u{202e}'
        | '\u{2066}'
        | '\u{2067}'
        | '\u{2069}'
        | '\u{feff}'
        | '\u{fff9}'..='\u{fffc}' => Some('•'),
        _ => None,
    }
}
fn special_description(c: char) -> String {
    let name = match c {
        '\0' => "null",
        '\u{7}' => "bell",
        '\u{8}' => "backspace",
        '\n' => "newline",
        '\u{b}' => "vertical tab",
        '\r' => "carriage return",
        '\u{1b}' => "escape",
        '\u{200b}' => "zero width space",
        '\u{200e}' => "left-to-right mark",
        '\u{200f}' => "right-to-left mark",
        '\u{2028}' => "line separator",
        '\u{2029}' => "paragraph separator",
        '\u{202d}' => "left-to-right override",
        '\u{202e}' => "right-to-left override",
        '\u{2066}' => "left-to-right isolate",
        '\u{2067}' => "right-to-left isolate",
        '\u{2069}' => "pop directional isolate",
        '\u{feff}' => "zero width no-break space",
        '\u{fffc}' => "object replacement",
        _ => return format!("Control character 0x{:x}", u32::from(c)),
    };
    format!("Control character {name}")
}
fn display_line(line: &str) -> (String, Vec<(Range<usize>, char)>) {
    let mut shown = String::new();
    let mut special = Vec::new();
    let mut column = 0;
    for c in line.chars() {
        if c == '\t' {
            let width = 2 - column % 2;
            shown.push_str(&" ".repeat(width));
            column += width;
        } else {
            let start = shown.len();
            shown.push(special_placeholder(c).unwrap_or(c));
            if special_placeholder(c).is_some() {
                special.push((start..shown.len(), c));
            }
            column += 1;
        }
    }
    (shown, special)
}
fn previous(text: &str, at: usize) -> usize {
    text[..at].char_indices().next_back().map(|(i, _)| i).unwrap_or(0)
}
fn next(text: &str, at: usize) -> usize {
    text[at..].chars().next().map(|c| at + c.len_utf8()).unwrap_or(at)
}
fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
fn word_edge(text: &str, at: usize, forward: bool) -> usize {
    let mut at = at;
    if forward {
        while at < text.len() && !word(text[at..].chars().next().unwrap()) {
            at = next(text, at);
        }
        while at < text.len() && word(text[at..].chars().next().unwrap()) {
            at = next(text, at);
        }
    } else {
        while at > 0 && !word(text[previous(text, at)..].chars().next().unwrap()) {
            at = previous(text, at);
        }
        while at > 0 && word(text[previous(text, at)..].chars().next().unwrap()) {
            at = previous(text, at);
        }
    }
    at
}
fn fold_end(text: &str, starts: &[usize], row: usize) -> Option<usize> {
    let value = &text[starts[row]..line_end(text, starts, row)];
    if value.trim().is_empty() {
        return None;
    }
    let indent = value.len() - value.trim_start().len();
    let indentless_sequence = value.trim_end().ends_with(':');
    let mut end = row;
    for i in row + 1..starts.len() {
        let value = &text[starts[i]..line_end(text, starts, i)];
        if value.trim().is_empty() {
            continue;
        }
        let nested = value.len() - value.trim_start().len();
        if nested < indent || (nested == indent && (!indentless_sequence || !value.trim_start().starts_with("- "))) {
            break;
        }
        end = i;
    }
    (end > row).then_some(end)
}
fn scalar_rows(text: &str) -> BTreeSet<usize> {
    let mut block_indent = None;
    let mut rows = BTreeSet::new();
    for (row, line) in text.split('\n').enumerate() {
        let indent = line.len() - line.trim_start().len();
        if let Some(block) = block_indent {
            if line.trim().is_empty() || indent > block {
                rows.insert(row);
                continue;
            }
            block_indent = None;
        }
        if line
            .split_once(": ")
            .is_some_and(|(_, value)| matches!(value.trim(), "|" | "|-" | "|+" | ">" | ">-" | ">+"))
        {
            block_indent = Some(indent);
        }
    }
    rows
}
fn bracket_pair(text: &str, cursor: usize, scalar_rows: &BTreeSet<usize>) -> Option<(usize, usize)> {
    let candidate = if text[cursor..].starts_with(['[', ']', '{', '}', '(', ')']) {
        cursor
    } else if cursor > 0 && text[..cursor].ends_with(['[', ']', '{', '}', '(', ')']) {
        cursor - 1
    } else {
        return None;
    };
    let mut stack = vec![];
    let mut row = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    for (index, c) in text.char_indices() {
        if c == '\n' {
            row += 1;
            comment = false;
            continue;
        }
        if comment || scalar_rows.contains(&row) {
            continue;
        }
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' && q == '"' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
        } else if c == '#' {
            comment = true;
        } else if "[{(".contains(c) {
            stack.push((index, c));
        } else if "]})".contains(c)
            && let Some((open, bracket)) = stack.pop()
            && matches!((bracket, c), ('[', ']') | ('{', '}') | ('(', ')'))
            && (open == candidate || index == candidate)
        {
            return Some((open, index));
        }
    }
    None
}
impl Editor {
    pub fn select(&mut self, range: Range<usize>) {
        let before = self.selections.clone();
        let primary = self.primary;
        self.selections = vec![Selection {
            anchor: range.start,
            head: range.end,
        }];
        self.primary = None;
        self.remember_selection(before, primary, "select");
        self.reveal = true;
    }
    pub fn select_all_matches(&mut self, matches: &[Range<usize>]) {
        let before = self.selections.clone();
        let primary = self.primary;
        self.selections = matches
            .iter()
            .map(|r| Selection {
                anchor: r.start,
                head: r.end,
            })
            .collect();
        self.primary = None;
        self.remember_selection(before, primary, "select");
        self.reveal = true;
    }
    fn normalize(&mut self, text: &str) {
        if self.selections.is_empty() {
            self.selections.push(Selection::default());
        }
        for s in &mut self.selections {
            for p in [&mut s.anchor, &mut s.head] {
                *p = (*p).min(text.len());
                while !text.is_char_boundary(*p) {
                    *p -= 1;
                }
            }
        }
        let primary = self.main();
        self.selections.sort_by_key(|s| s.range().start);
        let mut out: Vec<Selection> = vec![];
        for s in &self.selections {
            if let Some(last) = out.last_mut()
                && s.range().start <= last.range().end
            {
                *last = Selection {
                    anchor: last.range().start,
                    head: last.range().end.max(s.range().end),
                };
            } else {
                out.push(*s);
            }
        }
        self.selections = out;
        self.primary = self
            .selections
            .iter()
            .position(|s| s.range().start <= primary.range().start && s.range().end >= primary.range().end);
    }
    fn main_index(&self) -> usize {
        self.primary
            .unwrap_or(usize::MAX)
            .min(self.selections.len().saturating_sub(1))
    }
    fn main(&self) -> Selection {
        self.selections.get(self.main_index()).copied().unwrap_or_default()
    }
    fn remember(&mut self, text: &str) {
        self.edit_generation += 1;
        self.drag_source = None;
        self.undo.push(Record {
            text: Some(text.into()),
            selections: self.selections.clone(),
            primary: self.primary,
            folds: self.folds.clone(),
            selections_after: Vec::new(),
        });
        let mut size = self
            .undo
            .iter()
            .filter_map(|r| r.text.as_ref())
            .map(String::len)
            .sum::<usize>();
        while self.undo.len() > 100 || (size > 10_000_000 && self.undo.len() > 1) {
            size -= self.undo.remove(0).text.map_or(0, |text| text.len());
        }
        self.redo.clear();
    }
    fn remember_selection(&mut self, before: Vec<Selection>, primary: Option<usize>, origin: &'static str) {
        let before_primary = primary.unwrap_or(usize::MAX).min(before.len().saturating_sub(1));
        if (before == self.selections && before_primary == self.main_index()) || before.is_empty() {
            return;
        }
        let record = self.undo.last_mut();
        if let Some(record) = record {
            let same_shape = record.selections_after.last().is_some_and(|(last, _)| {
                last.len() == before.len()
                    && last
                        .iter()
                        .zip(&before)
                        .all(|(a, b)| a.range().is_empty() == b.range().is_empty())
            });
            if same_shape && self.selection_origin == origin && self.last_input - self.selection_time < 0.5 {
                return;
            }
            if record.selections_after.last() != Some(&(before.clone(), primary)) {
                record.selections_after.push((before, primary));
                if record.selections_after.len() > 200 {
                    record.selections_after.remove(0);
                }
            }
        } else {
            self.undo.push(Record {
                text: None,
                selections: before.clone(),
                primary,
                folds: self.folds.clone(),
                selections_after: vec![(before, primary)],
            });
        }
        self.selection_time = self.last_input;
        self.selection_origin = origin;
    }
    pub fn replace(&mut self, text: &mut String, insert: &str) {
        self.replace_each(text, &vec![insert.to_owned(); self.selections.len().max(1)]);
    }
    fn replace_each(&mut self, text: &mut String, inserts: &[String]) {
        self.normalize(text);
        self.remember(text);
        let mut shift = 0_isize;
        let mut selections = vec![];
        let ranges: Vec<_> = self.selections.iter().map(|s| s.range()).collect();
        for (range, insert) in ranges.into_iter().zip(inserts.iter().cycle()) {
            let start = range.start.checked_add_signed(shift).unwrap();
            let end = range.end.checked_add_signed(shift).unwrap();
            self.map_folds(text, start..end, insert);
            text.replace_range(start..end, insert);
            shift += insert.len() as isize - range.len() as isize;
            selections.push(Selection::caret(start + insert.len()));
        }
        self.selections = selections;
        self.column = None;
        self.reveal = true;
    }
    fn clipboard(&mut self, text: &mut String, cut: bool) -> String {
        self.normalize(text);
        let linewise = self.selections.iter().all(|s| s.range().is_empty());
        let starts = lines(text);
        let ranges = if linewise {
            self.selected_lines(text)
                .into_iter()
                .map(|r| r.start..line_end(text, &starts, line_at(&starts, r.start)))
                .collect::<Vec<_>>()
        } else {
            self.selections
                .iter()
                .filter(|s| !s.range().is_empty())
                .map(|s| s.range())
                .collect()
        };
        let copied = ranges.iter().map(|r| &text[r.clone()]).collect::<Vec<_>>().join("\n");
        self.linewise_copy = linewise.then(|| copied.clone());
        if cut {
            let before = self.selections.clone();
            let primary = self.primary;
            if linewise {
                self.selections = self
                    .selected_lines(text)
                    .into_iter()
                    .map(|r| Selection {
                        anchor: r.start,
                        head: r.end,
                    })
                    .collect();
                self.primary = None;
            } else {
                self.selections.retain(|s| !s.range().is_empty());
                self.primary = None;
            }
            self.replace(text, "");
            self.undo.last_mut().unwrap().selections = before;
            self.undo.last_mut().unwrap().primary = primary;
        }
        copied
    }
    fn paste(&mut self, text: &mut String, value: &str) {
        self.normalize(text);
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        let values: Vec<_> = normalized.split('\n').map(str::to_owned).collect();
        let by_line = values.len() == self.selections.len();
        let linewise = self.linewise_copy.as_deref() == Some(normalized.as_str())
            && self.selections.iter().all(|s| s.range().is_empty());
        if linewise {
            let starts = lines(text);
            let before = self.selections.clone();
            let insertion_rows = before.iter().map(|s| line_at(&starts, s.head)).collect::<BTreeSet<_>>();
            self.remember(text);
            let mut shift = 0;
            let mut last_line = None;
            let mut next_line = 0;
            for (selection, cursor) in before.iter().zip(&mut self.selections) {
                let start = starts[line_at(&starts, selection.head)];
                if last_line == Some(start) {
                    *cursor = Selection::caret(selection.head + shift);
                    continue;
                }
                let insert = format!("{}\n", if by_line { &values[next_line] } else { &normalized });
                next_line += 1;
                let at = start + shift;
                text.insert_str(at, &insert);
                shift += insert.len();
                *cursor = Selection::caret(selection.head + shift);
                last_line = Some(start);
            }
            self.folds = self
                .folds
                .iter()
                .map(|&row| {
                    let inserted = insertion_rows.iter().filter(|&&at| at <= row).count();
                    row + inserted * if by_line { 1 } else { values.len() }
                })
                .collect();
            self.column = None;
            self.reveal = true;
        } else if by_line {
            self.replace_each(text, &values);
        } else {
            self.replace(text, &normalized);
        }
    }
    fn map_folds(&mut self, text: &str, range: Range<usize>, insert: &str) {
        let starts = lines(text);
        let removed = text[range.clone()].bytes().filter(|&b| b == b'\n').count();
        let added = insert.bytes().filter(|&b| b == b'\n').count();
        self.folds = self
            .folds
            .iter()
            .filter_map(|&row| {
                let start = *starts.get(row)?;
                if range.start <= start && start < range.end {
                    None
                } else if start >= range.end {
                    Some(row - removed + added)
                } else {
                    Some(row)
                }
            })
            .collect();
    }
    fn type_text(&mut self, text: &mut String, insert: &str, time: f64) {
        let group = time - self.typing_time < 0.5
            && self.selections == self.typing_cursors
            && self
                .undo
                .last()
                .is_some_and(|r| r.text.is_some() && r.selections_after.is_empty());
        self.replace(text, insert);
        if group {
            self.undo.pop();
        }
        self.typing_cursors = self.selections.clone();
        self.typing_time = time;
    }
    fn history(&mut self, text: &mut String, redo: bool) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(record) = from.pop_if(|r| r.text.is_some()) {
            self.edit_generation += 1;
            self.drag_source = None;
            let selection_after = record
                .selections_after
                .first()
                .cloned()
                .unwrap_or_else(|| (self.selections.clone(), self.primary));
            to.push(Record {
                text: Some(std::mem::replace(text, record.text.unwrap())),
                selections: selection_after.0,
                primary: selection_after.1,
                folds: std::mem::replace(&mut self.folds, record.folds),
                selections_after: Vec::new(),
            });
            self.selections = record.selections;
            self.primary = record.primary;
            self.reveal = true;
        }
    }
    fn selection_history(&mut self, text: &mut String, redo: bool) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(selection) = from.last_mut().and_then(|r| r.selections_after.pop()) {
            let before = (
                std::mem::replace(&mut self.selections, selection.0),
                std::mem::replace(&mut self.primary, selection.1),
            );
            if let Some(record) = to.last_mut() {
                record.selections_after.push(before);
            } else {
                to.push(Record {
                    text: None,
                    selections: before.0.clone(),
                    primary: before.1,
                    folds: self.folds.clone(),
                    selections_after: vec![before],
                });
            }
            self.reveal = true;
        } else {
            self.history(text, redo);
        }
    }
    fn drop_selection(&mut self, text: &mut String, source: Range<usize>, at: usize, copy: bool) {
        if !copy && source.start <= at && at <= source.end {
            return;
        }
        let value = text[source.clone()].to_owned();
        self.remember(text);
        let at = if copy {
            at
        } else {
            self.map_folds(text, source.clone(), "");
            text.replace_range(source.clone(), "");
            if at > source.end { at - source.len() } else { at }
        };
        self.map_folds(text, at..at, &value);
        text.insert_str(at, &value);
        self.selections = vec![Selection {
            anchor: at,
            head: at + value.len(),
        }];
        self.primary = None;
        self.typing_cursors.clear();
        self.reveal = true;
    }
    fn indent(&mut self, text: &mut String, outdent: bool) {
        self.normalize(text);
        let starts = lines(text);
        let mut rows = BTreeSet::new();
        for selection in &self.selections {
            let range = selection.range();
            let first = line_at(&starts, range.start);
            let last = line_at(
                &starts,
                if range.is_empty() {
                    range.end
                } else {
                    range.end.saturating_sub(1)
                },
            );
            rows.extend(first..=last);
        }
        self.remember(text);
        for row in rows.into_iter().rev() {
            let at = starts[row];
            let remove = if outdent {
                text[at..].chars().take(2).take_while(|c| *c == ' ').count()
            } else {
                0
            };
            let insert = if outdent { "" } else { "  " };
            text.replace_range(at..at + remove, insert);
            for selection in &mut self.selections {
                for p in [&mut selection.anchor, &mut selection.head] {
                    if *p >= at {
                        *p = p.saturating_sub(remove).max(at) + insert.len();
                    }
                }
            }
        }
        self.reveal = true;
    }
    fn selected_lines(&self, text: &str) -> Vec<Range<usize>> {
        let starts = lines(text);
        let mut rows = BTreeSet::new();
        for selection in &self.selections {
            let range = selection.range();
            let last = if range.is_empty() { range.end } else { range.end - 1 };
            rows.extend(line_at(&starts, range.start)..=line_at(&starts, last));
        }
        rows.into_iter()
            .map(|row| starts[row]..starts.get(row + 1).copied().unwrap_or(text.len()))
            .collect()
    }
    fn indent_selection(&mut self, text: &mut String) {
        let selected = self.selected_lines(text);
        let starts = lines(text);
        let blocks = scalar_rows(text);
        let mut parents: Vec<(usize, usize)> = vec![];
        let mut changes = vec![];
        for (row, &start) in starts.iter().enumerate() {
            let value = &text[start..line_end(text, &starts, row)];
            if value.trim().is_empty() || blocks.contains(&row) {
                continue;
            }
            let old = value.len() - value.trim_start().len();
            while parents.last().is_some_and(|(indent, _)| *indent >= old) {
                parents.pop();
            }
            let desired = parents.last().map(|(_, indent)| indent + 2).unwrap_or(0);
            parents.push((old, desired));
            if old != desired && selected.iter().any(|range| range.start == start) {
                changes.push((start, old, desired));
            }
        }
        if changes.is_empty() {
            return;
        }
        self.remember(text);
        for (start, old, desired) in changes.into_iter().rev() {
            text.replace_range(start..start + old, &" ".repeat(desired));
            for s in &mut self.selections {
                for at in [&mut s.anchor, &mut s.head] {
                    if *at >= start {
                        *at = at.saturating_sub(old).max(start) + desired;
                    }
                }
            }
        }
        self.reveal = true;
    }
    fn comment(&mut self, text: &mut String) {
        let rows = self.selected_lines(text);
        let uncomment = rows.iter().all(|r| text[r.clone()].trim_start().starts_with('#'));
        self.remember(text);
        for row in rows.into_iter().rev() {
            let value = &text[row.clone()];
            let at = row.start + value.len() - value.trim_start().len();
            let remove = if uncomment {
                if text[at..].starts_with("# ") { 2 } else { 1 }
            } else {
                0
            };
            let insert = if uncomment { "" } else { "# " };
            text.replace_range(at..at + remove, insert);
            for s in &mut self.selections {
                for p in [&mut s.anchor, &mut s.head] {
                    if *p >= at {
                        *p = p.saturating_sub(remove).max(at) + insert.len();
                    }
                }
            }
        }
        self.reveal = true;
    }
    fn move_lines(&mut self, text: &mut String, down: bool, copy: bool) {
        let starts = lines(text);
        let rows = self.selected_lines(text);
        let Some(first) = rows.first() else {
            return;
        };
        let last = rows.last().unwrap();
        let start = first.start;
        let end = last.end;
        let a = line_at(&starts, start);
        let b = line_at(&starts, end.saturating_sub(1));
        if !copy && ((!down && a == 0) || (down && b + 1 >= starts.len())) {
            return;
        }
        self.remember(text);
        let block = text[start..end].to_owned();
        let delta: isize = if copy {
            let insertion = if down { end } else { start };
            let value = if end == text.len() && !block.ends_with('\n') {
                if down {
                    format!("\n{block}")
                } else {
                    format!("{block}\n")
                }
            } else {
                block
            };
            text.insert_str(insertion, &value);
            if down { value.len() as isize } else { 0 }
        } else if down {
            let after = starts.get(b + 2).copied().unwrap_or(text.len());
            let next = text[end..after].to_owned();
            let next_len = next.len();
            let value = if !next.ends_with('\n') {
                format!("{next}\n{}", block.strip_suffix('\n').unwrap_or(&block))
            } else {
                format!("{next}{block}")
            };
            text.replace_range(start..after, &value);
            (next_len + usize::from(!next.ends_with('\n'))) as isize
        } else {
            let before = starts[a - 1];
            let prior = text[before..start].to_owned();
            let value = if !block.ends_with('\n') {
                format!("{block}\n{}", prior.strip_suffix('\n').unwrap_or(&prior))
            } else {
                format!("{block}{prior}")
            };
            text.replace_range(before..end, &value);
            -(prior.len() as isize)
        };
        for s in &mut self.selections {
            s.anchor = s.anchor.checked_add_signed(delta).unwrap_or(0).min(text.len());
            s.head = s.head.checked_add_signed(delta).unwrap_or(0).min(text.len());
        }
        let count = b - a + 1;
        let after_starts = lines(text);
        self.folds = self
            .folds
            .iter()
            .map(|&row| {
                if copy {
                    let insertion = if down { b + 1 } else { a };
                    if row >= insertion { row + count } else { row }
                } else if down {
                    if (a..=b).contains(&row) {
                        row + 1
                    } else if row == b + 1 {
                        a
                    } else {
                        row
                    }
                } else if (a..=b).contains(&row) {
                    row - 1
                } else if row + 1 == a {
                    b
                } else {
                    row
                }
            })
            .filter(|&row| fold_end(text, &after_starts, row).is_some())
            .collect();
        self.reveal = true;
    }
    fn key(&mut self, text: &mut String, key: Key, m: Modifiers, page: usize) -> bool {
        let before = self.selections.clone();
        let primary = self.primary;
        let generation = self.edit_generation;
        let handled = self.handle_key(text, key, m, page);
        let selection_history = key == Key::U && (m.command || m.alt);
        if handled && self.edit_generation == generation && !selection_history {
            self.remember_selection(before, primary, "key");
        }
        handled
    }
    fn handle_key(&mut self, text: &mut String, mut key: Key, mut m: Modifiers, page: usize) -> bool {
        self.normalize(text);
        if key == Key::Escape && self.drag_source.take().is_some() {
            return true;
        }
        if key == Key::U && m.command {
            self.selection_history(text, cfg!(target_os = "macos") && m.shift);
            return true;
        }
        if key == Key::U && m.alt && !cfg!(target_os = "macos") {
            self.selection_history(text, true);
            return true;
        }
        let starts = lines(text);
        let head = self.main().head;
        let row = line_at(&starts, head);
        if (cfg!(target_os = "macos") && m.alt && m.shift && key == Key::M)
            || (!cfg!(target_os = "macos") && m.ctrl && key == Key::M)
        {
            self.tab_focus = !self.tab_focus;
            return true;
        }
        if m.alt && matches!(key, Key::ArrowUp | Key::ArrowDown) {
            if m.command {
                let add: Vec<_> = self
                    .selections
                    .iter()
                    .filter_map(|s| {
                        let row = line_at(&starts, s.head);
                        let target = if key == Key::ArrowUp {
                            row.checked_sub(1)?
                        } else {
                            row + 1
                        };
                        (target < starts.len()).then(|| {
                            Selection::caret(position(
                                text,
                                &starts,
                                target,
                                visual_column(&text[starts[row]..s.head]),
                            ))
                        })
                    })
                    .collect();
                self.selections.extend(add);
                self.primary = None;
                self.reveal = true;
            } else {
                self.move_lines(text, key == Key::ArrowDown, m.shift);
            }
            return true;
        }
        if (key == Key::L && if cfg!(target_os = "macos") { m.ctrl } else { m.alt })
            || (m.command && m.shift && key == Key::K)
        {
            self.selections = self
                .selected_lines(text)
                .into_iter()
                .map(|r| Selection {
                    anchor: r.start,
                    head: r.end,
                })
                .collect();
            if key == Key::K {
                self.replace(text, "");
            }
            return true;
        }
        if m.command && key == Key::Enter {
            self.selections = self
                .selections
                .iter()
                .map(|s| Selection::caret(line_end(text, &starts, line_at(&starts, s.head))))
                .collect();
            key = Key::Enter;
            m.command = false;
            m.mac_cmd = false;
            m.ctrl = false;
        }
        if m.command && (key == Key::Slash || (m.alt && m.shift && key == Key::A)) {
            self.comment(text);
            return true;
        }
        if m.command && (key == Key::D || (m.shift && key == Key::L)) {
            let mut range = self.main().range();
            if range.is_empty() {
                let start = word_edge(text, head, false);
                let end = word_edge(text, start, true);
                range = start..end;
                self.select(range);
            } else {
                let value = &text[range.clone()];
                let all: Vec<_> = text
                    .match_indices(value)
                    .map(|(at, value)| Selection {
                        anchor: at,
                        head: at + value.len(),
                    })
                    .collect();
                if key == Key::L {
                    self.selections = all;
                    self.primary = None;
                } else if let Some(next) = all
                    .iter()
                    .find(|s| s.anchor >= range.end && !self.selections.contains(s))
                    .or_else(|| all.iter().find(|s| !self.selections.contains(s)))
                {
                    self.selections.push(*next);
                    self.primary = None;
                }
                self.reveal = true;
            }
            return true;
        }
        if m.command && key == Key::Backslash && m.shift {
            if let Some((a, b)) = bracket_pair(text, head, &scalar_rows(text)) {
                self.select(if head <= a + 1 { b..b } else { a..a });
            }
            return true;
        }
        if m.command && key == Key::Backslash && m.alt {
            self.indent_selection(text);
            return true;
        }
        if m.command && key == Key::I {
            let mut start = starts[row];
            while start < text.len() && text[start..].starts_with(' ') {
                start += 1;
            }
            let end = fold_end(text, &starts, row)
                .map(|end| line_end(text, &starts, end))
                .unwrap_or_else(|| line_end(text, &starts, row));
            self.select(start..end);
            return true;
        }
        if cfg!(target_os = "macos") && m.ctrl && !m.mac_cmd {
            let emacs = match key {
                Key::B => Some(Key::ArrowLeft),
                Key::F => Some(Key::ArrowRight),
                Key::P => Some(Key::ArrowUp),
                Key::N => Some(Key::ArrowDown),
                Key::A => Some(Key::Home),
                Key::E => Some(Key::End),
                Key::D => Some(Key::Delete),
                Key::H => Some(Key::Backspace),
                Key::V => Some(Key::PageDown),
                _ => None,
            };
            if let Some(k) = emacs {
                key = k;
                m.ctrl = false;
            } else if key == Key::K {
                for s in &mut self.selections {
                    let end = line_end(text, &starts, line_at(&starts, s.head));
                    s.anchor = if end == s.head { next(text, end) } else { end };
                }
                self.replace(text, "");
                return true;
            } else if key == Key::O {
                let cursors = self.selections.clone();
                self.replace(text, "\n");
                self.selections = cursors;
                return true;
            } else if key == Key::T {
                let end = if head == line_end(text, &starts, row) {
                    head
                } else {
                    next(text, head)
                };
                let mid = previous(text, end);
                let start = previous(text, mid);
                if start < mid {
                    let swapped = format!("{}{}", &text[mid..end], &text[start..mid]);
                    self.select(start..end);
                    self.replace(text, &swapped);
                }
                return true;
            }
        }
        if m.command {
            match key {
                Key::A => {
                    self.select(0..text.len());
                    return true;
                }
                Key::Z => {
                    self.history(text, m.shift);
                    return true;
                }
                Key::Y if !cfg!(target_os = "macos") => {
                    self.history(text, true);
                    return true;
                }
                Key::OpenBracket | Key::CloseBracket if m.alt => {
                    if m.shift {
                        if key == Key::CloseBracket {
                            self.folds.clear();
                        } else {
                            self.folds = (0..starts.len())
                                .filter(|&i| fold_end(text, &starts, i).is_some())
                                .collect();
                        }
                    } else if key == Key::CloseBracket {
                        self.folds.remove(&row);
                    } else if fold_end(text, &starts, row).is_some() {
                        self.folds.insert(row);
                    }
                    return true;
                }
                Key::OpenBracket | Key::CloseBracket if m.shift && !cfg!(target_os = "macos") => {
                    if key == Key::CloseBracket {
                        self.folds.remove(&row);
                    } else if fold_end(text, &starts, row).is_some() {
                        self.folds.insert(row);
                    }
                    return true;
                }
                Key::OpenBracket | Key::CloseBracket => {
                    self.indent(text, key == Key::OpenBracket);
                    return true;
                }
                _ => {}
            }
        }
        if key == Key::Tab && !self.tab_focus {
            self.indent(text, m.shift);
            return true;
        }
        if key == Key::Enter && !m.command {
            let mut changes = self.selections.clone();
            self.remember(text);
            for s in changes.iter_mut().rev() {
                let range = s.range();
                let row = line_at(&starts, range.start);
                let before = &text[starts[row]..range.start];
                let indent = before.chars().take_while(|c| *c == ' ').count();
                let extra = usize::from(before.trim_end().ends_with(':')) * 2;
                let insert = format!("\n{}", " ".repeat(indent + extra));
                self.map_folds(text, range.clone(), &insert);
                text.replace_range(range.clone(), &insert);
                let delta = insert.len() as isize - range.len() as isize;
                for cursor in &mut self.selections {
                    if cursor.range().start == range.start {
                        *cursor = Selection::caret(range.start + insert.len());
                    } else if cursor.range().start > range.start {
                        cursor.anchor = cursor.anchor.checked_add_signed(delta).unwrap();
                        cursor.head = cursor.head.checked_add_signed(delta).unwrap();
                    }
                }
            }
            self.reveal = true;
            return true;
        }
        if matches!(key, Key::Backspace | Key::Delete) {
            for s in &mut self.selections {
                if s.range().is_empty() {
                    let row = line_at(&starts, s.head);
                    if key == Key::Backspace {
                        s.anchor = if m.command && cfg!(target_os = "macos") {
                            starts[row]
                        } else if m.alt || m.ctrl {
                            word_edge(text, s.head, false)
                        } else {
                            previous(text, s.head)
                        };
                    } else {
                        s.head = if m.alt || m.ctrl {
                            word_edge(text, s.head, true)
                        } else {
                            next(text, s.head)
                        };
                    }
                }
            }
            self.replace(text, "");
            return true;
        }
        if key == Key::Escape && self.selections.len() > 1 {
            self.selections = vec![self.main()];
            self.primary = None;
            return true;
        }
        if !matches!(
            key,
            Key::ArrowLeft
                | Key::ArrowRight
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
        ) {
            return false;
        }
        let column = self.column.unwrap_or_else(|| visual_column(&text[starts[row]..head]));
        let vertical = matches!(key, Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown);
        let visible: Vec<_> = (0..starts.len())
            .filter(|row| {
                !self
                    .folds
                    .iter()
                    .any(|&fold| *row > fold && fold_end(text, &starts, fold).is_some_and(|end| *row <= end))
            })
            .collect();
        for s in &mut self.selections {
            let row = line_at(&starts, s.head);
            let vrow = visible.partition_point(|&r| r <= row).saturating_sub(1);
            let boundary = if m.command && cfg!(target_os = "macos") {
                Some(match key {
                    Key::ArrowLeft => starts[row],
                    Key::ArrowRight => line_end(text, &starts, row),
                    Key::ArrowUp | Key::Home => 0,
                    Key::ArrowDown | Key::End => text.len(),
                    _ => s.head,
                })
            } else {
                None
            };
            let at = boundary.unwrap_or_else(|| match key {
                Key::ArrowLeft if !m.shift && !s.range().is_empty() => s.range().start,
                Key::ArrowRight if !m.shift && !s.range().is_empty() => s.range().end,
                Key::ArrowLeft => {
                    if m.alt || m.ctrl {
                        word_edge(text, s.head, false)
                    } else {
                        previous(text, s.head)
                    }
                }
                Key::ArrowRight => {
                    if m.alt || m.ctrl {
                        word_edge(text, s.head, true)
                    } else {
                        next(text, s.head)
                    }
                }
                Key::ArrowUp => position(text, &starts, visible[vrow.saturating_sub(1)], column),
                Key::ArrowDown => position(text, &starts, visible[(vrow + 1).min(visible.len() - 1)], column),
                Key::PageUp => position(text, &starts, visible[vrow.saturating_sub(page)], column),
                Key::PageDown => position(text, &starts, visible[(vrow + page).min(visible.len() - 1)], column),
                Key::Home => {
                    if m.ctrl {
                        0
                    } else {
                        let indent = text[starts[row]..line_end(text, &starts, row)]
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .count();
                        let first = position(text, &starts, row, indent);
                        if s.head == first { starts[row] } else { first }
                    }
                }
                Key::End => {
                    if m.ctrl {
                        text.len()
                    } else {
                        line_end(text, &starts, row)
                    }
                }
                _ => s.head,
            });
            s.head = at;
            if !m.shift {
                s.anchor = at;
            }
        }
        self.column = vertical.then_some(column);
        self.folds.retain(|&fold| {
            !self.selections.iter().any(|s| {
                let row = line_at(&starts, s.head);
                row > fold && fold_end(text, &starts, fold).is_some_and(|end| row <= end)
            })
        });
        self.reveal = true;
        true
    }
    pub fn show(&mut self, ui: &mut Ui, rect: Rect, text: &mut String, matches: &[Range<usize>], t: Tokens) {
        self.normalize(text);
        let starts = lines(text);
        if self.reveal {
            self.folds.retain(|&fold| {
                !self.selections.iter().any(|s| {
                    let row = line_at(&starts, s.head);
                    row > fold && fold_end(text, &starts, fold).is_some_and(|end| row <= end)
                })
            });
        }
        let id = Id::new("yaml-editor");
        text_edit::TextEditState::default().store(ui.ctx(), id);
        let response = ui.interact(rect, id, Sense::click_and_drag());
        if ui.memory(|m| m.focused().is_none()) {
            response.request_focus();
        }
        response.widget_info(|| WidgetInfo::text_edit(ui.is_enabled(), text.as_str(), text.as_str(), ""));
        let focused = response.has_focus();
        if ui.input(|i| !i.events.is_empty()) {
            self.last_input = ui.input(|i| i.time);
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(if ui.input(|i| i.modifiers.alt) {
                CursorIcon::Crosshair
            } else {
                CursorIcon::Text
            });
        }
        if focused {
            for event in ui.input(|i| i.events.clone()) {
                match event {
                    Event::Text(value) => self.type_text(text, &value, ui.input(|i| i.time)),
                    Event::Paste(value) => {
                        self.typing_cursors.clear();
                        self.paste(text, &value);
                    }
                    Event::Copy | Event::Cut => {
                        let cut = matches!(event, Event::Cut);
                        let copy = self.clipboard(text, cut);
                        ui.ctx().copy_text(copy);
                    }
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if self.key(text, key, modifiers, (rect.height() / LINE_HEIGHT).floor() as usize) => {
                        self.typing_cursors.clear();
                        ui.input_mut(|i| {
                            i.consume_key(modifiers, key);
                        });
                    }
                    Event::Ime(ImeEvent::Preedit { text, .. }) => self.preedit = text,
                    Event::Ime(ImeEvent::Commit(value)) => {
                        self.preedit.clear();
                        self.replace(text, &value);
                    }
                    _ => {}
                }
            }
        }
        let starts = lines(text);
        let blocks = scalar_rows(text);
        let pair = bracket_pair(text, self.main().head, &blocks);
        let selected = self.main().range();
        let selection_matches: Vec<_> = if selected.len() >= 2 && !text[selected.clone()].contains('\n') {
            text.match_indices(&text[selected.clone()])
                .filter_map(|(start, value)| {
                    let range = start..start + value.len();
                    (range != selected).then_some(range)
                })
                .collect()
        } else {
            vec![]
        };
        let char_width = text_width(ui, " ", 12.5, "mono");
        let gutter = starts.len().to_string().len() as f32 * char_width + 24.0;
        let mut visible = vec![];
        let mut row = 0;
        while row < starts.len() {
            visible.push(row);
            row = if self.folds.contains(&row) {
                fold_end(text, &starts, row).map(|r| r + 1).unwrap_or(row + 1)
            } else {
                row + 1
            };
        }
        let width = text.split('\n').map(visual_column).max().unwrap_or(0) as f32 * char_width + gutter + 12.0;
        let selections_before_pointer = self.selections.clone();
        let primary_before_pointer = self.primary;
        let mut dropped = None;
        ui.scope_builder(
            UiBuilder::new().id_salt("yaml-text-scroll-scope").max_rect(rect),
            |ui| {
                ui.set_clip_rect(rect);
                ScrollArea::both()
                    .id_salt("yaml-editor-scroll")
                    .auto_shrink([false, false])
                    .show_viewport(ui, |ui, viewport| {
                        let origin = ui.cursor().min;
                        let gutter_x = rect.left();
                        let mut gutter_ui = ui.new_child(UiBuilder::new().id_salt("gutter"));
                        gutter_ui.set_clip_rect(super::style::rect(gutter_x, rect.top(), gutter, rect.height()));
                        gutter_ui.painter().rect_filled(gutter_ui.clip_rect(), 0, t.bg);
                        let content_painter = ui.painter().with_clip_rect(Rect::from_min_max(
                            pos2(gutter_x + gutter, ui.clip_rect().top()),
                            ui.clip_rect().max,
                        ));
                        let highlight_painter = ui.painter().with_clip_rect(Rect::from_min_max(
                            pos2(gutter_x + gutter - 6.0, ui.clip_rect().top()),
                            ui.clip_rect().max,
                        ));
                        ui.allocate_space(vec2(
                            width.max(rect.width() - 12.0),
                            visible.len() as f32 * LINE_HEIGHT + 20.0,
                        ));
                        let index_at = |p: Pos2| {
                            let visible_row = (((p.y - origin.y - 10.0) / LINE_HEIGHT).floor().max(0.0) as usize)
                                .min(visible.len() - 1);
                            let row = visible[visible_row];
                            let column = ((p.x - origin.x - gutter) / char_width).round().max(0.0) as usize;
                            (position(text, &starts, row, column), row, column)
                        };
                        if let Some(p) = ui.input(|i| i.pointer.interact_pos())
                            && (response.clicked_by(PointerButton::Primary) || response.drag_started())
                        {
                            self.typing_cursors.clear();
                            response.request_focus();
                            let p = if response.drag_started() {
                                ui.input(|i| i.pointer.press_origin()).unwrap_or(p)
                            } else {
                                p
                            };
                            let (at, row, column) = index_at(p);
                            let m = ui.input(|i| i.modifiers);
                            let selected = self.main().range();
                            if response.drag_started()
                                && !selected.is_empty()
                                && selected.contains(&at)
                                && !m.shift
                                && !m.mac_cmd
                            {
                                self.drag_source = Some(selected);
                            } else if p.x < gutter_x + gutter - 4.0
                                && p.x > gutter_x + gutter - 18.0
                                && fold_end(text, &starts, row).is_some()
                            {
                                if !self.folds.remove(&row) {
                                    self.folds.insert(row);
                                }
                            } else if response.triple_clicked() {
                                self.selections = vec![Selection {
                                    anchor: starts[row],
                                    head: starts.get(row + 1).copied().unwrap_or(text.len()),
                                }];
                                self.primary = None;
                            } else if response.double_clicked() {
                                self.selections = vec![Selection {
                                    anchor: word_edge(text, at, false),
                                    head: word_edge(text, at, true),
                                }];
                                self.primary = None;
                            } else {
                                if m.alt {
                                    self.rectangle = Some((row, column));
                                } else {
                                    self.rectangle = None;
                                }
                                self.drag_anchor = if m.shift { self.main().anchor } else { at };
                                let selection = Selection {
                                    anchor: self.drag_anchor,
                                    head: at,
                                };
                                if m.command {
                                    if self.selections.len() > 1
                                        && let Some(index) = self
                                            .selections
                                            .iter()
                                            .position(|s| s.range().start <= at && at <= s.range().end)
                                    {
                                        let primary = self.main_index();
                                        self.selections.remove(index);
                                        self.primary = Some(if primary == index {
                                            0
                                        } else {
                                            primary.saturating_sub(usize::from(primary > index))
                                        });
                                    } else {
                                        self.selections.push(selection);
                                        self.primary = None;
                                    }
                                } else {
                                    self.selections = vec![selection];
                                    self.primary = None;
                                }
                            }
                        }
                        if response.dragged()
                            && let Some(p) = ui.input(|i| i.pointer.interact_pos())
                        {
                            let (at, row, column) = index_at(p);
                            if self.drag_source.is_some() {
                                if let Some(vrow) = visible.iter().position(|r| *r == row) {
                                    let x =
                                        origin.x + gutter + visual_column(&text[starts[row]..at]) as f32 * char_width;
                                    let y = origin.y + 10.0 + vrow as f32 * LINE_HEIGHT;
                                    content_painter
                                        .line_segment([pos2(x, y), pos2(x, y + LINE_HEIGHT)], Stroke::new(1.0, t.text));
                                }
                                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                            } else if let Some((first_row, first_column)) = self.rectangle {
                                self.selections = (first_row.min(row)..=first_row.max(row))
                                    .map(|row| Selection {
                                        anchor: position(text, &starts, row, first_column),
                                        head: position(text, &starts, row, column),
                                    })
                                    .collect();
                                self.primary = Some(row - first_row.min(row));
                            } else {
                                let index = self.main_index();
                                if let Some(s) = self.selections.get_mut(index) {
                                    s.head = at;
                                }
                            }
                            let edge_delta = |value: f32, min: f32, max: f32| {
                                if value < min {
                                    (min - value).min(30.0)
                                } else if value > max {
                                    -(value - max).min(30.0)
                                } else {
                                    0.0
                                }
                            };
                            let delta = vec2(
                                edge_delta(p.x, rect.left() + gutter, rect.right()),
                                edge_delta(p.y, rect.top(), rect.bottom()),
                            );
                            if delta != Vec2::ZERO {
                                ui.scroll_with_delta_animation(delta, eframe::egui::style::ScrollAnimation::none());
                                ui.ctx().request_repaint();
                            }
                        }
                        if response.drag_stopped()
                            && let Some(source) = self.drag_source.take()
                            && let Some(p) = ui.input(|i| i.pointer.interact_pos())
                            && rect.contains(p)
                        {
                            let at = index_at(p).0;
                            let copy = ui.input(|i| {
                                if cfg!(target_os = "macos") {
                                    i.modifiers.alt
                                } else {
                                    i.modifiers.ctrl
                                }
                            });
                            dropped = Some((source, at, copy));
                        }
                        let first = (viewport.top() / LINE_HEIGHT).floor().max(0.0) as usize;
                        let last = ((viewport.bottom() / LINE_HEIGHT).ceil() as usize + 1).min(visible.len());
                        let primary = self.main();
                        let active = line_at(&starts, primary.head);
                        for (vrow, &row) in visible.iter().enumerate().take(last).skip(first.saturating_sub(1)) {
                            let y = origin.y + 10.0 + vrow as f32 * LINE_HEIGHT;
                            let start = starts[row];
                            let end = line_end(text, &starts, row);
                            let x = origin.x + gutter;
                            let rr = super::style::rect(x, y, width.max(rect.width()) - gutter, LINE_HEIGHT);
                            if row == active {
                                highlight_painter.rect_filled(
                                    Rect::from_min_max(rr.min + vec2(-6.0, 0.5), rr.max + vec2(0.0, 0.5)),
                                    0,
                                    t.hover,
                                );
                            }
                            let number = (row + 1).to_string();
                            let number_width = text_width(ui, &number, 12.5, "mono");
                            let number_color = if row == active { t.muted } else { t.faint };
                            gutter_ui.painter().galley(
                                pos2(gutter_x + gutter - 19.0 - number_width, y + 3.4),
                                gutter_ui
                                    .painter()
                                    .layout_job(text_job(&number, 12.5, "mono", number_color)),
                                number_color,
                            );
                            if fold_end(text, &starts, row).is_some() {
                                let marker = if self.folds.contains(&row) { "›" } else { "⌄" };
                                gutter_ui.painter().galley(
                                    pos2(gutter_x + gutter - 15.0, y + 3.4),
                                    gutter_ui.painter().layout_job(text_job(marker, 12.5, "mono", t.faint)),
                                    t.faint,
                                );
                            }
                            let selection_rect = |range: Range<usize>| {
                                let left =
                                    visual_column(&text[start..range.start.clamp(start, end)]) as f32 * char_width;
                                let right = visual_column(&text[start..range.end.clamp(start, end)]) as f32
                                    * char_width
                                    + if range.end > end { char_width } else { 0.0 };
                                super::style::rect(x + left, y, (right - left).max(0.0), LINE_HEIGHT)
                            };
                            for range in matches.iter().filter(|r| r.start <= end && r.end >= start) {
                                content_painter.rect_filled(
                                    selection_rect(range.clone()),
                                    0,
                                    t.progress.gamma_multiply(0.3),
                                );
                            }
                            for range in selection_matches.iter().filter(|r| r.start <= end && r.end >= start) {
                                content_painter.rect_filled(
                                    selection_rect(range.clone()),
                                    0,
                                    Color32::from_rgba_unmultiplied(153, 255, 119, 128),
                                );
                            }
                            for s in &self.selections {
                                let range = s.range();
                                if !range.is_empty() && range.start <= end && range.end >= start {
                                    content_painter.rect_filled(selection_rect(range), 0, t.accent.gamma_multiply(0.3));
                                }
                            }
                            if let Some((a, b)) = pair {
                                for index in [a, b] {
                                    if index >= start && index < end {
                                        content_painter.rect_filled(
                                            selection_rect(index..index + 1),
                                            0,
                                            Color32::from_rgba_unmultiplied(50, 140, 130, 82),
                                        );
                                    }
                                }
                            }
                            let (shown, special) = display_line(&text[start..end]);
                            let mut job = if blocks.contains(&row) {
                                text_job(&shown, 12.5, "mono", t.string)
                            } else {
                                super::yaml::yaml_job(&shown, t)
                            };
                            if !special.is_empty() {
                                for section in std::mem::take(&mut job.sections) {
                                    let section_start = usize::from(section.byte_range.start);
                                    let section_end = usize::from(section.byte_range.end);
                                    let mut boundaries = vec![section_start, section_end];
                                    for (range, _) in &special {
                                        if range.start < section_end && range.end > section_start {
                                            boundaries
                                                .extend([range.start.max(section_start), range.end.min(section_end)]);
                                        }
                                    }
                                    boundaries.sort_unstable();
                                    boundaries.dedup();
                                    for bounds in boundaries.windows(2) {
                                        let mut piece = section.clone();
                                        piece.byte_range = bounds[0].into()..bounds[1].into();
                                        if bounds[0] != section_start {
                                            piece.leading_space = 0.0;
                                        }
                                        if special.iter().any(|(range, _)| range.contains(&bounds[0])) {
                                            piece.format.color = Color32::RED;
                                        }
                                        job.sections.push(piece);
                                    }
                                }
                            }
                            job.wrap.max_width = f32::INFINITY;
                            let galley = ui.painter().layout_job(job);
                            content_painter.galley(pos2(x, y + 3.4), galley, t.text);
                            for (range, character) in special {
                                let left = shown[..range.start].chars().count() as f32 * char_width;
                                ui.interact(
                                    super::style::rect(x + left, y, char_width, LINE_HEIGHT),
                                    id.with(("special-character", row, range.start)),
                                    Sense::hover(),
                                )
                                .on_hover_text(special_description(character));
                            }
                            if self.folds.contains(&row) {
                                let px = x + visual_column(&text[start..end]) as f32 * char_width + 4.0;
                                let placeholder = super::style::rect(px, y + 2.0, 16.0, LINE_HEIGHT - 4.0);
                                ui.painter().rect_filled(placeholder, 3, t.selected);
                                if ui
                                    .interact(placeholder, id.with(("unfold", row)), Sense::click())
                                    .clicked()
                                {
                                    self.folds.remove(&row);
                                }
                                label(
                                    ui,
                                    super::style::rect(px, y, 16.0, LINE_HEIGHT),
                                    "…",
                                    12.5,
                                    "mono",
                                    t.muted,
                                );
                            }
                        }
                        for s in &self.selections {
                            let row = line_at(&starts, s.head);
                            if let Some(vrow) = visible.iter().position(|r| *r == row) {
                                let x =
                                    origin.x + gutter + visual_column(&text[starts[row]..s.head]) as f32 * char_width;
                                let y = origin.y + 10.0 + vrow as f32 * LINE_HEIGHT;
                                let cursor = super::style::rect(x, y, 1.0, LINE_HEIGHT);
                                if focused && (ui.input(|i| i.time) - self.last_input) % 1.2 < 0.6 {
                                    content_painter.line_segment(
                                        [cursor.left_top(), cursor.left_bottom()],
                                        Stroke::new(1.0, t.text),
                                    );
                                }
                                if self.reveal {
                                    ui.scroll_to_rect(cursor.expand2(vec2(8.0, 3.0)), None);
                                }
                                if *s == primary && focused {
                                    ui.output_mut(|o| {
                                        o.mutable_text_under_cursor = true;
                                        o.ime = Some(eframe::egui::output::IMEOutput {
                                            purpose: IMEPurpose::Normal,
                                            rect,
                                            cursor_rect: cursor,
                                            should_interrupt_composition: false,
                                        });
                                    });
                                    if !self.preedit.is_empty() {
                                        label(
                                            ui,
                                            super::style::rect(x, y, 300.0, LINE_HEIGHT),
                                            &self.preedit,
                                            12.5,
                                            "mono",
                                            t.text,
                                        );
                                    }
                                }
                            }
                        }
                        self.reveal = false;
                        if focused {
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(600));
                        }
                    });
            },
        );
        self.remember_selection(selections_before_pointer, primary_before_pointer, "pointer");
        if let Some((source, at, copy)) = dropped {
            self.drop_selection(text, source, at, copy);
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adding_an_earlier_cursor_preserves_the_primary_cursor_and_history() {
        let mut text = "first: one\nsecond: two".to_owned();
        let mut editor = Editor {
            selections: vec![Selection::caret(20), Selection::caret(4)],
            ..Default::default()
        };
        editor.normalize(&text);
        assert_eq!(editor.main().head, 4);
        editor.replace(&mut text, "x");
        assert_eq!(editor.main().head, 5);
        editor.history(&mut text, false);
        assert_eq!(editor.main().head, 4);
        editor.select(1..1);
        editor.selection_history(&mut text, false);
        assert_eq!(editor.main().head, 4);
        editor.key(&mut text, Key::Escape, Modifiers::NONE, 20);
        assert_eq!(editor.selections, vec![Selection::caret(4)]);
    }
    #[test]
    fn clipboard_copies_whole_lines_once_and_pastes_them_at_line_start() {
        let mut text = "first: café\nsecond: two\nthird: three".to_owned();
        let mut editor = Editor {
            selections: vec![Selection::caret(3), Selection::caret(5), Selection::caret(15)],
            ..Default::default()
        };
        let copied = editor.clipboard(&mut text, false);
        assert_eq!(copied, "first: café\nsecond: two");
        editor.select(30..30);
        editor.paste(&mut text, &copied);
        assert_eq!(text, "first: café\nsecond: two\nfirst: café\nsecond: two\nthird: three");
        assert_eq!(editor.selections[0].head, 30 + copied.len() + 1);
        editor.history(&mut text, false);
        assert_eq!(text, "first: café\nsecond: two\nthird: three");
        editor.select(15..15);
        let copied = editor.clipboard(&mut text, true);
        assert_eq!(copied, "second: two");
        assert_eq!(text, "first: café\nthird: three");
        editor.history(&mut text, false);
        assert_eq!(editor.selections, vec![Selection::caret(15)]);
    }
    #[test]
    fn multiline_paste_distributes_lines_across_multiple_cursors() {
        let mut text = "first: old\nsecond: old".to_owned();
        let mut editor = Editor {
            selections: vec![Selection { anchor: 7, head: 10 }, Selection { anchor: 19, head: 22 }],
            ..Default::default()
        };
        let before = editor.selections.clone();
        editor.paste(&mut text, "café\r\nnew");
        assert_eq!(text, "first: café\nsecond: new");
        editor.history(&mut text, false);
        assert_eq!(text, "first: old\nsecond: old");
        assert_eq!(editor.selections, before);
        editor.paste(&mut text, "one\ntwo\nthree");
        assert_eq!(text, "first: one\ntwo\nthree\nsecond: one\ntwo\nthree");
    }
    #[test]
    fn moving_and_copying_folded_blocks_preserves_unrelated_folds() {
        let original = "first:\n  one: 1\nlast: x\nsecond:\n  two: 2";
        for (copy, expected) in [(false, BTreeSet::from([1, 3])), (true, BTreeSet::from([0, 5]))] {
            let mut text = original.to_owned();
            let mut editor = Editor {
                selections: vec![Selection {
                    anchor: 0,
                    head: original.find("last:").unwrap(),
                }],
                folds: BTreeSet::from([0, 3]),
                ..Default::default()
            };
            editor.move_lines(&mut text, true, copy);
            assert_eq!(editor.folds, expected);
            editor.history(&mut text, false);
            assert_eq!(text, original);
            assert_eq!(editor.folds, BTreeSet::from([0, 3]));
        }
    }
    #[test]
    fn tabs_and_control_markers_keep_the_document_and_caret_byte_positions() {
        let ctx = Context::default();
        install_fonts(&ctx);
        Tokens::new(true).apply(&ctx, crate::model::Theme::Dark);
        let bounds = super::super::style::rect(0.0, 0.0, 800.0, 400.0);
        let original = "name: \"a\tb\u{200b}\"\nsecond: café";
        let mut text = original.to_owned();
        let mut editor = Editor::default();
        let mut run = |events| {
            let mut width = 0.0;
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(bounds),
                    events,
                    ..Default::default()
                },
                |ui| {
                    width = text_width(ui, " ", 12.5, "mono");
                    editor.show(ui, bounds, &mut text, &[], Tokens::new(true));
                },
            );
            output.textures_delta.clear();
            width
        };
        let width = run(vec![]);
        let point = pos2(24.0 + width + 10.0 * width, 20.0);
        run(vec![
            Event::PointerMoved(point),
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        run(vec![Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        assert_eq!(text, original);
        assert_eq!(editor.selections[0].head, original.find('b').unwrap());
        assert_eq!(display_line("a\t\0\u{200b}").0, "a ␀•");
        assert_eq!(special_description('\u{200b}'), "Control character zero width space");
    }
    #[test]
    fn selection_history_restores_cursors_before_undoing_text() {
        let mut text = "one\ntwo".to_owned();
        let mut editor = Editor::default();
        editor.normalize(&text);
        editor.replace(&mut text, "prefix ");
        let after_typing = editor.selections.clone();
        editor.last_input = 1.0;
        editor.key(&mut text, Key::ArrowDown, Modifiers::NONE, 20);
        assert_ne!(editor.selections, after_typing);
        editor.key(&mut text, Key::U, Modifiers::COMMAND, 20);
        assert_eq!(editor.selections, after_typing);
        assert_eq!(text, "prefix one\ntwo");
        editor.key(&mut text, Key::U, Modifiers::COMMAND, 20);
        assert_eq!(text, "one\ntwo");
        editor.selection_history(&mut text, true);
        assert_eq!(text, "prefix one\ntwo");
        assert_eq!(editor.selections, after_typing);
    }
    #[test]
    fn document_undo_skips_selection_changes_and_redo_restores_the_edit_cursor() {
        let mut text = "one\ntwo".to_owned();
        let mut editor = Editor::default();
        editor.normalize(&text);
        editor.replace(&mut text, "prefix ");
        let after_typing = editor.selections.clone();
        editor.last_input = 1.0;
        editor.key(&mut text, Key::ArrowDown, Modifiers::NONE, 20);
        editor.history(&mut text, false);
        assert_eq!(text, "one\ntwo");
        editor.history(&mut text, true);
        assert_eq!(text, "prefix one\ntwo");
        assert_eq!(editor.selections, after_typing);
    }
    #[test]
    fn moving_or_copying_selected_text_preserves_unicode_and_undo() {
        let original = "name: café\nother: tea";
        for (at, copy, expected) in [
            (0, false, "caféname: \nother: tea"),
            (original.len(), false, "name: \nother: teacafé"),
            (original.len(), true, "name: café\nother: teacafé"),
        ] {
            let mut text = original.to_owned();
            let mut editor = Editor {
                selections: vec![Selection { anchor: 6, head: 11 }],
                ..Default::default()
            };
            editor.drop_selection(&mut text, 6..11, at, copy);
            assert_eq!(text, expected);
            assert_eq!(&text[editor.selections[0].range()], "café");
            editor.history(&mut text, false);
            assert_eq!(text, original);
            assert_eq!(editor.selections, vec![Selection { anchor: 6, head: 11 }]);
        }
    }
    #[test]
    fn dragging_selection_outside_the_editor_scrolls_and_extends_it() {
        let ctx = Context::default();
        install_fonts(&ctx);
        Tokens::new(true).apply(&ctx, crate::model::Theme::Dark);
        let bounds = super::super::style::rect(0.0, 0.0, 800.0, 400.0);
        let mut editor = Editor::default();
        let mut text = (0..100).map(|n| format!("line-{n}: value\n")).collect::<String>();
        let mut run = |events, time| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(bounds),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show(ui, bounds, &mut text, &[], Tokens::new(true));
                },
            );
            output.textures_delta.clear();
        };
        run(vec![], 0.0);
        run(
            vec![
                Event::PointerMoved(pos2(70.0, 20.0)),
                Event::PointerButton {
                    pos: pos2(70.0, 20.0),
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            0.1,
        );
        for frame in 0..40 {
            run(vec![Event::PointerMoved(pos2(100.0, 430.0))], 0.2 + frame as f64 * 0.02);
        }
        assert!(
            line_at(&lines(&text), editor.selections[0].range().end) > 35,
            "selection did not scroll: {:?}",
            editor.selections
        );
    }
    #[test]
    fn dragging_existing_selection_moves_text_at_the_drop_position() {
        let ctx = Context::default();
        install_fonts(&ctx);
        Tokens::new(true).apply(&ctx, crate::model::Theme::Dark);
        let bounds = super::super::style::rect(0.0, 0.0, 800.0, 400.0);
        let mut editor = Editor {
            selections: vec![Selection { anchor: 7, head: 12 }],
            ..Default::default()
        };
        let mut text = "first: café\nsecond: two\nthird: three".to_owned();
        let mut char_width = 0.0;
        let mut run = |events, time| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(bounds),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    char_width = text_width(ui, " ", 12.5, "mono");
                    editor.show(ui, bounds, &mut text, &[], Tokens::new(true));
                },
            );
            output.textures_delta.clear();
            char_width
        };
        let char_width = run(vec![], 0.0);
        let gutter = char_width + 24.0;
        let source = pos2(gutter + 8.0 * char_width, 20.0);
        let destination = pos2(gutter, 10.0 + 2.0 * LINE_HEIGHT + 3.0);
        run(
            vec![
                Event::PointerMoved(source),
                Event::PointerButton {
                    pos: source,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            0.1,
        );
        run(vec![Event::PointerMoved(destination)], 0.2);
        run(
            vec![Event::PointerButton {
                pos: destination,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            0.3,
        );
        assert_eq!(text, "first: \nsecond: two\ncaféthird: three");
        assert_eq!(&text[editor.selections[0].range()], "café");
    }
    #[test]
    fn space_and_enter_do_not_move_the_caret_to_the_mouse_pointer() {
        let ctx = Context::default();
        install_fonts(&ctx);
        let mut editor = Editor::default();
        let mut text = "first: one\nsecond: two\nthird: three".to_owned();
        let bounds = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let run = |editor: &mut Editor, text: &mut String, events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(bounds),
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show(ui, bounds, text, &[], Tokens::new(true));
                },
            );
            output.textures_delta.clear();
        };
        run(&mut editor, &mut text, vec![Event::PointerMoved(pos2(180.0, 48.0))]);
        editor.select(10..10);
        run(
            &mut editor,
            &mut text,
            vec![
                Event::Key {
                    key: Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                Event::Text(" ".into()),
            ],
        );
        assert_eq!(text, "first: one \nsecond: two\nthird: three");
        assert_eq!(editor.selections[0].head, 11);
        run(
            &mut editor,
            &mut text,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(text, "first: one \n\nsecond: two\nthird: three");
        assert_eq!(editor.selections[0].head, 12);
    }
    #[test]
    fn multicursor_edit_and_undo_preserve_utf8() {
        let mut text = "name: café\nname: tea".to_owned();
        let mut editor = Editor {
            selections: vec![Selection { anchor: 6, head: 11 }, Selection { anchor: 18, head: 21 }],
            ..Default::default()
        };
        editor.replace(&mut text, "水");
        assert_eq!(text, "name: 水\nname: 水");
        editor.history(&mut text, false);
        assert_eq!(text, "name: café\nname: tea");
        editor.history(&mut text, true);
        assert_eq!(text, "name: 水\nname: 水");
    }
    #[test]
    fn indent_and_folding_leave_sibling_fields_outside_block() {
        let mut text = "metadata:\n  labels:\n    app: web\nspec:\n  replicas: 2".to_owned();
        assert_eq!(fold_end(&text, &lines(&text), 0), Some(2));
        let mut editor = Editor::default();
        editor.select(10..31);
        editor.indent(&mut text, false);
        assert!(text.contains("    labels:\n      app: web\nspec:"));
        editor.indent(&mut text, true);
        assert_eq!(text, "metadata:\n  labels:\n    app: web\nspec:\n  replicas: 2");
    }
    #[test]
    fn moving_and_copying_the_last_line_keep_newlines_and_cursor() {
        let mut text = "first\ncafé\nlast".to_owned();
        let mut editor = Editor::default();
        editor.select(7..7);
        editor.move_lines(&mut text, true, false);
        assert_eq!(text, "first\nlast\ncafé");
        assert_eq!(editor.selections[0].head, 12);
        editor.move_lines(&mut text, false, false);
        assert_eq!(text, "first\ncafé\nlast");
        editor.select(text.len()..text.len());
        editor.move_lines(&mut text, true, true);
        assert_eq!(text, "first\ncafé\nlast\nlast");
        editor.history(&mut text, false);
        assert_eq!(text, "first\ncafé\nlast");
    }
    #[test]
    fn comments_roundtrip_indentation_and_unicode_selections() {
        let original = "metadata:\n  name: café\n  labels: {}\nspec: {}";
        let mut text = original.to_owned();
        let mut editor = Editor::default();
        editor.select(10..35);
        editor.comment(&mut text);
        assert_eq!(text, "metadata:\n  # name: café\n  # labels: {}\nspec: {}");
        editor.comment(&mut text);
        assert_eq!(text, original);
        assert_eq!(editor.selections[0].range(), 10..35);
    }
    #[test]
    fn matching_brackets_ignore_strings_comments_and_block_scalars() {
        let text = "items: [one, \"[two]\"] # [\nscript: |\n  [ignored\nnext: {}";
        assert_eq!(bracket_pair(text, 7, &scalar_rows(text)), Some((7, 20)));
        let quoted = text.find("[two]").unwrap();
        assert_eq!(bracket_pair(text, quoted, &scalar_rows(text)), None);
        let scalar = text.find("[ignored").unwrap();
        assert_eq!(bracket_pair(text, scalar, &scalar_rows(text)), None);
    }
    #[test]
    fn selecting_next_occurrence_adds_a_cursor_and_edits_all_matches() {
        let mut text = "name: café\nother: café\nlast: café".to_owned();
        let mut editor = Editor::default();
        editor.select(6..11);
        assert!(editor.key(&mut text, Key::D, Modifiers::COMMAND, 20));
        assert_eq!(editor.selections.len(), 2);
        assert!(editor.key(&mut text, Key::L, Modifiers::COMMAND | Modifiers::SHIFT, 20));
        assert_eq!(editor.selections.len(), 3);
        editor.replace(&mut text, "水");
        assert_eq!(text, "name: 水\nother: 水\nlast: 水");
    }
    #[test]
    fn typing_groups_undo_but_paste_remains_a_separate_edit() {
        let mut editor = Editor::default();
        let mut text = String::new();
        editor.type_text(&mut text, "c", 1.0);
        editor.type_text(&mut text, "a", 1.1);
        editor.type_text(&mut text, "fé", 1.2);
        editor.typing_cursors.clear();
        editor.replace(&mut text, " 水");
        editor.history(&mut text, false);
        assert_eq!(text, "café");
        editor.history(&mut text, false);
        assert_eq!(text, "");
        editor.history(&mut text, true);
        assert_eq!(text, "café");
    }
    #[test]
    fn vertical_navigation_skips_hidden_folded_lines() {
        let mut text = "metadata:\n  labels:\n    app: web\nspec: {}".to_owned();
        let mut editor = Editor::default();
        editor.folds.insert(0);
        editor.key(&mut text, Key::ArrowDown, Modifiers::NONE, 20);
        assert_eq!(editor.selections[0].head, text.find("spec:").unwrap());
        assert!(editor.folds.contains(&0));
    }
}
