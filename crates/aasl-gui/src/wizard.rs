//! First run: welcome, system check, Proton choice, Somnium's installer,
//! progress, done.

use aasl_core::Severity;
use aasl_service::InstallOptions;
use freya::prelude::*;

use crate::i18n::{step, t};
use crate::state::{Screen, Ui};
use crate::theme::{T, size, space};
use crate::widgets::{Tone, body, chip, glass_panel, muted, title};
use crate::{app, svc};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Welcome,
    System,
    Proton,
    Installer,
    Installing,
    Done,
}

const STEPS: &[&str] = &[
    "check_deps",
    "stop_running",
    "catalog",
    "umu",
    "proton",
    "patches",
    "prefix",
    "somnium_installer",
    "desktop_entry",
];

pub fn view(ui: Ui) -> impl IntoElement {
    let page = use_state(|| Page::Welcome);
    let path = use_state(String::new);
    let chosen = use_state(|| svc().catalog_now().recommended_proton.clone());
    let content: Element = match *page.read() {
        Page::Welcome => welcome(page).into_element(),
        Page::System => system(page).into_element(),
        Page::Proton => proton(ui, page, chosen).into_element(),
        Page::Installer => installer(ui, page, path, chosen).into_element(),
        Page::Installing => installing(ui, page).into_element(),
        Page::Done => done(ui).into_element(),
    };
    rect()
        .expanded()
        .padding(space::XL)
        .content(Content::Flex)
        .child(app::brand())
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.))
                .center()
                .child(
                    glass_panel()
                        .width(Size::px(620.))
                        .padding(space::XXL)
                        .spacing(space::L)
                        .child(dots(*page.read()))
                        .child(content),
                ),
        )
}

fn dots(current: Page) -> impl IntoElement {
    let pages = [
        Page::Welcome,
        Page::System,
        Page::Proton,
        Page::Installer,
        Page::Installing,
        Page::Done,
    ];
    let idx = pages.iter().position(|p| *p == current).unwrap_or(0);
    rect().horizontal().spacing(6.).children(
        (0..pages.len())
            .map(|i| {
                rect()
                    .key(i)
                    .width(Size::px(if i == idx { 22. } else { 8. }))
                    .height(Size::px(8.))
                    .corner_radius(4.)
                    .background(if i <= idx { T.accent } else { T.border })
            })
            .collect::<Vec<_>>(),
    )
}

