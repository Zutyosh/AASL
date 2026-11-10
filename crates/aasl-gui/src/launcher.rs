//! The main screen: hero, brand and settings at the top, a status and
//! action bar at the bottom.

use aasl_service::{LaunchOpts, Status};
use freya::prelude::*;

use crate::i18n::t;
use crate::state::{Screen, Ui};
use crate::theme::{T, size, space};
use crate::widgets::{Tone, chip, glass_panel, stat};
use crate::{app, svc};

pub fn view(mut ui: Ui) -> impl IntoElement {
    let st = ui.status.read().clone();
    rect()
        .expanded()
        .padding(space::XL)
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
                        .maybe_child(
                            (!ui.approvals.read().is_empty())
                                .then(|| chip(t("agents.pending"), Tone::Accent)),
                        )
                        .child(
                            Button::new()
                                .flat()
                                .on_press(move |_| ui.screen.set(Screen::Settings))
                                .child(t("nav.settings")),
                        ),
                ),
        )
        .child(rect().width(Size::fill()).height(Size::flex(1.)))
        .child(notices(ui))
        .child(bar(ui, &st))
}

fn notices(ui: Ui) -> impl IntoElement {
    rect()
        .width(Size::fill())
        .spacing(space::XS)
        .padding((0., 0., space::M, 0.))
        .children(
            ui.notices
                .read()
                .iter()
                .enumerate()
                .map(|(i, (sev, msg))| {
                    label()
                        .key(i)
                        .text(msg.clone())
                        .font_size(size::SMALL)
                        .color(crate::widgets::severity_tone(*sev).color())
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
            format!(
                "{} ({})",
                r.name.clone().unwrap_or_else(|| r.kind.to_string()),
                t("stat.not_ready")
            ),
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

fn bar(ui: Ui, st: &Status) -> impl IntoElement {
    let (vr, vr_tone) = vr_line(st);
    let (patches, patch_tone) = patch_line(st);
    let proton = st
        .proton
        .clone()
        .map(|p| p.split_once(':').map(|(_, t)| t.to_string()).unwrap_or(p))
        .unwrap_or_else(|| t("stat.none").into());
    let progress = ui.progress.read().clone();
    let step = ui.step.read().clone();
    glass_panel()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(space::XL)
        .padding((space::L, space::XL))
        .child(stat(t("stat.vr"), vr, vr_tone))
        .child(stat(t("stat.proton"), proton, Tone::Neutral))
        .child(stat(t("stat.patches"), patches, patch_tone))
        .child(
            rect()
                .width(Size::flex(1.))
                .spacing(space::XS)
                .maybe_child(step.map(|s| crate::widgets::muted(crate::i18n::step(&s))))
                .maybe_child(progress.map(|(task, done, total)| {
                    let pct = total
                        .filter(|t| *t > 0)
                        .map(|t| done as f32 * 100. / t as f32)
                        .unwrap_or(0.);
                    rect()
                        .width(Size::fill())
                        .spacing(2.)
                        .child(crate::widgets::muted(task))
                        .child(ProgressBar::new(pct).width(Size::fill()))
                })),
        )
        .child(actions(ui, st))
}

fn actions(mut ui: Ui, st: &Status) -> impl IntoElement {
    let busy = st.operation.is_some();
    let big = |text: &str| {
        label()
            .text(text.to_string())
            .font_size(size::LEAD)
            .font_weight(FontWeight::SEMI_BOLD)
    };
    let row = rect()
        .horizontal()
        .spacing(space::S)
        .cross_align(Alignment::Center);
    if !st.installed {
        return row.child(
            Button::new()
                .filled()
                .on_press(move |_| ui.screen.set(Screen::Wizard))
                .child(big(t("play.install"))),
        );
    }
    if st.session_pid.is_some() {
        return row.child(chip(t("play.running"), Tone::Ok)).child(
            Button::new()
                .on_press(move |_| ui.run(async { svc().stop().await }, |_| {}))
                .child(t("play.stop")),
        );
    }
    if busy {
        return row
            .child(chip(st.operation.clone().unwrap_or_default(), Tone::Accent))
            .child(
                Button::new()
                    .flat()
                    .on_press(|_| svc().cancel())
                    .child(t("cancel")),
            );
    }
    let needs_repair = st.patches.as_ref().is_none_or(|p| !p.intact);
    row.maybe_child(needs_repair.then(|| {
        Button::new()
            .on_press(move |_| ui.run(async { svc().repair().await }, |_| {}))
            .child(t("play.repair"))
    }))
    .child(
        Button::new()
            .flat()
            .on_press(move |_| {
                ui.run(
                    async { svc().launch(LaunchOpts { allow_flat: true }).await },
                    |_| {},
                )
            })
            .child(t("play.flat")),
    )
    .child(
        rect().width(Size::px(180.)).child(
            Button::new()
                .filled()
                .expanded()
                .on_press(move |_| {
                    ui.run(
                        async { svc().launch(LaunchOpts { allow_flat: false }).await },
                        |_| {},
                    )
                })
                .child(
                    rect()
                        .padding((space::S, 0.))
                        .child(big(t("play")).color(T.accent_text)),
                ),
        ),
    )
}
