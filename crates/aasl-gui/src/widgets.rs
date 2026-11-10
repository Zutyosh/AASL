//! Building blocks bound to the tokens.

use freya::prelude::*;

use crate::theme::{RADIUS_PANEL, T, glass, size, space};

pub fn title(text: impl Into<String>) -> Label {
    label()
        .text(text.into())
        .font_size(size::TITLE)
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

/// Frosted panel over the hero.
pub fn glass_panel() -> Rect {
    rect()
        .background(glass())
        .backdrop_blur(24.)
        .corner_radius(RADIUS_PANEL)
        .border(
            Border::new()
                .fill(Color::from_af32rgb(0.08, 255, 255, 255))
                .width(1.),
        )
        .padding(space::L)
}

/// A group of preference rows, libadwaita-style.
pub fn group(heading_text: &str, rows: Vec<Element>) -> Rect {
    rect()
        .width(Size::fill())
        .spacing(space::S)
        .child(
            label()
                .text(heading_text.to_string())
                .font_size(size::SMALL)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(T.muted),
        )
        .child(
            rect()
                .width(Size::fill())
                .background(T.surface)
                .corner_radius(RADIUS_PANEL)
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

/// Small rounded label with a coloured edge, for statuses.
pub fn chip(text: impl Into<String>, tone: Tone) -> Rect {
    let (r, g, b) = tone.color();
    rect()
        .padding((3., 10.))
        .corner_radius(999.)
        .background(Color::from_af32rgb(0.16, r, g, b))
        .border(
            Border::new()
                .fill(Color::from_af32rgb(0.5, r, g, b))
                .width(1.),
        )
        .child(
            label()
                .text(text.into())
                .font_size(size::SMALL)
                .color(tone.color()),
        )
}

/// Status line inside the bottom bar: a label above a value.
pub fn stat(name: &str, value: impl Into<String>, tone: Tone) -> Rect {
    rect().spacing(2.).child(muted(name.to_string())).child(
        label()
            .text(value.into())
            .font_size(size::BODY)
            .font_weight(FontWeight::MEDIUM)
            .color(tone.color()),
    )
}

pub fn severity_tone(s: aasl_core::Severity) -> Tone {
    match s {
        aasl_core::Severity::Error => Tone::Error,
        aasl_core::Severity::Warn => Tone::Warn,
        aasl_core::Severity::Info => Tone::Neutral,
    }
}