fn nav(
    mut page: State<Page>,
    back: Option<Page>,
    next: Option<(Page, &'static str, bool)>,
) -> impl IntoElement {
    rect()
        .width(Size::fill())
        .horizontal()
        .main_align(Alignment::End)
        .spacing(space::S)
        .padding((space::S, 0., 0., 0.))
        .maybe_child(back.map(|b| {
            Button::new()
                .flat()
                .on_press(move |_| page.set(b))
                .child(t("wiz.back"))
        }))
        .maybe_child(next.map(|(n, label_key, enabled)| {
            Button::new()
                .filled()
                .enabled(enabled)
                .on_press(move |_| page.set(n))
                .child(t(label_key))
        }))
}

fn welcome(page: State<Page>) -> impl IntoElement {
    rect()
        .spacing(space::M)
        .child(
            label()
                .text(t("wiz.welcome.title"))
                .font_size(size::DISPLAY)
                .font_weight(FontWeight::BOLD)
                .color(T.text),
        )
        .child(body(t("wiz.welcome.body")))
        .child(nav(page, None, Some((Page::System, "wiz.start", true))))
}

fn check(label_text: String, ok: bool, warn_only: bool) -> Rect {
    let tone = if ok {
        Tone::Ok
    } else if warn_only {
        Tone::Warn
    } else {
        Tone::Error
    };
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(space::M)
        .child(
            rect()
                .width(Size::px(10.))
                .height(Size::px(10.))
                .corner_radius(5.)
                .background(tone.color()),
        )
        .child(rect().width(Size::flex(1.)).child(body(label_text)))
}

fn system(page: State<Page>) -> impl IntoElement {
    let sys = svc().system();
    let vr = svc().vr_status();
    let distro = sys
        .distro
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Linux".into());
    let py_ok = sys.python.is_some_and(|p| p.supported());
    let py = sys
        .python
        .map(|p| format!("Python {p}"))
        .unwrap_or_else(|| "Python 3 missing".into());
    let gpu = sys
        .gpus
        .first()
        .map(|g| {
            format!(
                "{} GPU, {} Vulkan driver(s)",
                g.vendor,
                sys.vulkan_icds.len()
            )
        })
        .unwrap_or_else(|| "No GPU found".into());
    let runtimes = if vr.installed.is_empty() {
        "No VR runtime installed".to_string()
    } else {
        format!(
            "VR runtimes: {}",
            vr.installed
                .iter()
                .map(|k| k.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let blocking = !py_ok || sys.vulkan_icds.is_empty();
    rect()
        .spacing(space::M)
        .child(title(t("wiz.system.title")))
        .child(muted(t("wiz.system.body")))
        .child(check(distro, true, false))
        .child(check(py, py_ok, false))
        .child(check(gpu, !sys.vulkan_icds.is_empty(), false))
        .child(check(runtimes, !vr.installed.is_empty(), true))
        .children(
            vr.blockers
                .iter()
                .map(|b| muted(format!("· {b}")))
                .collect::<Vec<_>>(),
        )
        .child(nav(
            page,
            Some(Page::Welcome),
            Some((Page::Proton, "wiz.next", !blocking)),
        ))
}

fn proton(ui: Ui, page: State<Page>, mut chosen: State<String>) -> impl IntoElement {
    let cat = svc().catalog_now();
    let recommended = cat.recommended_proton.clone();
    let mut options: Vec<String> = vec![recommended.clone()];
    for v in cat.verified_protons() {
        if !options.contains(&v) {
            options.push(v);
        }
    }
    for r in ui
        .releases
        .read()
        .iter()
        .filter(|r| r.patches.is_some())
        .take(4)
    {
        let id = r.id.to_string();
        if !options.contains(&id) {
            options.push(id);
        }
    }
    let current = chosen.read().clone();
    rect()
        .spacing(space::M)
        .child(title(t("wiz.proton.title")))
        .child(muted(t("wiz.proton.body")))
        .children(
            options
                .into_iter()
                .map(|id| {
                    let selected = id == current;
                    let status = aasl_components::ProtonId::parse(&id)
                        .ok()
                        .and_then(|p| cat.resolve(&p).first().map(|m| m.status));
                    let (txt, tone) = match status {
                        Some(aasl_patches::TargetStatus::Verified) => {
                            (t("prefs.patch.verified"), Tone::Ok)
                        }
                        Some(_) => (t("prefs.patch.untested"), Tone::Warn),
                        None => (t("prefs.patch.none"), Tone::Error),
                    };
                    let id2 = id.clone();
                    Button::new()
                        .key(id.clone())
                        .expanded()
                        .on_press(move |_| chosen.set(id2.clone()))
                        .maybe(selected, |b| b.filled())
                        .maybe(!selected, |b| b.flat())
                        .child(
                            rect()
                                .width(Size::fill())
                                .horizontal()
                                .content(Content::Flex)
                                .cross_align(Alignment::Center)
                                .spacing(space::S)
                                .child(rect().width(Size::flex(1.)).child(label().text(id.clone())))
                                .maybe_child(
                                    (id == recommended)
                                        .then(|| chip(t("wiz.proton.recommended"), Tone::Accent)),
                                )
                                .child(chip(txt, tone)),
                        )
                })
                .collect::<Vec<_>>(),
        )
        .child(nav(
            page,
            Some(Page::System),
            Some((Page::Installer, "wiz.next", true)),
        ))
}

fn installer(
    ui: Ui,
    mut page: State<Page>,
    path: State<String>,
    chosen: State<String>,
) -> impl IntoElement {
    let raw = path.read().trim().to_string();
    let p = expand(&raw);
    let valid = p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    rect()
        .spacing(space::M)
        .child(title(t("wiz.installer.title")))
        .child(muted(t("wiz.installer.body")))
        .child(
            Input::new(path)
                .placeholder(t("wiz.installer.path"))
                .width(Size::fill()),
        )
        .maybe_child((!raw.is_empty() && !valid).then(|| {
            label()
                .text(format!("{} ?", p.display()))
                .font_size(size::SMALL)
                .color(T.warn)
        }))
        .child(
            rect()
                .width(Size::fill())
                .horizontal()
                .main_align(Alignment::End)
                .spacing(space::S)
                .child(
                    Button::new()
                        .flat()
                        .on_press(move |_| page.set(Page::Proton))
                        .child(t("wiz.back")),
                )
                .child(
                    Button::new()
                        .filled()
                        .enabled(valid)
                        .on_press(move |_| {
                            let proton = chosen.read().clone();
                            let exe = expand(path.read().trim());
                            page.set(Page::Installing);
                            let mut cfg = svc().config();
                            cfg.proton = Some(proton);
                            if let Err(e) = svc().set_config(cfg) {
                                ui.notice(Severity::Error, e.to_string());
                            }
                            let opts = InstallOptions {
                                somnium_installer: Some(exe),
                                local_bundle: None,
                                aasl_exe: cli_path(),
                            };
                            ui.run(async move { svc().install(opts).await }, move |_| {
                                page.set(Page::Done)
                            });
                        })
                        .child(t("wiz.install")),
                ),
        )
}

fn installing(ui: Ui, mut page: State<Page>) -> impl IntoElement {
    let done = ui.done_steps.read().clone();
    let current = ui.step.read().clone();
    let progress = ui.progress.read().clone();
    let failed = ui.error.read().is_some() && ui.status.read().operation.is_none();
    rect()
        .spacing(space::S)
        .child(title(t("wiz.installing.title")))
        .children(
            STEPS
                .iter()
                .map(|s| {
                    let (dot, color) = if done.iter().any(|d| d == s) {
                        ("●", T.ok)
                    } else if current.as_deref() == Some(*s) {
                        ("◐", T.accent)
                    } else {
                        ("○", T.muted)
                    };
                    rect()
                        .key(*s)
                        .horizontal()
                        .spacing(space::M)
                        .child(label().text(dot).color(color))
                        .child(label().text(step(s)).color(if color == T.muted {
                            T.muted
                        } else {
                            T.text
                        }))
                })
                .collect::<Vec<_>>(),
        )
        .maybe_child(progress.map(|(task, d, total)| {
            let pct = total
                .filter(|t| *t > 0)
                .map(|t| d as f32 * 100. / t as f32)
                .unwrap_or(0.);
            rect()
                .width(Size::fill())
                .spacing(2.)
                .child(muted(task))
                .child(ProgressBar::new(pct).width(Size::fill()))
        }))
        .children(
            ui.notices
                .read()
                .iter()
                .rev()
                .take(3)
                .map(|(sev, m)| {
                    label()
                        .text(m.clone())
                        .font_size(size::SMALL)
                        .color(crate::widgets::severity_tone(*sev).color())
                })
                .collect::<Vec<_>>(),
        )
        .maybe_child(failed.then(|| {
            rect()
                .horizontal()
                .main_align(Alignment::End)
                .width(Size::fill())
                .child(
                    Button::new()
                        .on_press(move |_| page.set(Page::Installer))
                        .child(t("wiz.back")),
                )
        }))
}

fn done(mut ui: Ui) -> impl IntoElement {
    rect()
        .spacing(space::M)
        .child(title(t("wiz.done.title")))
        .child(body(t("wiz.done.body")))
        .child(
            rect()
                .width(Size::fill())
                .horizontal()
                .main_align(Alignment::End)
                .child(
                    Button::new()
                        .filled()
                        .on_press(move |_| ui.screen.set(Screen::Launcher))
                        .child(t("wiz.done.go")),
                ),
        )
}

pub fn expand(p: &str) -> std::path::PathBuf {
    let p = p.trim_matches(|c| c == '\'' || c == '"');
    let p = p.strip_prefix("file://").unwrap_or(p);
    match p.strip_prefix("~/") {
        Some(rest) => svc().paths().home.join(rest),
        None => p.into(),
    }
}

/// The desktop entry should start the `aasl` CLI that sits next to us.
pub fn cli_path() -> Option<std::path::PathBuf> {
    let me = std::env::current_exe().ok()?;
    let cli = me.with_file_name("aasl");
    Some(if cli.exists() { cli } else { me })
}
