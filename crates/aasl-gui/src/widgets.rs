//! Building blocks bound to the tokens.

use freya::prelude::*;

use crate::theme::{RADIUS_CONTROL, RADIUS_PANEL, RADIUS_PILL, T, alpha, glass, size, space};

pub fn title(text: impl Into<String>) -> Label {
    label()
        .text(text.into())
        .font_size(size::TITLE)
        .font_weight(FontWeight::SEMI_BOLD)
        .color(T.text)
}

pub fn display(text: impl Into<String>) -> Label {
    label()
        .text(text.into())
        .font_size(size::DISPLAY)
        .font_weight(FontWeight::SEMI_BOLD)
        .color(T.text)
}

pub fn body(text: impl Into<String>) -> Label {
    label()
        .text(text.into())
        .font_size(size::BODY)
        .color(T.text)
}

pub fn muted(text: impl Into<String>) -> Label {
    label()
        .text(text.into())
        .font_size(size::SMALL)
        .color(T.muted)
}

/// Frosted panel over the key art.
pub fn glass_panel() -> Rect {
    rect()
        .background(glass())
        .backdrop_blur(30.)
        .corner_radius(RADIUS_PANEL)
        .border(Border::new().fill(alpha(T.text, 0.08)).width(1.))
        .padding(space::L)
}

/// The main call to action: a filled pill.
pub fn pill(text: impl Into<String>) -> Button {
    Button::new().filled().child(
        label()
            .text(text.into())
            .font_size(size::LEAD)
            .font_weight(FontWeight::SEMI_BOLD)
            .color(T.accent_text),
    )
}

/// A quiet text button for secondary paths.
pub fn link(text: impl Into<String>) -> Button {
    Button::new().flat().compact().child(
        label()
            .text(text.into())
            .font_size(size::SMALL)
            .color(T.muted),
    )
}

/// Sliders icon for Settings.
pub fn settings_icon() -> impl IntoElement {
    const SVG: &str = r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="white" stroke-width="2" stroke-linecap="round"><path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1"/><circle cx="15" cy="6" r="2"/><circle cx="9" cy="12" r="2"/><circle cx="17" cy="18" r="2"/></svg>"#;
    SvgViewer::new(SVG.as_bytes())
        .stroke(T.text)
        .width(Size::px(22.))
        .height(Size::px(22.))
}

/// A group of preference rows, libadwaita-style.
pub fn group(heading_text: &str, rows: Vec<Element>) -> Rect {
    rect()
        .width(Size::fill())
        .spacing(space::S)
        .child(
            label()
                .text(heading_text.to_string())
                .font_size(size::BODY)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(T.text),
        )
        .child(
            rect()
                .width(Size::fill())
                .background(T.surface)
                .corner_radius(RADIUS_CONTROL + 2.)
                .border(Border::new().fill(T.border).width(1.))
                .children(rows),
        )
}

/// One row: title, optional subtitle, and a trailing control.
pub fn row(
    title_text: impl Into<String>,
    subtitle: Option<String>,
    trailing: Option<Element>,
) -> Element {
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .padding((space::M, space::L))
        .spacing(space::M)
        .child(
            rect()
                .width(Size::flex(1.))
                .spacing(2.)
                .child(body(title_text))
                .maybe_child(subtitle.map(muted)),
        )
        .maybe_child(trailing)
        .into_element()
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Ok,
    Warn,
    Error,
    Neutral,
    Accent,
}

impl Tone {
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Tone::Ok => T.ok,
            Tone::Warn => T.warn,
            Tone::Error => T.error,
            Tone::Neutral => T.muted,
            Tone::Accent => T.accent,
        }
    }
}

/// Small rounded status label.
pub fn chip(text: impl Into<String>, tone: Tone) -> Rect {
    rect()
        .padding((3., 10.))
        .corner_radius(RADIUS_PILL)
        .background(alpha(tone.color(), 0.14))
        .child(
            label()
                .text(text.into())
                .font_size(size::SMALL)
                .color(tone.color()),
        )
}

/// A coloured dot, for compact status.
pub fn dot(tone: Tone) -> Rect {
    rect()
        .width(Size::px(8.))
        .height(Size::px(8.))
        .corner_radius(4.)
        .background(tone.color())
}

/// Status in the bottom bar: a dot, a name and a value.
pub fn stat(name: &str, value: impl Into<String>, tone: Tone) -> Rect {
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(space::S)
        .child(dot(tone))
        .child(
            rect().spacing(1.).child(muted(name.to_string())).child(
                label()
                    .text(value.into())
                    .font_size(size::BODY)
                    .font_weight(FontWeight::MEDIUM)
                    .color(T.text),
            ),
        )
}

pub fn severity_tone(s: aasl_core::Severity) -> Tone {
    match s {
        aasl_core::Severity::Error => Tone::Error,
        aasl_core::Severity::Warn => Tone::Warn,
        aasl_core::Severity::Info => Tone::Neutral,
    }
}

/// Percentage of a progress event, if the total is known.
pub fn percent(done: u64, total: Option<u64>) -> Option<f32> {
    total
        .filter(|t| *t > 0)
        .map(|t| (done as f32 * 100. / t as f32).clamp(0., 100.))
}

/// A verdict strip: a tone, a headline, one sentence and an optional action.
pub fn banner(
    tone: Tone,
    head: impl Into<String>,
    text: impl Into<String>,
    action: Option<Element>,
) -> Element {
    let head_color = if tone == Tone::Neutral {
        T.text
    } else {
        tone.color()
    };
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(space::L)
        .padding(space::L)
        .corner_radius(RADIUS_CONTROL + 2.)
        .background(alpha(tone.color(), 0.10))
        .border(Border::new().fill(alpha(tone.color(), 0.35)).width(1.))
        .child(
            rect()
                .width(Size::flex(1.))
                .spacing(space::XS)
                .child(
                    label()
                        .text(head.into())
                        .font_size(size::LEAD)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(head_color),
                )
                .child(body(text)),
        )
        .maybe_child(action)
        .into_element()
}

/// Rows with a hairline between them.
pub fn divided(rows: Vec<Element>) -> Vec<Element> {
    let mut out = Vec::with_capacity(rows.len() * 2);
    for (i, r) in rows.into_iter().enumerate() {
        if i > 0 {
            out.push(
                rect()
                    .key(format!("sep{i}"))
                    .width(Size::fill())
                    .height(Size::px(1.))
                    .background(T.border)
                    .into_element(),
            );
        }
        out.push(r);
    }
    out
}

/// A command or a path, in a small monospace box.
pub fn mono_box(text: impl Into<String>) -> Rect {
    rect()
        .width(Size::fill())
        .padding((space::S, space::M))
        .corner_radius(6.)
        .background(T.bg)
        .child(
            label()
                .text(text.into())
                .font_size(size::SMALL)
                .font_family("monospace")
                .color(T.text),
        )
}

/// A Proton's tag and the title of its provider.
pub fn proton_name(id: &aasl_components::ProtonId) -> (String, String) {
    let provider = aasl_components::proton::provider(&id.provider)
        .map(|p| p.title.to_string())
        .unwrap_or_else(|| id.provider.clone());
    (id.tag.clone(), provider)
}

/// [`proton_name`] for an id saved as text; an unreadable id is shown as is.
pub fn proton_label(id: &str) -> (String, String) {
    match aasl_components::ProtonId::parse(id) {
        Ok(p) => proton_name(&p),
        Err(_) => (id.to_string(), String::new()),
    }
}
