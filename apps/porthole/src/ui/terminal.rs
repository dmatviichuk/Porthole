use super::{Action, App, style::*};
use eframe::egui::*;
impl App {
    pub(super) fn dock(&mut self, ui: &mut Ui, r: Rect) {
        let t = self.t;
        line(ui, r.left_top(), r.right_top(), t.line);
        let resize = ui
            .interact(
                rect(r.left(), r.top() - 4.0, r.width(), 8.0),
                ui.id().with("dock-resize"),
                Sense::drag(),
            )
            .on_hover_cursor(CursorIcon::ResizeVertical);
        if resize.dragged() {
            self.prefs.dock_height = (self.prefs.dock_height - ui.input(|i| i.pointer.delta().y))
                .clamp(140.0, ui.ctx().content_rect().height() * 0.8);
        }
        let tabs = rect(r.left(), r.top() + 1.0, r.width(), 31.0);
        ui.painter().rect_filled(tabs, 0, t.sidebar);
        line(ui, tabs.left_bottom(), tabs.right_bottom(), t.line);
        let mut x = tabs.left();
        for s in &self.shells {
            let title = format!("{} · {}", s.pod, s.container);
            let width = (text_width(ui, &title, 12.0, "sans") + 54.0).min(260.0);
            let rr = rect(x, tabs.top(), width, tabs.height());
            if self.active_shell == Some(s.key) {
                ui.painter().rect_filled(rr, 0, t.bg);
            }
            self.icons.paint(
                ui,
                "square-terminal",
                rect(x + 12.0, rr.center().y - 6.5, 13.0, 13.0),
                if self.active_shell == Some(s.key) {
                    t.text
                } else {
                    t.muted
                },
            );
            label(
                ui,
                rect(x + 31.0, rr.top(), width - 52.0, rr.height()),
                &title,
                12.0,
                "sans",
                if self.active_shell == Some(s.key) {
                    t.text
                } else {
                    t.muted
                },
            );
            let response = ui.interact(
                rect(x, rr.top(), width - 20.0, rr.height()),
                ui.id().with(("shell-tab", s.key)),
                Sense::click(),
            );
            response
                .clone()
                .on_hover_text(format!("{} / {} / {} / {}", s.context, s.namespace, s.pod, s.container));
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &title));
            if response.clicked() {
                self.actions.push(Action::ActiveShell(s.key));
            }
            let close = rect(rr.right() - 18.0, rr.center().y - 8.0, 16.0, 16.0);
            self.icons.paint(ui, "x", close.shrink(2.0), t.muted);
            if ui
                .interact(close, ui.id().with(("close-shell", s.key)), Sense::click())
                .clicked()
            {
                self.actions.push(Action::CloseShell(s.key));
            }
            line(ui, rr.right_top(), rr.right_bottom(), t.line);
            x += width;
        }
        let body = rect(r.left(), r.top() + 32.0, r.width(), r.height() - 32.0);
        let Some(s) = self.shells.iter_mut().find(|s| Some(s.key) == self.active_shell) else {
            return;
        };
        let cw = text_width(ui, "0000000000", 12.5, "mono") / 10.0;
        let ch = 17.0;
        let cols = ((body.width() - 24.0) / cw).floor().max(1.0) as u16;
        let rows = ((body.height() - 8.0) / ch).floor().max(1.0) as u16;
        if cols != s.cols || rows != s.rows {
            s.cols = cols;
            s.rows = rows;
            s.parser.screen_mut().set_size(rows, cols);
            let _ = self.data.resize(s.session, cols, rows);
        }
        let response = ui.interact(body, Id::new(("terminal", s.key)), Sense::click_and_drag());
        if response.clicked() {
            self.shell_focus = true;
            response.request_focus();
        }
        if !self.shell_focus && response.has_focus() {
            response.surrender_focus();
        }
        self.shell_focus &= response.has_focus();
        let origin = body.left_top() + vec2(12.0, 8.0);
        let cell_at = |p: Pos2| {
            (
                ((p.y - origin.y) / ch).floor().clamp(0.0, (rows - 1) as f32) as u16,
                ((p.x - origin.x) / cw).floor().clamp(0.0, (cols - 1) as f32) as u16,
            )
        };
        if response.drag_started()
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
        {
            let cell = cell_at(p);
            s.selection = Some((cell, cell));
        }
        if response.dragged()
            && let Some(p) = ui.input(|i| i.pointer.hover_pos())
            && let Some((start, _)) = s.selection
        {
            s.selection = Some((start, cell_at(p)));
        }
        if response.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta.y);
            if delta != 0.0 {
                let current = s.parser.screen().scrollback() as isize;
                s.parser
                    .screen_mut()
                    .set_scrollback((current + (delta / ch).round() as isize).max(0) as usize);
            }
        }
        if self.shell_focus {
            let events = ui.input(|i| i.events.clone());
            for event in events {
                match event {
                    Event::Copy => {
                        if let Some((a, b)) = s.selection {
                            let (a, b) = if a <= b { (a, b) } else { (b, a) };
                            ui.ctx().copy_text(s.parser.screen().contents_between(
                                a.0,
                                a.1,
                                b.0,
                                b.1.saturating_add(1),
                            ));
                        }
                    }
                    Event::Text(text) if !s.ended => {
                        let _ = self.data.shell_input(s.session, text.into_bytes());
                        s.parser.screen_mut().set_scrollback(0);
                    }
                    Event::Paste(text) if !s.ended => {
                        let text = if s.parser.screen().bracketed_paste() {
                            format!("\x1b[200~{text}\x1b[201~")
                        } else {
                            text
                        };
                        let _ = self.data.shell_input(s.session, text.into_bytes());
                    }
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if modifiers.command && key == Key::C {
                            if let Some((a, b)) = s.selection {
                                let (a, b) = if a <= b { (a, b) } else { (b, a) };
                                ui.ctx().copy_text(s.parser.screen().contents_between(
                                    a.0,
                                    a.1,
                                    b.0,
                                    b.1.saturating_add(1),
                                ));
                            }
                            continue;
                        }
                        if modifiers.ctrl && key == Key::Backtick {
                            continue;
                        }
                        if let Some(bytes) = key_bytes(key, modifiers, s.parser.screen().application_cursor())
                            && !s.ended
                        {
                            let _ = self.data.shell_input(s.session, bytes);
                            s.parser.screen_mut().set_scrollback(0);
                        }
                    }
                    _ => {}
                }
            }
        }
        let screen = s.parser.screen();
        let painter = ui.painter().with_clip_rect(body);
        for row in 0..rows {
            for col in 0..cols {
                let Some(cell) = screen.cell(row, col) else {
                    continue;
                };
                let cr = rect(origin.x + col as f32 * cw, origin.y + row as f32 * ch, cw, ch);
                let mut fg = ansi(cell.fgcolor(), t.text);
                let mut bg = ansi(cell.bgcolor(), t.bg);
                if cell.inverse() {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if bg != t.bg {
                    painter.rect_filled(cr, 0, bg);
                }
                if let Some((a, b)) = s.selection {
                    let (a, b) = if a <= b { (a, b) } else { (b, a) };
                    if (row, col) >= a && (row, col) <= b {
                        painter.rect_filled(cr, 0, t.accent.gamma_multiply(0.33));
                    }
                }
                if cell.has_contents() {
                    painter.text(
                        cr.left_top() + vec2(0.0, 1.0),
                        Align2::LEFT_TOP,
                        cell.contents(),
                        font(12.5, if cell.bold() { "mono-bold" } else { "mono" }),
                        fg,
                    );
                    if cell.underline() {
                        painter.line_segment([cr.left_bottom(), cr.right_bottom()], Stroke::new(1.0, fg));
                    }
                }
            }
        }
        if !s.ended
            && !screen.hide_cursor()
            && screen.scrollback() == 0
            && (!self.shell_focus || ui.input(|i| (i.time * 2.0) as i64 % 2 == 0))
        {
            let (row, col) = screen.cursor_position();
            let cr = rect(origin.x + col as f32 * cw, origin.y + row as f32 * ch, cw, ch);
            if self.shell_focus {
                painter.rect_filled(cr, 0, t.text);
            } else {
                painter.rect_stroke(cr, 0, Stroke::new(1.0, t.text), StrokeKind::Inside);
            }
            if self.shell_focus
                && let Some(cell) = screen.cell(row, col)
            {
                painter.text(
                    cr.left_top() + vec2(0.0, 1.0),
                    Align2::LEFT_TOP,
                    cell.contents(),
                    font(12.5, if cell.bold() { "mono-bold" } else { "mono" }),
                    t.bg,
                );
            }
        }
        if self.shell_focus {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}
fn key_bytes(key: Key, m: Modifiers, application: bool) -> Option<Vec<u8>> {
    if m.ctrl && !m.mac_cmd {
        let letter = match key {
            Key::A => 'a',
            Key::B => 'b',
            Key::C => 'c',
            Key::D => 'd',
            Key::E => 'e',
            Key::F => 'f',
            Key::G => 'g',
            Key::H => 'h',
            Key::I => 'i',
            Key::J => 'j',
            Key::K => 'k',
            Key::L => 'l',
            Key::M => 'm',
            Key::N => 'n',
            Key::O => 'o',
            Key::P => 'p',
            Key::Q => 'q',
            Key::R => 'r',
            Key::S => 's',
            Key::T => 't',
            Key::U => 'u',
            Key::V => 'v',
            Key::W => 'w',
            Key::X => 'x',
            Key::Y => 'y',
            Key::Z => 'z',
            Key::OpenBracket => '[',
            Key::Backslash => '\\',
            Key::CloseBracket => ']',
            _ => '\0',
        };
        if letter != '\0' {
            return Some(vec![(letter as u8) & 0x1f]);
        }
    }
    let modifier = 1 + usize::from(m.shift) + 2 * usize::from(m.alt) + 4 * usize::from(m.ctrl);
    let cursor = |c: char| {
        if modifier > 1 {
            format!("\x1b[1;{modifier}{c}")
        } else if application {
            format!("\x1bO{c}")
        } else {
            format!("\x1b[{c}")
        }
    };
    let text = match key {
        Key::Enter => "\r".into(),
        Key::Backspace => {
            if m.alt {
                "\x1b\x7f".into()
            } else {
                "\x7f".into()
            }
        }
        Key::Tab => {
            if m.shift {
                "\x1b[Z".into()
            } else {
                "\t".into()
            }
        }
        Key::Escape => "\x1b".into(),
        Key::ArrowUp => cursor('A'),
        Key::ArrowDown => cursor('B'),
        Key::ArrowRight => cursor('C'),
        Key::ArrowLeft => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::Insert => "\x1b[2~".into(),
        Key::Delete => "\x1b[3~".into(),
        Key::PageUp => "\x1b[5~".into(),
        Key::PageDown => "\x1b[6~".into(),
        Key::F1 => "\x1bOP".into(),
        Key::F2 => "\x1bOQ".into(),
        Key::F3 => "\x1bOR".into(),
        Key::F4 => "\x1bOS".into(),
        Key::F5 => "\x1b[15~".into(),
        Key::F6 => "\x1b[17~".into(),
        Key::F7 => "\x1b[18~".into(),
        Key::F8 => "\x1b[19~".into(),
        Key::F9 => "\x1b[20~".into(),
        Key::F10 => "\x1b[21~".into(),
        Key::F11 => "\x1b[23~".into(),
        Key::F12 => "\x1b[24~".into(),
        _ => return None,
    };
    Some(text.into_bytes())
}
fn ansi(c: vt100::Color, default: Color32) -> Color32 {
    match c {
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        vt100::Color::Idx(i) => {
            let basic = [
                "#2e3436", "#cc0000", "#4e9a06", "#c4a000", "#3465a4", "#75507b", "#06989a", "#d3d7cf", "#555753",
                "#ef2929", "#8ae234", "#fce94f", "#729fcf", "#ad7fa8", "#34e2e2", "#eeeeec",
            ];
            if i < 16 {
                hex(basic[i as usize])
            } else if i >= 232 {
                let v = 8 + (i - 232) * 10;
                Color32::from_rgb(v, v, v)
            } else {
                let i = i - 16;
                let scale = |x: u8| if x == 0 { 0 } else { 55 + 40 * x };
                Color32::from_rgb(scale(i / 36), scale(i % 36 / 6), scale(i % 6))
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_key_sequences() {
        assert_eq!(key_bytes(Key::ArrowUp, Modifiers::NONE, false).unwrap(), b"\x1b[A");
        assert_eq!(key_bytes(Key::C, Modifiers::CTRL, false).unwrap(), vec![3]);
    }
}
