//! Design tokens (DESIGN.md). Components never use a literal colour.

use freya::prelude::*;

pub type Rgb = (u8, u8, u8);

pub struct Tokens {
    pub bg: Rgb,
    pub surface: Rgb,
    pub raised: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    pub accent_text: Rgb,
    pub ok: Rgb,
    pub warn: Rgb,
    pub error: Rgb,
}

pub const T: &Tokens = &Tokens {
    bg: (0x12, 0x14, 0x17),
    surface: (0x1b, 0x1e, 0x23),
    raised: (0x24, 0x28, 0x30),
    border: (0x2e, 0x33, 0x3c),
    text: (0xec, 0xea, 0xe5),
    muted: (0xa0, 0xa3, 0xa9),
    accent: (0xe8, 0xa3, 0x3d),
    accent_text: (0x16, 0x12, 0x0a),
    ok: (0x6c, 0xc2, 0x8a),
    warn: (0xe8, 0xc3, 0x3d),
    error: (0xe5, 0x65, 0x5b),
};

/// Glass panels over the hero image.
pub fn glass() -> Color {
    Color::from_af32rgb(0.72, 0x14, 0x16, 0x1b)
}

pub mod size {
    pub const SMALL: f32 = 13.;
    pub const BODY: f32 = 15.;
    pub const LEAD: f32 = 18.;
    pub const TITLE: f32 = 24.;
    pub const DISPLAY: f32 = 44.;
}

pub mod space {
    pub const XS: f32 = 4.;
    pub const S: f32 = 8.;
    pub const M: f32 = 12.;
    pub const L: f32 = 16.;
    pub const XL: f32 = 24.;
    pub const XXL: f32 = 32.;
}

pub const RADIUS_PANEL: f32 = 14.;

fn c(rgb: Rgb) -> Color {
    Color::from_rgb(rgb.0, rgb.1, rgb.2)
}

/// Freya's dark theme with AASL's accent and surfaces, so built-in
/// components (buttons, inputs, switches) match.
pub fn aasl_theme() -> Theme {
    let mut t = dark_theme();
    let k = &mut t.colors;
    k.primary = c(T.accent);
    k.secondary = Color::from_rgb(0xf3, 0xd0, 0x9b);
    k.tertiary = Color::from_rgb(0xb3, 0x7a, 0x26);
    k.success = c(T.ok);
    k.warning = c(T.warn);
    k.error = c(T.error);
    k.background = c(T.bg);
    k.surface_primary = c(T.raised);
    k.surface_secondary = c(T.surface);
    k.surface_tertiary = c(T.bg);
    k.border = c(T.border);
    k.border_focus = c(T.accent);
    k.text_primary = c(T.text);
    k.text_secondary = c(T.muted);
    k.text_inverse = c(T.accent_text);
    k.focus = c(T.accent);
    t
}
