//! The main screen, after the An Anime Team launchers: the key art, a
//! compact status card bottom left, Settings and Play bottom right.

use freya::prelude::*;
use unilux_service::{LaunchOpts, Status};

use crate::i18n::t;
use crate::state::{Screen, Ui};
use crate::theme::glass;
use crate::theme::{RADIUS_PILL, size, space};
use crate::theme::{T, alpha};
use crate::widgets::{Icon, Tone, chip, glass_panel, muted, percent, pill, settings_icon, stat};

/// A secondary action on the key art: frosted, pill-shaped, with its icon.
fn glass_button(icon: Icon, text: &str) -> Button {
    Button::new()
        .background(glass())
        .hover_background(alpha(T.raised, 0.9))
        .border_fill(alpha(T.text, 0.1))
        .corner_radius(RADIUS_PILL)
        .padding((14., 20.))
        .child(
            rect()
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(space::S)
                .child(icon.view(18., T.text))
                .child(text.to_string()),
        )
}

/// Play: the one bright control on the screen.
fn play_button(text: &str) -> Button {
    Button::new()
        .filled()
        .corner_radius(RADIUS_PILL)
        .padding((16., 40.))
        .child(
            rect()
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(space::M)
                .child(Icon::Play.view(22., T.bg))
                .child(
                    label()
                        .text(text.to_string())
                        .font_size(size::LEAD + 2.)
                        .font_weight(FontWeight::MEDIUM)
                        .color(T.bg),
                ),
        )
}
use crate::{app, svc};

#[derive(PartialEq)]
pub struct LauncherScreen(pub Ui);

impl Component for LauncherScreen {
    fn render(&self) -> impl IntoElement {
        view(self.0)
    }
}

fn view(ui: Ui) -> impl IntoElement {
    let st = ui.status.read().clone();
    rect()
        .expanded()
        .padding(space::XXL)
        .content(Content::Flex)
        .child(
            rect()
                .width(Size::fill())
                .horizontal()
                .cross_align(Alignment::Center)
                .main_align(Alignment::SpaceBetween)
                .child(app::brand())
                .child(
                    rect()
                        .horizontal()
                        .spacing(space::S)
                        .cross_align(Alignment::Center)
                        .maybe_child(crate::events::online_badge(ui))
                        .maybe_child(
                            (!ui.approvals.read().is_empty())
                                .then(|| chip(t("agents.pending"), Tone::Accent)),
                        )
                        .maybe_child(update_badge(ui)),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.))
                .horizontal()
                .main_align(Alignment::End)
                .padding((space::XL, 0., space::L, 0.))
                .child(crate::events::events_panel(ui)),
        )
        .child(notices(ui))
        .child(
            rect()
                .width(Size::fill())
                .horizontal()
                .content(Content::Flex)
                .cross_align(Alignment::End)
                .spacing(space::L)
                .child(status_card(&st))
                .child(rect().width(Size::flex(1.)))
                .child(actions(ui, &st)),
        )
}

fn notices(ui: Ui) -> impl IntoElement {
    let list = ui.notices.read().clone();
    let skip = list.len().saturating_sub(2);
    rect()
        .width(Size::fill())
        .spacing(space::XS)
        .padding((0., 0., space::M, 0.))
        .children(
            list.into_iter()
                .enumerate()
                .skip(skip)
                .map(|(i, (sev, msg))| {
                    label()
                        .key(i)
                        .text(msg)
                        .font_size(size::SMALL)
                        .color(crate::widgets::severity_tone(sev).color())
                        .into_element()
                })
                .collect::<Vec<_>>(),
        )
}

fn vr_line(st: &Status) -> (String, Tone) {
    match (&st.vr.active, st.vr.ready) {
        (Some(r), true) => (
            r.name.clone().unwrap_or_else(|| r.kind.to_string()),
            Tone::Ok,
        ),
        (Some(r), false) => (
            r.name.clone().unwrap_or_else(|| r.kind.to_string()),
            Tone::Warn,
        ),
        (None, _) => (t("stat.not_ready").to_string(), Tone::Warn),
    }
}

fn patch_line(st: &Status) -> (String, Tone) {
    match &st.patches {
        Some(p) if p.bundle.is_some() && p.intact => (t("stat.intact").to_string(), Tone::Ok),
        Some(p) if p.bundle.is_some() => (t("stat.changed").to_string(), Tone::Error),
        _ => (t("stat.none").to_string(), Tone::Error),
    }
}

