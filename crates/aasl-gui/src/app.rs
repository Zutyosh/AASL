//! Root: a full-bleed hero with the launcher on top, the first-run wizard,
//! or the settings view.

use freya::prelude::*;

use crate::state::{Screen, use_ui};
use crate::theme::{T, aasl_theme, size, space};
use crate::{launcher, settings, widgets, wizard};

pub const HERO: &[u8] = include_bytes!("../assets/hero.jpg");
pub const LOGO: &[u8] = include_bytes!("../assets/logo.png");

pub fn app() -> impl IntoElement {
    use_init_theme(aasl_theme);
    let ui = use_ui();
    let screen = *ui.screen.read();
    let body: Element = match screen {
        Screen::Launcher => launcher::view(ui).into_element(),
        Screen::Wizard => wizard::view(ui).into_element(),
        Screen::Settings => settings::view(ui).into_element(),
    };
    rect()
        .expanded()
        .background(T.bg)
        .color(T.text)
        .font_size(size::BODY)
        .maybe_child((screen != Screen::Settings).then(hero))
        .child(body)
        .maybe_child(ui.error.read().clone().map(move |msg| error_toast(ui, msg)))
}

/// The background picture, cover-scaled, with a gradient so text reads.
fn hero() -> impl IntoElement {
    rect()
        .position(Position::new_absolute().top(0.).left(0.))
        .width(Size::fill())
        .height(Size::fill())
        .child(
            ImageViewer::new(HERO)
                .width(Size::fill())
                .height(Size::fill())
                .aspect_ratio(AspectRatio::Max)
                .image_cover(ImageCover::Center),
        )
        .child(
            rect()
                .position(Position::new_absolute().top(0.).left(0.))
                .width(Size::fill())
                .height(Size::fill())
                .background(
                    LinearGradient::new()
                        .angle(180.)
                        .stop((Color::from_af32rgb(0.0, 0x12, 0x14, 0x17), 0.35))
                        .stop((Color::from_af32rgb(0.92, 0x12, 0x14, 0x17), 1.0)),
                ),
        )
}

pub fn brand() -> impl IntoElement {
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(space::M)
        .child(
            ImageViewer::new(LOGO)
                .width(Size::px(40.))
                .height(Size::px(40.)),
        )
        .child(
            rect()
                .child(
                    label()
                        .text("AASL")
                        .font_size(size::LEAD)
                        .font_weight(FontWeight::BOLD)
                        .color(T.text),
                )
                .child(widgets::muted(crate::i18n::t("app.tagline"))),
        )
}

fn error_toast(mut ui: crate::state::Ui, msg: String) -> impl IntoElement {
    rect()
        .position(Position::new_absolute().top(space::L).right(space::L))
        .max_width(Size::px(460.))
        .child(
            widgets::glass_panel()
                .border(Border::new().fill(T.error).width(1.))
                .spacing(space::S)
                .child(label().text(msg).color(T.error))
                .child(
                    Button::new()
                        .flat()
                        .on_press(move |_| ui.error.set(None))
                        .child("OK"),
                ),
        )
}
