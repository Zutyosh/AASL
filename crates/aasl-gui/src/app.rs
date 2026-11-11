//! Root: the key art behind everything, then the launcher, the first-run
//! wizard or the settings.

use freya::prelude::*;

use crate::state::{Screen, use_ui};
use crate::theme::{RADIUS_PANEL, T, aasl_theme, alpha, size, space};
use crate::{launcher, settings, widgets, wizard};

pub const LOGO: &[u8] = include_bytes!("../assets/logo.png");

pub fn app() -> impl IntoElement {
    use_init_theme(aasl_theme);
    let ui = use_ui();
    let screen = *ui.screen.read();
    let body: Element = match screen {
        Screen::Launcher => launcher::LauncherScreen(ui).into_element(),
        Screen::Wizard => wizard::WizardScreen(ui).into_element(),
        Screen::Settings => settings::SettingsScreen(ui).into_element(),
    };
    let lang = *ui.lang.read() as u8;
    let body: Element = rect().expanded().key(lang).child(body).into_element();
    let bg = ui.background.read().clone();
    rect()
        .expanded()
        .background(T.bg)
        .color(T.text)
        .font_size(size::BODY)
        .maybe_child((screen != Screen::Settings).then(|| backdrop(bg, screen == Screen::Wizard)))
        .child(body)
        .maybe_child(crate::state::building(ui).then_some(crate::build_screen::BuildScreen(ui)))
        .maybe_child(ui.error.read().clone().map(move |msg| error_toast(ui, msg)))
}

fn layer() -> Rect {
    rect()
        .position(Position::new_absolute().top(0.).left(0.))
        .width(Size::fill())
        .height(Size::fill())
}

/// Somnium's key art, cover-scaled, darkened towards the bottom so the
/// bar reads. The wizard dims it further so the text sits on calm ground.
fn backdrop(bg: Option<std::path::PathBuf>, dim: bool) -> impl IntoElement {
    // Images paint above sibling rects, so the dimming is the picture's own
    // opacity over the background colour rather than an overlay.
    layer().maybe_child(bg.map(|p| {
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .opacity(if dim { 0.3 } else { 1.0 })
            .child(
                ImageViewer::new(p)
                    .width(Size::fill())
                    .height(Size::fill())
                    .aspect_ratio(AspectRatio::Max)
                    .image_cover(ImageCover::Center),
            )
    }))
}

pub fn logo(px: f32) -> impl IntoElement {
    ImageViewer::new(LOGO)
        .width(Size::px(px))
        .height(Size::px(px))
}

pub fn brand() -> impl IntoElement {
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(space::M)
        .child(logo(32.))
        .child(
            label()
                .text("AASL")
                .font_size(size::LEAD)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(T.text),
        )
}

fn error_toast(mut ui: crate::state::Ui, msg: String) -> impl IntoElement {
    rect()
        .position(Position::new_absolute().top(space::L).left(0.))
        .width(Size::fill())
        .center()
        .child(
            rect()
                .max_width(Size::px(560.))
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(space::M)
                .padding((space::M, space::L))
                .corner_radius(RADIUS_PANEL)
                .background(T.raised)
                .border(Border::new().fill(alpha(T.error, 0.6)).width(1.))
                .child(widgets::dot(widgets::Tone::Error))
                .child(
                    rect()
                        .max_width(Size::px(440.))
                        .child(label().text(msg).color(T.text)),
                )
                .child(
                    Button::new()
                        .flat()
                        .compact()
                        .on_press(move |_| ui.error.set(None))
                        .child(crate::i18n::t("dismiss")),
                ),
        )
}
