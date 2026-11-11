//! Settings: a sidebar of sections, each a set of grouped rows.

use aasl_core::Severity;
use aasl_service::LaunchOpts;
use freya::prelude::*;

use crate::i18n::t;
use crate::state::{Screen, Section, Ui};
use aasl_service::PatchSource;

use crate::theme::{RADIUS_PILL, T, alpha, size, space};
use crate::widgets::{Tone, body, chip, group, muted, percent, row, severity_tone, title};
use crate::{approvals, svc};

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

#[derive(PartialEq)]
pub(crate) struct ProtonSection(pub Ui);

impl Component for ProtonSection {
    fn render(&self) -> impl IntoElement {
        proton(self.0)
    }
}

fn view(mut ui: Ui) -> impl IntoElement {
    let section = *ui.section.read();
    let content: Element = match section {
        Section::General => GeneralSection(ui).into_element(),
        Section::Recipes => crate::recipes::RecipesSection(ui).into_element(),
        Section::Vr => vr().into_element(),
        Section::Diagnostics => diagnostics(ui).into_element(),
        Section::Logs => logs(ui).into_element(),
        Section::Agents => agents(ui).into_element(),
    };
    rect()
        .expanded()
        .horizontal()
        .content(Content::Flex)
        .child(
            rect()
                .width(Size::px(236.))
                .height(Size::fill())
                .padding(space::L)
                .spacing(2.)
                .background(T.surface)
                .border(
                    Border::new()
                        .fill(T.border)
                        .width(1.)
                        .alignment(BorderAlignment::Inner),
                )
                .child(
                    Button::new()
                        .compact()
                        .flat()
                        .on_press(move |_| ui.screen.set(Screen::Launcher))
                        .child(format!("← {}", t("nav.back"))),
                )
                .child(
                    rect()
                        .padding((space::S, space::S, space::XL, space::S))
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
                                .expanded()
                                .flat()
                                .background(if selected {
                                    alpha(T.raised, 1.0)
                                } else {
                                    alpha(T.bg, 0.0)
                                })
                                .on_press(move |_| ui.section.set(s))
                                .child(
                                    rect().width(Size::fill()).child(
                                        label()
                                            .text(t(key))
                                            .color(if selected { T.text } else { T.muted })
                                            .font_weight(if selected {
                                                FontWeight::MEDIUM
                                            } else {
                                                FontWeight::NORMAL
                                            }),
                                    ),
                                )
                                .into_element()
                        })
                        .collect::<Vec<_>>(),
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

fn general(mut ui: Ui) -> impl IntoElement {
    let st = ui.status.read().clone();
    let mut wait = use_state(|| svc().config().wait_for_headset);
    let setter = WritableSetter::new(wait, move |v: bool| {
        let mut cfg = svc().config();
        cfg.wait_for_headset = v;
        match svc().set_config(cfg) {
            Ok(()) => wait.set(v),
            Err(e) => ui.error.set(Some(e.to_string())),
        }
    });
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
                    t("prefs.wait"),
                    Some(t("prefs.wait.sub").into()),
                    Some(Switch::new(setter).into_element()),
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

fn source_chip(s: PatchSource, can_build: bool) -> Rect {
    match s {
        PatchSource::Published(aasl_patches::TargetStatus::Verified) => {
            chip(t("prefs.patch.verified"), Tone::Ok)
        }
        PatchSource::Published(_) => chip(t("prefs.patch.untested"), Tone::Warn),
        PatchSource::Built => chip(t("prefs.patch.built"), Tone::Ok),
        PatchSource::NotNeeded => chip(t("prefs.patch.not_needed"), Tone::Ok),
        PatchSource::MustBuild if can_build => chip(t("prefs.patch.on_use"), Tone::Neutral),
        PatchSource::MustBuild => chip(t("prefs.patch.need_builder"), Tone::Error),
    }
}

/// After ProtonPlus: every provider, every recent release, installed
/// builds on top. Using a build without published patches builds them.
fn proton(mut ui: Ui) -> impl IntoElement {
    let mut show_all = use_state(|| false);
    use_hook(move || {
        ui.releases.set(Vec::new());
        ui.load_releases(false)
    });
    let busy = ui.status.read().operation.is_some();
    let progress = ui.progress.read().clone();
    let can_build = svc().patch_builder().is_some();
    let reload = move |mut ui: Ui| {
        ui.installed.set(svc().proton_installed());
        ui.load_releases(false);
    };
    let installed: Vec<Element> = ui
        .installed
        .read()
        .iter()
        .map(|p| {
            let id = p.proton.id.clone();
            let id2 = id.clone();
            let provider = aasl_components::proton::provider(&id.provider)
                .map(|x| x.title)
                .unwrap_or("");
            let patch = match (&p.patches.bundle, p.patches.intact) {
                (Some(_), true) => t("stat.intact").to_string(),
                (Some(_), false) => t("stat.changed").to_string(),
                (None, _) => t("prefs.patch.none").to_string(),
            };
            let usable = can_build || svc().patch_source(&id) != PatchSource::MustBuild;
            let trailing = if p.in_use {
                chip(t("prefs.proton.in_use"), Tone::Accent).into_element()
            } else {
                rect()
                    .horizontal()
                    .spacing(space::S)
                    .child(
                        Button::new()
                            .compact()
                            .enabled(!busy && usable)
                            .on_press(move |_| {
                                let id = id.clone();
                                ui.run(
                                    async move { svc().proton_use(id, false).await },
                                    move |_| reload(ui),
                                )
                            })
                            .child(t("prefs.proton.use")),
                    )
                    .child(
                        Button::new()
                            .compact()
                            .flat()
                            .enabled(!busy)
                            .on_press(move |_| {
                                if let Err(e) = svc().proton_remove(&id2) {
                                    ui.error.set(Some(e.to_string()));
                                }
                                reload(ui);
                            })
                            .child(t("prefs.proton.remove")),
                    )
                    .into_element()
            };
            row(
                p.proton.id.tag.clone(),
                Some(format!("{provider}, {}, {patch}", p.proton.variant)),
                Some(trailing),
            )
        })
        .collect();
    let provider = *ui.provider.read();
    let releases = ui.releases.read().clone();
    let total = releases.len();
    let limit = if *show_all.read() { total } else { 12 };
    let available: Vec<Element> = releases
        .into_iter()
        .take(limit)
        .map(|r| {
            let id = r.id.clone();
            let id2 = id.clone();
            let date = r
                .published_at
                .as_deref()
                .and_then(|d| d.get(..10))
                .unwrap_or("")
                .to_string();
            let mut sub = date;
            if r.size > 0 {
                sub.push_str(&format!(", {:.0} MB", r.size as f64 / 1e6));
            }
            if r.prerelease {
                sub.push_str(&format!(", {}", t("prefs.proton.pre")));
            }
            let downloading = progress.as_ref().filter(|(task, _, _)| *task == r.id.tag);
            let trailing = rect()
                .horizontal()
                .spacing(space::S)
                .cross_align(Alignment::Center)
                .child(source_chip(r.source, can_build))
                .maybe_child(downloading.map(|(_, d, tot)| {
                    rect().width(Size::px(120.)).child(
                        ProgressBar::new(percent(*d, *tot).unwrap_or(0.))
                            .show_progress(false)
                            .width(Size::fill()),
                    )
                }))
                .maybe_child((r.in_use).then(|| chip(t("prefs.proton.in_use"), Tone::Accent)))
                .maybe_child(
                    (!r.installed && r.variant.is_some() && downloading.is_none()).then(|| {
                        Button::new()
                            .compact()
                            .flat()
                            .enabled(!busy)
                            .on_press(move |_| {
                                let id = id.clone();
                                ui.run(async move { svc().proton_install(id).await }, move |_| {
                                    reload(ui)
                                })
                            })
                            .child(t("prefs.proton.download"))
                    }),
                )
                .maybe_child((!r.in_use && r.variant.is_some()).then(|| {
                    Button::new()
                        .compact()
                        .enabled(!busy && (can_build || r.source != PatchSource::MustBuild))
                        .on_press(move |_| {
                            let id = id2.clone();
                            ui.run(
                                async move { svc().proton_use(id, false).await },
                                move |_| reload(ui),
                            )
                        })
                        .child(t("prefs.proton.use"))
                }))
                .into_element();
            row(r.id.tag.clone(), Some(sub), Some(trailing))
        })
        .collect();
    let desc = aasl_components::proton::provider(provider)
        .map(|p| p.description)
        .unwrap_or("");
    rect()
        .spacing(space::XL)
        .child(
            rect()
                .spacing(space::XS)
                .child(muted(t("prefs.proton.lead"))),
        )
        .child(group(
            t("prefs.proton.installed"),
            if installed.is_empty() {
                vec![row(t("stat.none"), None, None)]
            } else {
                installed
            },
        ))
        .child(
            rect()
                .width(Size::fill())
                .spacing(space::M)
                .child(
                    rect()
                        .width(Size::fill())
                        .horizontal()
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .spacing(space::XS)
                        .children(
                            aasl_components::proton::PROVIDERS
                                .iter()
                                .map(|p| {
                                    let id = p.id;
                                    let selected = id == provider;
                                    Button::new()
                                        .key(id)
                                        .compact()
                                        .corner_radius(RADIUS_PILL)
                                        .maybe(selected, |b| b.filled())
                                        .maybe(!selected, |b| b.flat())
                                        .on_press(move |_| {
                                            ui.provider.set(id);
                                            ui.releases.set(Vec::new());
                                            ui.load_releases(false);
                                        })
                                        .child(label().text(p.title).color(if selected {
                                            T.accent_text
                                        } else {
                                            T.text
                                        }))
                                        .into_element()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .child(rect().width(Size::flex(1.)))
                        .child(
                            Button::new()
                                .compact()
                                .flat()
                                .on_press(move |_| ui.load_releases(true))
                                .child(t("prefs.proton.refresh")),
                        ),
                )
                .child(muted(desc)),
        )
        .child(if available.is_empty() {
            rect()
                .width(Size::fill())
                .padding(space::XL)
                .center()
                .child(CircularLoader::new().size(28.))
        } else {
            group(t("prefs.proton.available"), available)
        })
        .maybe_child((total > limit).then(|| {
            Button::new()
                .flat()
                .on_press(move |_| show_all.set(true))
                .child(format!("{} ({})", t("prefs.proton.more"), total - limit))
        }))
}

fn vr() -> impl IntoElement {
    let v = svc().vr_status();
    let active = v
        .active
        .as_ref()
        .map(|r| {
            format!(
                "{} · {}",
                r.name.clone().unwrap_or_else(|| r.kind.to_string()),
                r.manifest.display()
            )
        })
        .unwrap_or_else(|| t("stat.none").into());
    let openvr = v
        .openvr
        .as_ref()
        .map(|o| format!("{:?} · {}", o.layer, o.path.display()))
        .unwrap_or_else(|| t("stat.none").into());
    let installed = if v.installed.is_empty() {
        t("stat.none").to_string()
    } else {
        v.installed
            .iter()
            .map(|k| k.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut blockers: Vec<Element> = v
        .blockers
        .iter()
        .map(|b| row(b.clone(), None, Some(chip("!", Tone::Warn).into_element())))
        .collect();
    blockers.extend(v.notes.iter().map(|n| row(n.clone(), None, None)));
    if blockers.is_empty() {
        blockers.push(row(
            t("prefs.vr.ready"),
            None,
            Some(chip(t("stat.ready"), Tone::Ok).into_element()),
        ));
    }
    rect()
        .spacing(space::XL)
        .child(title(t("prefs.vr")))
        .child(group(
            t("prefs.vr"),
            vec![
                row(
                    t("prefs.vr.runtime"),
                    Some(active),
                    Some(
                        chip(
                            if v.ready {
                                t("stat.ready")
                            } else {
                                t("stat.not_ready")
                            },
                            if v.ready { Tone::Ok } else { Tone::Warn },
                        )
                        .into_element(),
                    ),
                ),
                row(t("prefs.vr.installed"), Some(installed), None),
                row(t("prefs.vr.openvr"), Some(openvr), None),
            ],
        ))
        .child(group(t("prefs.vr.blockers"), blockers))
}

fn diagnostics(mut ui: Ui) -> impl IntoElement {
    let findings = ui.findings.read().clone();
    let rows: Vec<Element> = match &findings {
        None => vec![],
        Some(v) if v.is_empty() => vec![row(t("diag.none"), None, None)],
        Some(v) => v
            .iter()
            .map(|f| {
                let mut sub = f.evidence.join("\n");
                if let Some(a) = &f.advice {
                    if !sub.is_empty() {
                        sub.push('\n');
                    }
                    sub.push_str(a);
                }
                let fix = f.fix;
                let trailing = match fix {
                    Some(fix) => Button::new()
                        .on_press(move |_| {
                            ui.run(async move { svc().apply_fix(fix, false).await }, move |o| {
                                ui.notice(Severity::Info, o.message);
                                ui.findings.set(None);
                            })
                        })
                        .child(t("diag.apply"))
                        .into_element(),
                    None => chip(
                        format!("{:?}", f.severity).to_lowercase(),
                        severity_tone(f.severity),
                    )
                    .into_element(),
                };
                row(
                    f.title.clone(),
                    (!sub.is_empty()).then_some(sub),
                    Some(trailing),
                )
            })
            .collect(),
    };
    rect()
        .spacing(space::XL)
        .child(title(t("prefs.diagnostics")))
        .child(
            rect()
                .horizontal()
                .spacing(space::S)
                .child(
                    Button::new()
                        .filled()
                        .on_press(move |_| {
                            spawn_in_window(async move {
                                if let Ok(v) =
                                    tokio::spawn(async { svc().doctor(true).await }).await
                                {
                                    ui.findings.set(Some(v));
                                }
                            });
                        })
                        .child(t("diag.run")),
                )
                .child(
                    Button::new()
                        .flat()
                        .on_press(move |_| {
                            spawn_in_window(async move {
                                if let Ok(r) = tokio::spawn(async { svc().report().await }).await {
                                    let p = svc().paths().logs_dir().join("bug-report.json");
                                    let _ = std::fs::create_dir_all(svc().paths().logs_dir());
                                    match std::fs::write(&p, r) {
                                        Ok(()) => {
                                            ui.notice(Severity::Info, p.display().to_string())
                                        }
                                        Err(e) => ui.error.set(Some(e.to_string())),
                                    }
                                }
                            });
                        })
                        .child(t("diag.report")),
                ),
        )
        .maybe_child((!rows.is_empty()).then(|| group(t("prefs.diagnostics"), rows)))
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

fn agents(ui: Ui) -> impl IntoElement {
    let audit = std::fs::read_to_string(svc().paths().audit_log()).unwrap_or_default();
    let recent: Vec<Element> = audit
        .lines()
        .rev()
        .take(15)
        .map(|l| row(l.to_string(), None, None))
        .collect();
    let pending: Vec<Element> = ui
        .approvals
        .read()
        .iter()
        .map(|(id, msg)| {
            let id = *id;
            row(
                msg.clone(),
                Some(t("agents.pending").into()),
                Some(
                    rect()
                        .horizontal()
                        .spacing(space::S)
                        .child(
                            Button::new()
                                .filled()
                                .on_press(move |_| approvals::answer(id, true))
                                .child(t("agents.approve")),
                        )
                        .child(
                            Button::new()
                                .on_press(move |_| approvals::answer(id, false))
                                .child(t("agents.deny")),
                        )
                        .into_element(),
                ),
            )
        })
        .collect();
    rect()
        .spacing(space::XL)
        .child(title(t("prefs.agents")))
        .child(body(t("agents.lead")))
        .child(muted(format!(
            "{}: {}",
            t("agents.socket"),
            svc().paths().socket().display()
        )))
        .maybe_child((!pending.is_empty()).then(|| group(t("agents.pending"), pending)))
        .maybe_child((!recent.is_empty()).then(|| group(t("agents.audit"), recent)))
}
