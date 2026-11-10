//! Settings: a sidebar of sections, each a set of grouped rows.

use aasl_core::Severity;
use aasl_service::LaunchOpts;
use freya::prelude::*;

use crate::i18n::t;
use crate::state::{Screen, Section, Ui};
use crate::theme::{T, size, space};
use crate::widgets::{Tone, body, chip, group, muted, row, severity_tone, title};
use crate::{approvals, svc};

const SECTIONS: &[(Section, &str)] = &[
    (Section::General, "prefs.general"),
    (Section::Proton, "prefs.proton"),
    (Section::Patches, "prefs.patches"),
    (Section::Vr, "prefs.vr"),
    (Section::Diagnostics, "prefs.diagnostics"),
    (Section::Logs, "prefs.logs"),
    (Section::Agents, "prefs.agents"),
];

pub fn view(mut ui: Ui) -> impl IntoElement {
    let section = *ui.section.read();
    let content: Element = match section {
        Section::General => general(ui).into_element(),
        Section::Proton => proton(ui).into_element(),
        Section::Patches => patches(ui).into_element(),
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
                .width(Size::px(220.))
                .height(Size::fill())
                .padding(space::L)
                .spacing(space::XS)
                .background(T.surface)
                .child(
                    rect()
                        .padding((0., 0., space::L, 0.))
                        .child(crate::app::brand()),
                )
                .child(
                    Button::new()
                        .flat()
                        .expanded()
                        .on_press(move |_| ui.screen.set(Screen::Launcher))
                        .child(t("nav.back")),
                )
                .child(rect().height(Size::px(space::S)))
                .children(
                    SECTIONS
                        .iter()
                        .map(|(s, key)| {
                            let s = *s;
                            let b = Button::new().key(*key).expanded().on_press(move |_| {
                                ui.section.set(s);
                                if s == Section::Proton && ui.releases.read().is_empty() {
                                    ui.load_releases(false);
                                }
                            });
                            let b = if s == section { b.filled() } else { b.flat() };
                            b.child(t(key))
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
                        .max_width(Size::px(820.))
                        .padding(space::XXL)
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
    rect()
        .spacing(space::XL)
        .child(title(t("prefs.general")))
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

fn patch_chip(p: Option<aasl_patches::TargetStatus>) -> Rect {
    match p {
        Some(aasl_patches::TargetStatus::Verified) => chip(t("prefs.patch.verified"), Tone::Ok),
        Some(_) => chip(t("prefs.patch.untested"), Tone::Warn),
        None => chip(t("prefs.patch.none"), Tone::Neutral),
    }
}

fn proton(mut ui: Ui) -> impl IntoElement {
    let busy = ui.status.read().operation.is_some();
    let installed: Vec<Element> = ui
        .installed
        .read()
        .iter()
        .map(|p| {
            let id = p.proton.id.clone();
            let id2 = id.clone();
            let patch = match (&p.patches.bundle, p.patches.intact) {
                (Some(b), true) => format!("{} · {b}", p.proton.variant),
                (Some(b), false) => format!("{} · {b} ({})", p.proton.variant, t("stat.changed")),
                (None, _) => format!("{} · {}", p.proton.variant, t("prefs.patch.none")),
            };
            let trailing = if p.in_use {
                chip(t("prefs.proton.in_use"), Tone::Ok).into_element()
            } else {
                rect()
                    .horizontal()
                    .spacing(space::S)
                    .child(
                        Button::new()
                            .flat()
                            .enabled(!busy)
                            .on_press(move |_| {
                                let id = id.clone();
                                ui.run(async move { svc().proton_use(id, false).await }, |_| {})
                            })
                            .child(t("prefs.proton.use")),
                    )
                    .child(
                        Button::new()
                            .flat()
                            .on_press(move |_| {
                                if let Err(e) = svc().proton_remove(&id2) {
                                    ui.error.set(Some(e.to_string()));
                                }
                                ui.installed.set(svc().proton_installed());
                            })
                            .child(t("prefs.proton.remove")),
                    )
                    .into_element()
            };
            row(p.proton.id.to_string(), Some(patch), Some(trailing))
        })
        .collect();
    let provider = *ui.provider.read();
    let available: Vec<Element> = ui
        .releases
        .read()
        .iter()
        .take(12)
        .map(|r| {
            let id = r.id.clone();
            let id2 = id.clone();
            let date = r
                .published_at
                .as_deref()
                .and_then(|d| d.get(..10))
                .unwrap_or("")
                .to_string();
            let trailing = rect()
                .horizontal()
                .spacing(space::S)
                .cross_align(Alignment::Center)
                .child(patch_chip(r.patches))
                .maybe_child((!r.installed).then(|| {
                    Button::new()
                        .flat()
                        .enabled(!busy)
                        .on_press(move |_| {
                            let id = id.clone();
                            ui.run(async move { svc().proton_install(id).await }, |_| {})
                        })
                        .child(t("prefs.proton.download"))
                }))
                .maybe_child((!r.in_use && r.patches.is_some()).then(|| {
                    Button::new()
                        .enabled(!busy)
                        .on_press(move |_| {
                            let id = id2.clone();
                            ui.run(async move { svc().proton_use(id, false).await }, |_| {})
                        })
                        .child(t("prefs.proton.use"))
                }))
                .into_element();
            row(
                r.id.tag.clone(),
                Some(format!(
                    "{date}{}",
                    r.variant
                        .as_deref()
                        .map(|v| format!(" · {v}"))
                        .unwrap_or_default()
                )),
                Some(trailing),
            )
        })
        .collect();
    let mut set_provider = move |p: &'static str| {
        ui.provider.set(p);
        ui.releases.set(Vec::new());
        ui.load_releases(false);
    };
    rect()
        .spacing(space::XL)
        .child(title(t("prefs.proton")))
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
                .horizontal()
                .spacing(space::S)
                .cross_align(Alignment::Center)
                .children(
                    aasl_components::proton::PROVIDERS
                        .iter()
                        .map(|p| {
                            let id = p.id;
                            let b = Button::new().key(id).on_press(move |_| set_provider(id));
                            let b = if id == provider { b.filled() } else { b.flat() };
                            b.child(p.title)
                        })
                        .collect::<Vec<_>>(),
                )
                .child(
                    Button::new()
                        .flat()
                        .on_press(move |_| ui.load_releases(true))
                        .child(t("prefs.proton.refresh")),
                ),
        )
        .child(group(t("prefs.proton.available"), available))
}

fn patches(mut ui: Ui) -> impl IntoElement {
    let st = ui.status.read().clone();
    let busy = st.operation.is_some();
    let current = match &st.patches {
        Some(p) => match (&p.bundle, p.intact) {
            (Some(b), true) => row(
                b.clone(),
                Some(t("stat.intact").into()),
                Some(chip(t("stat.intact"), Tone::Ok).into_element()),
            ),
            (Some(b), false) => row(
                b.clone(),
                Some(p.changed.join(", ")),
                Some(chip(t("stat.changed"), Tone::Error).into_element()),
            ),
            (None, _) => row(
                t("stat.none"),
                None,
                Some(chip(t("stat.none"), Tone::Error).into_element()),
            ),
        },
        None => row(t("stat.none"), None, None),
    };
    let files: Vec<Element> = svc()
        .patch_list()
        .unwrap_or_default()
        .into_iter()
        .map(|p| row(p.subject, Some(format!("{} · {}", p.series, p.name)), None))
        .collect();
    let proton = svc().current_proton();
    rect()
        .spacing(space::XL)
        .child(title(t("prefs.patches")))
        .child(group(
            t("prefs.patches.current"),
            vec![
                current,
                row(
                    t("prefs.patches.build"),
                    Some(t("prefs.patches.build.sub").into()),
                    Some(
                        Button::new()
                            .enabled(!busy && proton.is_some())
                            .on_press(move |_| {
                                if let Some(id) = proton.clone() {
                                    ui.section.set(Section::Logs);
                                    ui.run(
                                        async move { svc().patch_build(id, false, &[]).await },
                                        move |p| ui.notice(Severity::Info, p.display().to_string()),
                                    );
                                }
                            })
                            .child(t("prefs.patches.build"))
                            .into_element(),
                    ),
                ),
                row(
                    t("prefs.patches.restore"),
                    None,
                    Some(
                        Button::new()
                            .flat()
                            .enabled(!busy)
                            .on_press(move |_| ui.run(async { svc().patch_restore() }, |_| {}))
                            .child(t("prefs.patches.restore"))
                            .into_element(),
                    ),
                ),
            ],
        ))
        .child(group(t("prefs.patches.files"), files))
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
                            spawn(async move {
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
                            spawn(async move {
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