fn status_card(st: &Status) -> impl IntoElement {
    let (vr, vr_tone) = vr_line(st);
    let (patches, patch_tone) = patch_line(st);
    let proton_tone = if st.proton.is_some() {
        Tone::Ok
    } else {
        Tone::Error
    };
    let proton = st
        .proton
        .clone()
        .map(|p| p.split_once(':').map(|(_, t)| t.to_string()).unwrap_or(p))
        .unwrap_or_else(|| t("stat.none").into());
    glass_panel()
        .horizontal()
        .spacing(space::XXL)
        .padding((space::M, space::XL))
        .child(stat(t("stat.vr"), vr, vr_tone))
        .child(stat(t("stat.proton"), proton, proton_tone))
        .child(stat(t("stat.patches"), patches, patch_tone))
}

fn progress_line(ui: Ui) -> Option<Element> {
    let step = ui.step.read().clone();
    let progress = ui.progress.read().clone();
    if step.is_none() && progress.is_none() {
        return None;
    }
    let pct = progress.as_ref().and_then(|(_, d, tot)| percent(*d, *tot));
    Some(
        rect()
            .width(Size::px(280.))
            .spacing(space::XS)
            .child(muted(
                step.map(|s| crate::i18n::step(&s))
                    .or(progress.map(|p| p.0))
                    .unwrap_or_default(),
            ))
            .child(
                ProgressBar::new(pct.unwrap_or(0.))
                    .show_progress(false)
                    .width(Size::fill()),
            )
            .into_element(),
    )
}

fn actions(mut ui: Ui, st: &Status) -> impl IntoElement {
    let busy = st.operation.is_some();
    let row = rect()
        .horizontal()
        .spacing(space::M)
        .cross_align(Alignment::Center);
    let settings = Button::new()
        .background(glass())
        .border_fill(crate::theme::alpha(crate::theme::T.text, 0.08))
        .on_press(move |_| ui.screen.set(Screen::Settings))
        .corner_radius(RADIUS_PILL)
        .padding(15.)
        .child(settings_icon());
    let row = row.maybe_child(progress_line(ui));
    let play: Element = if !st.installed {
        pill(t("play.install"))
            .on_press(move |_| ui.screen.set(Screen::Wizard))
            .corner_radius(RADIUS_PILL)
            .padding((14., 44.))
            .into_element()
    } else if st.session_pid.is_some() {
        rect()
            .horizontal()
            .spacing(space::S)
            .cross_align(Alignment::Center)
            .child(chip(t("play.running"), Tone::Ok))
            .child(
                Button::new()
                    .corner_radius(RADIUS_PILL)
                    .padding((14., 32.))
                    .on_press(move |_| ui.run(async { svc().stop().await }, |_| {}))
                    .child(t("play.stop")),
            )
            .into_element()
    } else if busy {
        Button::new()
            .corner_radius(RADIUS_PILL)
            .padding((14., 32.))
            .on_press(|_| svc().cancel())
            .child(t("cancel"))
            .into_element()
    } else {
        let needs_repair = st.patches.as_ref().is_none_or(|p| !p.intact);
        rect()
            .horizontal()
            .spacing(space::S)
            .cross_align(Alignment::Center)
            .maybe_child(needs_repair.then(|| {
                glass_button(Icon::Wrench, t("play.repair"))
                    .on_press(move |_| ui.run(async { svc().repair().await }, |_| {}))
            }))
            .child(
                glass_button(Icon::Monitor, t("play.flat")).on_press(move |_| {
                    ui.run(
                        async { svc().launch(LaunchOpts { allow_flat: true }).await },
                        |_| {},
                    )
                }),
            )
            .child(play_button(t("play")).on_press(move |_| {
                ui.run(
                    async { svc().launch(LaunchOpts { allow_flat: false }).await },
                    |_| {},
                )
            }))
            .into_element()
    };
    row.child(play).child(settings)
}

/// Top-bar entry for Unilux's own updates: install, then restart.
fn update_badge(ui: Ui) -> Option<Element> {
    if *ui.restart_ready.read() {
        return Some(
            Button::new()
                .filled()
                .on_press(|_| crate::restart())
                .child(t("update.restart"))
                .into_element(),
        );
    }
    let info = ui.update.read().clone()?;
    let rel = info.available?;
    let text = format!("{} {}", t("update.available"), rel.version);
    let r = rect()
        .horizontal()
        .spacing(space::S)
        .cross_align(Alignment::Center)
        .child(chip(text, Tone::Accent));
    Some(if info.can_apply {
        r.child(
            Button::new()
                .compact()
                .on_press(move |_| ui.update_now())
                .child(t("update.install")),
        )
        .into_element()
    } else {
        r.child(muted(t("update.package"))).into_element()
    })
}
