use crate::{model::Theme, summary::Health};
use eframe::egui::{self, *};
use std::collections::HashMap;
#[derive(Clone, Copy)]
pub struct Tokens {
    pub bg: Color32,
    pub sidebar: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub line: Color32,
    pub strong: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub ok: Color32,
    pub progress: Color32,
    pub failed: Color32,
    pub done: Color32,
    pub ending: Color32,
    pub key: Color32,
    pub string: Color32,
    pub number: Color32,
    pub meta: Color32,
}
pub fn hex(s: &str) -> Color32 {
    Color32::from_rgb(
        u8::from_str_radix(&s[1..3], 16).unwrap(),
        u8::from_str_radix(&s[3..5], 16).unwrap(),
        u8::from_str_radix(&s[5..7], 16).unwrap(),
    )
}
impl Tokens {
    pub fn new(dark: bool) -> Self {
        let v = if dark {
            [
                "#0e0e10", "#151517", "#18181b", "#17171a", "#26262b", "#212125", "#2f2f35", "#ececef", "#8e8e97",
                "#5c5c64", "#3d8bff", "#3cc97c", "#f0b13d", "#ff5d63", "#8e8e97", "#a98bff", "#ff9e7a", "#8fd6a1",
                "#8cb8ff", "#6c6c75",
            ]
        } else {
            [
                "#ffffff", "#f4f4f6", "#f4f4f6", "#f5f5f7", "#e6e6ea", "#ececef", "#d9d9de", "#1d1d1f", "#6e6e76",
                "#a3a3aa", "#0a66e4", "#1d9a52", "#c27c0e", "#e0383e", "#8a8a92", "#8257e6", "#a3361f", "#1d6b3a",
                "#1f5bc2", "#8a8a92",
            ]
        };
        let c = v.map(hex);
        Self {
            bg: c[0],
            sidebar: c[1],
            raised: c[2],
            hover: c[3],
            selected: c[4],
            line: c[5],
            strong: c[6],
            text: c[7],
            muted: c[8],
            faint: c[9],
            accent: c[10],
            ok: c[11],
            progress: c[12],
            failed: c[13],
            done: c[14],
            ending: c[15],
            key: c[16],
            string: c[17],
            number: c[18],
            meta: c[19],
        }
    }
    pub fn health(self, h: Option<Health>) -> Color32 {
        match h {
            Some(Health::Ok) => self.ok,
            Some(Health::Progress) => self.progress,
            Some(Health::Failed) => self.failed,
            Some(Health::Done) => self.done,
            Some(Health::Ending) => self.ending,
            None => self.faint,
        }
    }
    pub fn apply(self, ctx: &Context, theme: Theme) {
        ctx.set_theme(match theme {
            Theme::Light => ThemePreference::Light,
            Theme::Dark => ThemePreference::Dark,
            Theme::Auto => ThemePreference::System,
        });
        ctx.style_mut_of(ctx.theme(), |s| {
            s.override_font_id = Some(font(13.0, "sans"));
            s.spacing.item_spacing = vec2(0.0, 0.0);
            s.spacing.button_padding = vec2(8.0, 4.0);
            s.spacing.interact_size = vec2(20.0, 24.0);
            s.spacing.window_margin = Margin::same(20);
            s.spacing.scroll.bar_width = 6.0;
            s.spacing.scroll.bar_inner_margin = 3.0;
            s.spacing.scroll.bar_outer_margin = 3.0;
            s.spacing.scroll.floating = false;
            s.spacing.scroll.foreground_color = false;
            s.visuals.override_text_color = Some(self.text);
            s.visuals.disabled_alpha = 0.4;
            s.visuals.text_options.color_transfer_function = egui::epaint::FontColorTransferFunction::Off;
            s.visuals.panel_fill = self.bg;
            s.visuals.window_fill = self.bg;
            s.visuals.extreme_bg_color = self.bg;
            s.visuals.faint_bg_color = self.raised;
            s.visuals.window_stroke = Stroke::new(1.0, self.strong);
            s.visuals.window_corner_radius = CornerRadius::same(12);
            s.visuals.selection.bg_fill = self.accent.gamma_multiply(0.3);
            s.visuals.selection.stroke = Stroke::new(1.0, self.accent);
            s.visuals.widgets.inactive.bg_fill = self.strong;
            s.visuals.widgets.inactive.weak_bg_fill = self.raised;
            s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.strong);
            s.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
            s.visuals.widgets.hovered.bg_fill = self.muted;
            s.visuals.widgets.hovered.weak_bg_fill = self.hover;
            s.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, self.text);
            s.visuals.widgets.active.bg_fill = self.muted;
            s.visuals.widgets.active.weak_bg_fill = self.selected;
            s.visuals.widgets.active.fg_stroke = Stroke::new(1.0, self.text);
            s.visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
            s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.muted);
        });
    }
}
pub fn font(size: f32, family: &str) -> FontId {
    FontId::new(size, FontFamily::Name(family.into()))
}
pub fn install_fonts(ctx: &Context) {
    let mut defs = FontDefinitions::default();
    let symbols = if cfg!(target_os = "macos") {
        std::fs::read("/System/Library/Fonts/Apple Symbols.ttf").ok()
    } else {
        None
    };
    if let Some(bytes) = symbols {
        defs.font_data.insert(
            "system-symbols".into(),
            std::sync::Arc::new(FontData::from_owned(bytes)),
        );
    }
    if cfg!(target_os = "macos")
        && let Ok(bytes) = std::fs::read("/System/Library/Fonts/LucidaGrande.ttc")
    {
        defs.font_data.insert(
            "system-ui-symbols".into(),
            std::sync::Arc::new(FontData::from_owned(bytes)),
        );
    }
    for (name, weight, mono) in [
        ("sans", 400.0, false),
        ("medium", 510.0, false),
        ("semibold", 590.0, false),
        ("bold", 700.0, false),
        ("mono", 400.0, true),
        ("mono-bold", 600.0, true),
    ] {
        #[cfg(target_os = "macos")]
        let system = super::fonts::system_font(mono).map(FontData::from_static);
        #[cfg(not(target_os = "macos"))]
        let system: Option<FontData> = None;
        let selected = system.or_else(|| {
            use font_kit::{
                family_name::FamilyName,
                handle::Handle,
                properties::{Properties, Weight},
                source::SystemSource,
            };
            let families = if mono {
                vec![
                    FamilyName::Title("Cascadia Mono".into()),
                    FamilyName::Title("Menlo".into()),
                    FamilyName::Title("Consolas".into()),
                    FamilyName::Monospace,
                ]
            } else {
                vec![
                    FamilyName::Title("Segoe UI Variable".into()),
                    FamilyName::Title("Segoe UI".into()),
                    FamilyName::SansSerif,
                ]
            };
            let h = SystemSource::new()
                .select_best_match(&families, Properties::new().weight(Weight(weight)))
                .ok()?;
            let (mut bytes, index) = match h {
                Handle::Path { path, font_index } => Some((std::fs::read(path).ok()?, font_index)),
                Handle::Memory { bytes, font_index } => Some((bytes.as_ref().clone(), font_index)),
            }?;
            if !mono {
                super::fonts::tabular_digits(&mut bytes, index);
            }
            let mut data = FontData::from_owned(bytes);
            data.index = index;
            Some(data)
        });
        let base = if mono {
            FontFamily::Monospace
        } else {
            FontFamily::Proportional
        };
        let mut fallback = defs.families.get(&base).cloned().unwrap_or_default();
        if defs.font_data.contains_key("system-symbols") {
            fallback.insert(0, "system-symbols".into());
        }
        if defs.font_data.contains_key("system-ui-symbols") {
            fallback.insert(0, "system-ui-symbols".into());
        }
        if let Some(mut data) = selected {
            data.tweak.coords =
                egui::epaint::text::VariationCoords::new([(b"wght", weight), (b"opsz", 17.0), (b"YAXS", 324.3341)]);
            defs.font_data.insert(name.into(), std::sync::Arc::new(data));
            fallback.insert(0, name.into());
        }
        defs.families.insert(FontFamily::Name(name.into()), fallback);
    }
    ctx.set_fonts(defs);
}
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}
pub fn line(ui: &Ui, a: Pos2, b: Pos2, c: Color32) {
    ui.painter().line_segment([a, b], Stroke::new(1.0, c));
}
/// CoreText applies the system font's AAT tracking table. The Rust shaper handles
/// kerning, but not that table, so retain its size-dependent default tracking here.
pub fn tracking(size: f32, family: &str) -> f32 {
    if !cfg!(target_os = "macos") || family.starts_with("mono") {
        return 0.0;
    }
    const TRACKS: [(f32, f32); 21] = [
        (6., 82.),
        (9., 38.),
        (10., 24.),
        (11., 12.),
        (12., 0.),
        (13., -12.),
        (14., -22.),
        (15., -32.),
        (16., -40.),
        (17., -52.),
        (20., -46.),
        (22., -24.),
        (24., 6.),
        (28., 28.),
        (32., 26.),
        (36., 21.),
        (50., 14.),
        (64., 7.),
        (80., 0.),
        (100., 0.),
        (138., 0.),
    ];
    let value = TRACKS
        .windows(2)
        .find(|v| size >= v[0].0 && size <= v[1].0)
        .map(|v| v[0].1 + (v[1].1 - v[0].1) * (size - v[0].0) / (v[1].0 - v[0].0))
        .unwrap_or(0.0);
    value * size / 2048.0
}
pub fn text_format(size: f32, family: &str, color: Color32) -> TextFormat {
    TextFormat {
        font_id: font(size, family),
        color,
        extra_letter_spacing: tracking(size, family),
        coords: if cfg!(target_os = "macos") && !family.starts_with("mono") {
            egui::epaint::text::VariationCoords::new([(b"opsz", size.max(17.0))])
        } else {
            Default::default()
        },
        ..Default::default()
    }
}
pub fn text_job(text: &str, size: f32, family: &str, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    job.append(text, 0.0, text_format(size, family, color));
    job
}
pub fn label(ui: &Ui, r: Rect, text: &str, size: f32, family: &str, color: Color32) {
    let mut job = text_job(text, size, family, color);
    job.wrap.max_width = r.width().max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    let baseline = if cfg!(target_os = "macos") { 0.5 } else { 0.0 };
    ui.painter().galley(
        pos2(r.left(), r.center().y - galley.size().y / 2.0 - baseline),
        galley,
        color,
    );
}
pub fn text_width(ui: &Ui, text: &str, size: f32, family: &str) -> f32 {
    ui.painter()
        .layout_job(text_job(text, size, family, Color32::WHITE))
        .size()
        .x
}

