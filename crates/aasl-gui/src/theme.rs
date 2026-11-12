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
    pub accent_hover: Rgb,
    pub accent_text: Rgb,
    /// Second loader colour, the pink of Somnium's key art.
    pub glow: Rgb,
    pub ok: Rgb,
    pub warn: Rgb,
    pub error: Rgb,
}

pub const T: &Tokens = &Tokens {
    bg: (0x11, 0x11, 0x13),
    surface: (0x19, 0x19, 0x1c),
    raised: (0x24, 0x24, 0x28),
    border: (0x30, 0x30, 0x35),
    text: (0xec, 0xec, 0xef),
    muted: (0x9b, 0x9b, 0xa4),
    accent: (0xb6, 0x9a, 0xff),
    accent_hover: (0xc9, 0xb6, 0xff),
    accent_text: (0x16, 0x10, 0x2a),
    glow: (0xff, 0x8f, 0xd6),
    ok: (0x7c, 0xd3, 0x9a),
    warn: (0xf0, 0xc6, 0x5a),
    error: (0xf2, 0x7f, 0x7f),
};

/// Frosted panels over the key art.
pub fn glass() -> Color {
    alpha(T.bg, 0.62)
}

/// A token at some opacity.
pub fn alpha(rgb: Rgb, a: f32) -> Color {
    Color::from_af32rgb(a, rgb.0, rgb.1, rgb.2)
}

pub mod size {
    pub const SMALL: f32 = 13.;
    pub const BODY: f32 = 15.;
    pub const LEAD: f32 = 18.;
    pub const TITLE: f32 = 26.;
    pub const DISPLAY: f32 = 36.;
}

pub mod space {
    pub const XS: f32 = 4.;
    pub const S: f32 = 8.;
    pub const M: f32 = 12.;
    pub const L: f32 = 16.;
    pub const XL: f32 = 24.;
    pub const XXL: f32 = 32.;
    pub const XXXL: f32 = 48.;
}

pub const RADIUS_CONTROL: f32 = 10.;
pub const RADIUS_PANEL: f32 = 16.;
pub const RADIUS_PILL: f32 = 999.;

fn c(rgb: Rgb) -> Color {
    Color::from_rgb(rgb.0, rgb.1, rgb.2)
}

/// Freya's dark theme with AASL's palette and rounder controls, so the
/// built-in components (buttons, inputs, switches) match.
pub fn aasl_theme() -> Theme {
    let mut t = dark_theme();
    let k = &mut t.colors;
    k.primary = c(T.accent);
    k.secondary = c(T.text);
    k.tertiary = c(T.accent_hover);
    k.success = c(T.ok);
    k.warning = c(T.warn);
    k.error = c(T.error);
    k.background = c(T.bg);
    k.surface_primary = c(T.raised);
    k.surface_secondary = c(T.border);
    k.surface_tertiary = c(T.surface);
    k.border = c(T.border);
    k.border_focus = c(T.accent);
    k.text_primary = c(T.text);
    k.text_secondary = c(T.muted);
    k.text_inverse = c(T.accent_text);
    k.focus = c(T.accent);
    // Primary actions are off-white on graphite: the violet stays for the
    // key art, the selection, toggles and progress, so it never competes.
    t.set(
        "filled_button",
        ButtonColorsThemePreference {
            background: Preference::Specific(c(T.text)),
            hover_background: Preference::Specific(Color::WHITE),
            border_fill: Preference::Specific(Color::TRANSPARENT),
            focus_border_fill: Preference::Specific(c(T.accent)),
            color: Preference::Specific(c(T.bg)),
        },
    );
    t.set(
        "switch",
        SwitchColorsThemePreference {
            background: Preference::Specific(c(T.border)),
            thumb_background: Preference::Specific(c(T.muted)),
            toggled_background: Preference::Specific(c(T.accent)),
            toggled_thumb_background: Preference::Specific(c(T.accent_text)),
            focus_border_fill: Preference::Specific(c(T.accent)),
        },
    );
    t.set(
        "progressbar",
        ProgressBarThemePreference {
            color: Preference::Specific(c(T.text)),
            background: Preference::Specific(alpha(T.text, 0.12)),
            progress_background: Preference::Specific(c(T.accent)),
            height: Preference::Specific(6.),
        },
    );
    for (key, pad) in [
        ("button_layout", Gaps::new(8., 16., 8., 16.)),
        ("compact_button_layout", Gaps::new(4., 10., 4., 10.)),
        ("expanded_button_layout", Gaps::new(12., 20., 12., 20.)),
    ] {
        t.set(
            key,
            ButtonLayoutThemePreference {
                padding: Preference::Specific(pad),
                margin: Preference::Specific(Gaps::new_all(0.)),
                corner_radius: Preference::Specific(CornerRadius::new_all(RADIUS_CONTROL)),
                width: Preference::Specific(Size::Inner),
                height: Preference::Specific(Size::Inner),
            },
        );
    }
    t
}
