//! Settings: a sidebar of sections, each a set of grouped rows.

use aasl_service::LaunchOpts;
use freya::prelude::*;

use crate::i18n::t;
use crate::state::{Screen, Section, Ui};

use crate::svc;
use crate::theme::{T, size, space};
use crate::widgets::{Icon, Tone, dot, group, muted, row, title};

const SECTIONS: &[(Section, &str)] = &[
    (Section::General, "prefs.general"),
    (Section::Recipes, "prefs.recipes"),
    (Section::Vr, "prefs.vr"),
    (Section::Diagnostics, "prefs.diagnostics"),
    (Section::Logs, "prefs.logs"),
    (Section::Agents, "prefs.agents"),
];

#[derive(PartialEq)]
pub struct SettingsScreen(pub Ui);

impl Component for SettingsScreen {
    fn render(&self) -> impl IntoElement {
        view(self.0)
    }
}

#[derive(PartialEq)]
struct GeneralSection(Ui);

impl Component for GeneralSection {
    fn render(&self) -> impl IntoElement {
        general(self.0)
    }
}

fn view(mut ui: Ui) -> impl IntoElement {
    let section = *ui.section.read();
    let content: Element = match section {
        Section::General => GeneralSection(ui).into_element(),
        Section::Recipes => crate::recipes::RecipesSection(ui).into_element(),
        Section::Vr => crate::vr::VrSection(ui).into_element(),
        Section::Diagnostics => crate::diagnostics::DiagnosticsSection(ui).into_element(),
        Section::Logs => logs(ui).into_element(),
        Section::Agents => crate::agents::AgentsSection(ui).into_element(),
    };
    let st = ui.status.read().clone();
    let findings = ui.findings.read().clone();
    let pending = !ui.approvals.read().is_empty();
    let health = move |s: Section| -> Option<Tone> {
        match s {
            Section::Vr => Some(if st.vr.ready { Tone::Ok } else { Tone::Warn }),
            Section::Recipes => Some(
                if st.proton.is_some() && st.patches.as_ref().is_some_and(|p| p.intact) {
                    Tone::Ok
                } else {
                    Tone::Warn
                },
            ),
            Section::Diagnostics => findings.as_ref().map(|f| {
                if f.iter().any(|f| f.severity == aasl_core::Severity::Error) {
                    Tone::Error
                } else if f.iter().any(|f| f.severity == aasl_core::Severity::Warn) {
                    Tone::Warn
                } else {
                    Tone::Ok
                }
            }),
            Section::Agents => pending.then_some(Tone::Accent),
            _ => None,
        }
    };
    let nav_row = |icon: Icon, text: String, selected: bool, dot_tone: Option<Tone>| {
        let fg = if selected { T.text } else { T.muted };
        rect()
            .width(Size::fill())
            .height(Size::px(36.))
            .horizontal()
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .spacing(space::M)
            .child(icon.view(18., fg))
            .child(
                label()
                    .text(text)
                    .width(Size::flex(1.))
                    .color(fg)
                    .font_weight(if selected {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    }),
            )
            .maybe_child(dot_tone.map(dot))
    };
    rect()
        .expanded()
        .horizontal()
        .content(Content::Flex)
        .child(
            rect()
                .width(Size::px(288.))
                .height(Size::fill())
                .content(Content::Flex)
                .padding((space::XL, space::M))
                .spacing(space::XS)
                .background(T.surface)
                .border(
                    Border::new()
                        .fill(crate::widgets::hairline())
                        .width(BorderWidth {
                            right: 1.,
                            ..Default::default()
                        })
                        .alignment(BorderAlignment::Inner),
                )
                .child(
                    rect()
                        .padding((0., space::M, space::XL, space::M))
                        .child(crate::app::brand()),
                )
                .children(
                    SECTIONS
                        .iter()
                        .map(|(s, key)| {
                            let s = *s;
                            let selected = s == section;
                            Button::new()
                                .key(*key)
                                .width(Size::fill())
                                .padding((0., space::M))
                                .flat()
                                .maybe(selected, |b| b.background(T.raised))
                                .on_press(move |_| ui.section.set(s))
                                .child(nav_row(
                                    section_icon(s),
                                    t(key).to_string(),
                                    selected,
                                    health(s),
                                ))
                                .into_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .child(rect().height(Size::flex(1.)))
                .child(
                    Button::new()
                        .width(Size::fill())
                        .padding((0., space::M))
                        .flat()
                        .on_press(move |_| ui.screen.set(Screen::Launcher))
                        .child(nav_row(Icon::Back, t("nav.back").to_string(), false, None)),
                )
                .child(
                    rect()
                        .padding((space::S, space::M, 0., space::M))
                        .child(muted(format!("AASL {}", aasl_update::Version::current()))),
                ),
        )
        .child(
            ScrollView::new()
                .width(Size::flex(1.))
                .height(Size::fill())
                .child(
                    rect()
                        .width(Size::fill())
                        .max_width(Size::px(860.))
                        .padding((space::XXXL, space::XXXL))
                        .spacing(space::XL)
                        .child(content),
                ),
        )
}

fn section_icon(s: Section) -> Icon {
    match s {
        Section::General => Icon::Gear,
        Section::Recipes => Icon::Flask,
        Section::Vr => Icon::Headset,
        Section::Diagnostics => Icon::Pulse,
        Section::Logs => Icon::Lines,
        Section::Agents => Icon::Bot,
    }
}

fn general(mut ui: Ui) -> impl IntoElement {
    let st = ui.status.read().clone();
    let busy = st.operation.is_some();
    let mut auto = use_state(|| svc().config().check_updates);
    let auto_setter = WritableSetter::new(auto, move |v: bool| {
        let mut cfg = svc().config();
        cfg.check_updates = v;
        match svc().set_config(cfg) {
            Ok(()) => auto.set(v),
            Err(e) => ui.error.set(Some(e.to_string())),
        }
    });
    let mut beta = use_state(|| svc().config().update_channel == "beta");
    let beta_setter = WritableSetter::new(beta, move |v: bool| {
        let mut cfg = svc().config();
        cfg.update_channel = if v { "beta" } else { "stable" }.into();
        match svc().set_config(cfg) {
            Ok(()) => beta.set(v),
            Err(e) => ui.error.set(Some(e.to_string())),
        }
    });
    let cur_locale = svc().config().locale;
    let lang_btn = |label: &'static str, val: Option<&'static str>| {
        let active = cur_locale.as_deref() == val;
        let b = Button::new();
        let b = if active { b.filled() } else { b.flat() };
        b.on_press(move |_| {
            if active {
                return;
            }
            let mut cfg = svc().config();
            cfg.locale = val.map(String::from);
            match svc().set_config(cfg) {
                Ok(()) => {
                    crate::i18n::set(crate::i18n::detect());
                    ui.lang.set(crate::i18n::current());
                }
                Err(e) => ui.error.set(Some(e.to_string())),
            }
        })
        .child(label)
        .into_element()
    };
    let lang_row = rect()
        .horizontal()
        .spacing(space::S)
        .child(lang_btn(t("prefs.lang.auto"), None))
        .child(lang_btn("English", Some("en")))
        .child(lang_btn("Français", Some("fr")))
        .into_element();
    let current = aasl_update::Version::current().to_string();
    let update_line = match ui.update.read().as_ref() {
        Some(u) => match &u.available {
            Some(r) => format!("{current} → {}", r.version),
            None => format!("{current} · {}", t("prefs.updates.uptodate")),
        },
        None => current,
    };
    let can_install = ui
        .update
        .read()
        .as_ref()
        .is_some_and(|u| u.available.is_some() && u.can_apply);
    let restart = *ui.restart_ready.read();
    let version_action = if restart {
        Button::new()
            .filled()
            .on_press(|_| crate::restart())
            .child(t("update.restart"))
            .into_element()
    } else if can_install {
        Button::new()
            .filled()
            .enabled(!busy)
            .on_press(move |_| ui.update_now())
            .child(t("update.install"))
            .into_element()
    } else {
        Button::new()
            .flat()
            .on_press(move |_| {
                ui.run(async { svc().update_check().await }, move |i| {
                    ui.update.set(Some(i))
                })
            })
            .child(t("prefs.updates.check"))
            .into_element()
    };
    rect()
        .spacing(space::XL)
        .child(title(t("prefs.general")))
        .child(group(
            crate::i18n::tr("Somnium Space account", "Compte Somnium Space"),
            vec![crate::events::AccountForm(ui).into_element()],
        ))
        .child(group(
            t("prefs.updates"),
            vec![
                row(
                    format!("AASL {}", t("prefs.updates.version")),
                    Some(update_line),
                    Some(version_action),
                ),
                row(
                    t("prefs.updates.auto"),
                    None,
                    Some(Switch::new(auto_setter).into_element()),
                ),
                row(
                    t("prefs.updates.beta"),
                    None,
                    Some(Switch::new(beta_setter).into_element()),
                ),
            ],
        ))
        .child(group(
            t("prefs.general"),
            vec![
                row(
                    t("prefs.lang"),
                    Some(t("prefs.lang.sub").into()),
                    Some(lang_row),
                ),
                row(
                    t("prefs.prefix"),
                    Some(st.prefix.display().to_string()),
                    None,
                ),
                row(
                    t("play.flat"),
                    None,
                    Some(
                        Button::new()
                            .flat()
                            .enabled(st.installed && !busy)
                            .on_press(move |_| {
                                ui.run(
                                    async { svc().launch(LaunchOpts { allow_flat: true }).await },
                                    |_| {},
                                )
                            })
                            .child(t("play"))
                            .into_element(),
                    ),
                ),
                row(
                    t("prefs.repair"),
                    Some(t("prefs.repair.sub").into()),
                    Some(
                        Button::new()
                            .enabled(st.installed && !busy)
                            .on_press(move |_| ui.run(async { svc().repair().await }, |_| {}))
                            .child(t("play.repair"))
                            .into_element(),
                    ),
                ),
            ],
        ))
}

fn logs(ui: Ui) -> impl IntoElement {
    let lines = ui.logs.read().clone();
    rect()
        .spacing(space::M)
        .child(title(t("prefs.logs")))
        .child(muted(svc().paths().launch_log().display().to_string()))
        .child(
            rect()
                .width(Size::fill())
                .padding(space::L)
                .spacing(2.)
                .background(T.surface)
                .corner_radius(10.)
                .children(
                    lines
                        .into_iter()
                        .enumerate()
                        .map(|(i, l)| {
                            label()
                                .key(i)
                                .text(l)
                                .font_size(size::SMALL)
                                .font_family("monospace")
                        })
                        .collect::<Vec<_>>(),
                ),
        )
}