/// CSS normal wrapping considers a hyphen and a space equally valid break points.
/// egui prefers an earlier space even when a later hyphen fits on the same line.
pub fn css_paragraph(ui: &Ui, mut job: egui::text::LayoutJob, width: f32) -> std::sync::Arc<Galley> {
    job.wrap.max_width = f32::INFINITY;
    let measured = ui.painter().layout_job(job.clone());
    let mut breaks = Vec::new();
    let mut byte_offset = 0;
    for row in &measured.rows {
        let glyphs = &row.glyphs;
        let mut start = 0;
        while start < glyphs.len() {
            let left = glyphs[start].pos.x;
            let mut candidate = None;
            let mut end = start;
            while end < glyphs.len() {
                let glyph = &glyphs[end];
                let whitespace = glyph.chr.is_whitespace() && glyph.chr != '\u{a0}';
                let right = if whitespace {
                    glyph.pos.x
                } else {
                    glyph.pos.x + glyph.advance_width
                };
                if right - left > width && candidate.is_some() {
                    break;
                }
                if whitespace || glyph.chr == '-' {
                    candidate = Some(end + 1);
                    if right - left > width {
                        break;
                    }
                }
                end += 1;
            }
            if end == glyphs.len() {
                break;
            }
            let next = candidate.unwrap_or(glyphs.len());
            let offset = byte_offset + glyphs[..next].iter().map(|g| g.chr.len_utf8()).sum::<usize>();
            breaks.push(offset);
            start = next;
        }
        byte_offset += glyphs.iter().map(|g| g.chr.len_utf8()).sum::<usize>() + usize::from(row.ends_with_newline);
    }
    if breaks.is_empty() {
        return measured;
    }
    let mut wrapped = egui::text::LayoutJob::default();
    wrapped.wrap.max_width = f32::INFINITY;
    for section in &job.sections {
        let section_start = section.byte_range.start.0;
        let section_end = section.byte_range.end.0;
        let mut start = section_start;
        for &end in breaks.iter().filter(|&&end| section_start < end && end <= section_end) {
            wrapped.append(job.text[start..end].trim_end_matches(' '), 0.0, section.format.clone());
            wrapped.append("\n", 0.0, section.format.clone());
            start = end;
        }
        wrapped.append(&job.text[start..section_end], 0.0, section.format.clone());
    }
    ui.painter().layout_job(wrapped)
}

#[cfg(all(test, target_os = "macos"))]
mod font_tests {
    use super::*;

    #[test]
    fn system_font_metrics_match_the_original_coretext_ui() {
        let ctx = Context::default();
        install_fonts(&ctx);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            for (text, size, expected) in [
                ("Move focus between the shells and the", 13.0, 235.8789),
                ("↩", 11.5, 11.5),
                ("↑", 11.5, 9.0181),
                ("⌘1", 11.5, 17.8789),
            ] {
                let actual = text_width(ui, text, size, "sans");
                assert!((actual - expected).abs() < 1.0, "{text}: {actual}, expected {expected}");
            }
        });
        output.textures_delta.clear();
    }
}
#[allow(clippy::too_many_arguments)]
pub fn button(
    ui: &mut Ui,
    r: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    size: f32,
    color: Color32,
    bg: Option<Color32>,
    border: Option<Color32>,
    t: Tokens,
) -> Response {
    button_with_font(ui, r, id, text, size, "sans", color, bg, border, t)
}
#[allow(clippy::too_many_arguments)]
pub fn button_with_font(
    ui: &mut Ui,
    r: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    size: f32,
    family: &str,
    color: Color32,
    bg: Option<Color32>,
    border: Option<Color32>,
    t: Tokens,
) -> Response {
    let response = ui.interact(r, ui.id().with(id), Sense::click());
    // egui already applies disabled_alpha to the painter.
    let opacity = 1.0;
    if let Some(bg) = bg {
        ui.painter().rect_filled(r, 6, bg.gamma_multiply(opacity));
    } else if response.hovered() {
        ui.painter().rect_filled(r, 6, t.hover);
    }
    if let Some(c) = border {
        ui.painter()
            .rect_stroke(r, 6, Stroke::new(1.0, c.gamma_multiply(opacity)), StrokeKind::Inside);
    }
    let w = text_width(ui, text, size, family);
    label(
        ui,
        rect(r.center().x - w / 2.0, r.top(), w + 1.0, r.height()),
        text,
        size,
        family,
        color.gamma_multiply(opacity),
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), text));
    response
}
pub fn search(ui: &mut Ui, r: Rect, value: &mut String, placeholder: &str, id: &str, t: Tokens) -> Response {
    ui.painter().rect_filled(r, 6, t.bg);
    let mut input = ui.new_child(UiBuilder::new().max_rect(r));
    input.visuals_mut().weak_text_color = Some(t.faint);
    let mut layouter = |ui: &Ui, value: &dyn TextBuffer, _: f32| {
        ui.painter().layout_job(text_job(value.as_str(), 13.0, "sans", t.text))
    };
    let response = input.put(
        rect(r.left() + 27.0, r.top() + 3.5, r.width() - 34.0, r.height() - 7.0),
        TextEdit::singleline(value)
            .id(Id::new(id))
            .font(font(13.0, "sans"))
            .layouter(&mut layouter)
            .text_color(t.text)
            .vertical_align(Align::Center)
            .hint_text(RichText::new(placeholder).color(t.faint))
            .frame(Frame::NONE)
            .margin(0),
    );
    ui.painter().rect_stroke(
        r,
        6,
        Stroke::new(1.0, if response.has_focus() { t.accent } else { t.strong }),
        StrokeKind::Inside,
    );
    response
}
pub struct Icons {
    textures: HashMap<String, TextureHandle>,
}
impl Icons {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }
    pub fn paint(&mut self, ui: &Ui, name: &str, r: Rect, color: Color32) {
        self.paint_stroke(ui, name, r, color, 2.0);
    }
    pub fn paint_stroke(&mut self, ui: &Ui, name: &str, r: Rect, color: Color32, stroke: f32) {
        let pixels = (r.size() * ui.ctx().pixels_per_point()).round();
        let key = format!("{name}:{stroke}:{}:{}", pixels.x, pixels.y);
        let h = self.textures.entry(key.clone()).or_insert_with(|| {
            let svg = String::from_utf8_lossy(super::icons::svg(name))
                .replace("stroke-width=\"2\"", &format!("stroke-width=\"{stroke}\""));
            let image = egui_extras::image::load_svg_bytes_with_size(
                svg.as_bytes(),
                egui::load::SizeHint::Size {
                    width: pixels.x as u32,
                    height: pixels.y as u32,
                    maintain_aspect_ratio: true,
                },
                &Default::default(),
            )
            .expect("valid original Lucide SVG");
            ui.ctx().load_texture(key, image, TextureOptions::LINEAR)
        });
        ui.painter()
            .image(h.id(), r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), color);
    }
}
pub fn status(ui: &Ui, icons: &mut Icons, r: Rect, health: Option<Health>, text: &str, t: Tokens) {
    if text.is_empty() {
        return;
    }
    let icon = match (health, text) {
        (Some(Health::Ok), "Scheduled") => "clock",
        (Some(Health::Done), "Suspended") => "pause",
        (Some(Health::Ok | Health::Done), _) => "check",
        (Some(Health::Progress), _) => "clock-3",
        (Some(Health::Failed), _) => "x",
        (Some(Health::Ending), _) => "circle-dashed",
        _ => "minus",
    };
    icons.paint_stroke(
        ui,
        icon,
        rect(r.left(), r.center().y - 6.5, 13.0, 13.0),
        t.health(health),
        2.5,
    );
    label(
        ui,
        rect(r.left() + 19.0, r.top(), r.width() - 19.0, r.height()),
        text,
        13.0,
        "sans",
        t.text,
    );
}
pub fn gauge(ui: &Ui, r: Rect, fill: Option<f64>, cpu: bool, text: &str, t: Tokens) {
    let r = r.translate(vec2(0.0, -2.0));
    let g = rect(r.left(), r.center().y - 12.0, 10.0, 24.0);
    ui.painter().rect_filled(g, 3, t.selected);
    let fill = fill.unwrap_or(0.0);
    let height = (fill.clamp(if fill > 0.0 { 6.0 } else { 0.0 }, 100.0) * 0.24) as f32;
    let p = ui.painter().with_clip_rect(g);
    p.rect_filled(
        rect(g.left(), g.bottom() - height, 10.0, height),
        2,
        if fill >= 90.0 {
            t.failed
        } else if cpu {
            t.ok
        } else {
            t.accent
        },
    );
    label(
        ui,
        rect(r.left() + 20.0, r.top(), r.width() - 20.0, r.height()),
        text,
        13.0,
        "sans",
        t.text,
    );
}
pub fn bar(ui: &Ui, r: Rect, value: f64, cpu: Option<bool>, t: Tokens) {
    ui.painter().rect_filled(r, 3, t.selected);
    let fill = value.clamp(if value > 0.0 { 2.0 } else { 0.0 }, 100.0) as f32 / 100.0;
    let color = if value >= 100.0 {
        t.failed
    } else {
        match cpu {
            Some(true) => t.ok,
            Some(false) => t.accent,
            None => t.text.gamma_multiply(0.7),
        }
    };
    ui.painter()
        .rect_filled(rect(r.left(), r.top(), r.width() * fill, r.height()), 3, color);
}
pub const LABEL_COLORS: [(&str, &str); 8] = [
    ("gray", "#8b8d98"),
    ("red", "#e5484d"),
    ("orange", "#f76b15"),
    ("amber", "#d9a01b"),
    ("green", "#30a46c"),
    ("teal", "#12a594"),
    ("blue", "#3d8bff"),
    ("violet", "#8e4ec6"),
];
pub fn label_color(name: &str) -> Color32 {
    hex(LABEL_COLORS
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or(&LABEL_COLORS[0])
        .1)
}
