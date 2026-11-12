//! First run: welcome, Proton choice, where Somnium's installer comes
//! from, then a silent background install behind a loading screen.

use std::path::PathBuf;

use freya::prelude::*;
use unilux_core::Severity;
use unilux_service::InstallOptions;

use crate::i18n::{step, t};
use crate::state::{Screen, Ui};
use crate::theme::{RADIUS_PILL, T, alpha, space};
use crate::widgets::{
    Tone, body, chip, display, dot, link, muted, percent, pill, proton_label, title,
};
use crate::{app, svc};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Welcome,
    Proton,
    Account,
    Source,
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

const PHRASES: usize = 10;

#[derive(PartialEq)]
pub struct WizardScreen(pub Ui);

impl Component for WizardScreen {
    fn render(&self) -> impl IntoElement {
        let ui = self.0;
        let page = use_state(first_page);
        let chosen = use_state(|| svc().catalog_now().recommended_proton.clone());
        let picked = use_state(|| None::<PathBuf>);
        let mut hover = use_state(|| false);
        let mut phrase = use_state(|| 0usize);
        let mut failed = use_state(|| None::<String>);
        use_future(move || async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(4500)).await;
                let n = (*phrase.peek() + 1 + (now_ms() % 3) as usize) % PHRASES;
                phrase.set(n);
            }
        });
        // Safety net: if the install finished while this screen missed the
        // result, the files on disk still say so.
        use_future(move || async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if *page.peek() == Page::Installing && failed.peek().is_none() {
                    let st = svc().status();
                    if st.installed && st.operation.is_none() && st.state.installed_at.is_some() {
                        let mut page = page;
                        page.set(Page::Done);
                    }
                }
            }
        });
        let w = Wiz {
            ui,
            page,
            chosen,
            picked,
            failed,
        };
        let content: Element = match *page.read() {
            Page::Welcome => welcome(w).into_element(),
            Page::Proton => proton(w).into_element(),
            Page::Account => account(w).into_element(),
            Page::Source => source(w, *hover.read()).into_element(),
            Page::Installing => installing(w, *phrase.read()).into_element(),
            Page::Done => done(w).into_element(),
        };
        rect()
            .expanded()
            .padding(space::XXL)
            .content(Content::Flex)
            .on_global_file_hover(move |_| {
                if *page.peek() == Page::Source {
                    hover.set(true)
                }
            })
            .on_global_file_hover_cancelled(move |_| hover.set(false))
            .on_file_drop(move |e: Event<FileEventData>| {
                hover.set(false);
                if *page.peek() != Page::Source {
                    return;
                }
                match e.file_paths.iter().find(|p| is_exe(p)) {
                    Some(p) => start(w, Some(p.clone())),
                    None => failed.set(Some(t("wiz.source.not_exe").into())),
                }
            })
            .child(app::brand())
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.))
                    .center()
                    .child(
                        rect()
                            .width(Size::px(600.))
                            .cross_align(Alignment::Center)
                            .spacing(space::L)
                            .padding((space::XXL + 8., space::XXXL))
                            .corner_radius(28.)
                            .background(alpha(T.bg, 0.82))
                            .backdrop_blur(40.)
                            .border(
                                Border::new()
                                    .fill(if *hover.read() {
                                        alpha(T.accent, 1.0)
                                    } else {
                                        alpha(T.text, 0.1)
                                    })
                                    .width(if *hover.read() { 2. } else { 1. }),
                            )
                            .shadow(Shadow::new().y(24.).blur(64.).color(alpha((0, 0, 0), 0.45)))
                            .child(content),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .center()
                    .child(dots(*page.read())),
            )
    }
}

#[cfg(test)]
thread_local! {
    /// Lets the snapshot test open a given page.
    pub static TEST_PAGE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

fn first_page() -> Page {
    #[cfg(test)]
    {
        return match TEST_PAGE.with(|p| p.get()) {
            1 => Page::Proton,
            2 => Page::Account,
            3 => Page::Source,
            4 => Page::Installing,
            5 => Page::Done,
            _ => Page::Welcome,
        };
    }
    #[allow(unreachable_code)]
    Page::Welcome
}

#[derive(Clone, Copy, PartialEq)]
struct Wiz {
    ui: Ui,
    page: State<Page>,
    chosen: State<String>,
    picked: State<Option<PathBuf>>,
    failed: State<Option<String>>,
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

fn is_exe(p: &std::path::Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

fn centered(text: Label) -> Label {
    text.text_align(TextAlign::Center).width(Size::fill())
}

fn dots(current: Page) -> impl IntoElement {
    let pages = [
        Page::Welcome,
        Page::Proton,
        Page::Account,
        Page::Source,
        Page::Installing,
        Page::Done,
    ];
    let idx = pages.iter().position(|p| *p == current).unwrap_or(0);
    rect().horizontal().spacing(6.).children(
        (0..pages.len())
            .map(|i| {
                rect()
                    .key(i)
                    .width(Size::px(if i == idx { 20. } else { 6. }))
                    .height(Size::px(6.))
                    .corner_radius(3.)
                    .background(if i == idx {
                        alpha(T.text, 0.9)
                    } else {
                        alpha(T.text, 0.25)
                    })
                    .into_element()
            })
            .collect::<Vec<_>>(),
    )
}

fn actions(children: Vec<Element>) -> Rect {
    rect()
        .horizontal()
        .spacing(space::M)
        .cross_align(Alignment::Center)
        .padding((space::S, 0., 0., 0.))
        .children(children)
}

fn primary(text: &str) -> Button {
    pill(text).corner_radius(RADIUS_PILL).padding((12., 36.))
}

fn welcome(mut w: Wiz) -> impl IntoElement {
    let sys = svc().system();
    let mut problems: Vec<String> = Vec::new();
    match sys.python {
        Some(p) if !p.supported() => {
            problems.push(format!("Python {p}: {}", t("wiz.sys.python_old")))
        }
        None => problems.push(t("wiz.sys.python_missing").into()),
        _ => {}
    }
    if sys.vulkan_icds.is_empty() {
        problems.push(t("wiz.sys.vulkan").into());
    }
    let blocking = !problems.is_empty();
    rect()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .spacing(space::L)
        .child(app::logo(112.))
        .child(centered(display(t("wiz.welcome.title"))))
        .child(centered(lead(t("wiz.welcome.body"))))
        .children(
            problems
                .into_iter()
                .enumerate()
                .map(|(i, p)| {
                    rect()
                        .key(i)
                        .horizontal()
                        .spacing(space::S)
                        .cross_align(Alignment::Center)
                        .child(dot(Tone::Error))
                        .child(body(p))
                        .into_element()
                })
                .collect::<Vec<_>>(),
        )
        .child(actions(vec![
            primary(t("wiz.start"))
                .enabled(!blocking)
                .on_press(move |_| w.page.set(Page::Proton))
                .into_element(),
        ]))
}

fn proton(mut w: Wiz) -> impl IntoElement {
    let cat = svc().catalog_now();
    let recommended = cat.recommended_proton.clone();
    let mut options: Vec<String> = vec![recommended.clone()];
    for v in cat.verified_protons() {
        if !options.contains(&v) {
            options.push(v);
        }
    }
    let current = w.chosen.read().clone();
    rect()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .spacing(space::L)
        .child(centered(title(t("wiz.proton.title"))))
        .child(centered(lead(t("wiz.proton.body"))))
        .child(
            rect().width(Size::fill()).spacing(space::S).children(
                options
                    .into_iter()
                    .map(|id| {
                        let selected = id == current;
                        let (name, provider) = proton_label(&id);
                        let id2 = id.clone();
                        let mut chosen = w.chosen;
                        Button::new()
                            .key(id.clone())
                            .expanded()
                            .flat()
                            .background(if selected {
                                alpha(T.accent, 0.12)
                            } else {
                                alpha(T.text, 0.04)
                            })
                            .border_fill(if selected { T.accent } else { T.border })
                            .on_press(move |_| chosen.set(id2.clone()))
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .horizontal()
                                    .content(Content::Flex)
                                    .cross_align(Alignment::Center)
                                    .spacing(space::M)
                                    .child(
                                        rect()
                                            .width(Size::px(16.))
                                            .height(Size::px(16.))
                                            .corner_radius(8.)
                                            .border(
                                                Border::new()
                                                    .fill(if selected { T.accent } else { T.muted })
                                                    .width(2.),
                                            )
                                            .center()
                                            .maybe_child(selected.then(|| {
                                                rect()
                                                    .width(Size::px(8.))
                                                    .height(Size::px(8.))
                                                    .corner_radius(4.)
                                                    .background(T.accent)
                                            })),
                                    )
                                    .child(
                                        rect()
                                            .width(Size::flex(1.))
                                            .spacing(2.)
                                            .child(body(name))
                                            .child(muted(format!(
                                                "{provider}, {}",
                                                t("prefs.patch.verified")
                                            ))),
                                    )
                                    .maybe_child(
                                        (id == recommended).then(|| {
                                            chip(t("wiz.proton.recommended"), Tone::Accent)
                                        }),
                                    ),
                            )
                            .into_element()
                    })
                    .collect::<Vec<_>>(),
            ),
        )
        .child(centered(muted(t("wiz.proton.more"))))
        .child(actions(vec![
            link(t("wiz.back"))
                .on_press(move |_| w.page.set(Page::Welcome))
                .into_element(),
            primary(t("wiz.next"))
                .on_press(move |_| w.page.set(Page::Account))
                .into_element(),
        ]))
}

/// Save the Proton choice and run the whole install in the background.
fn start(mut w: Wiz, exe: Option<PathBuf>) {
    w.picked.set(exe.clone());
    w.failed.set(None);
    w.page.set(Page::Installing);
    let mut cfg = svc().config();
    cfg.proton = Some(w.chosen.peek().clone());
    if let Err(e) = svc().set_config(cfg) {
        w.ui.notice(Severity::Error, e.to_string());
    }
    let opts = InstallOptions {
        somnium_installer: exe,
        local_bundle: None,
        unilux_exe: cli_path(),
        ..Default::default()
    };
    // Window-level task: the button that started it unmounts right away,
    // and a component task would be cancelled with it.
    spawn_in_window(async move {
        let r = tokio::spawn(async move { svc().install(opts).await }).await;
        let mut ui = w.ui;
        ui.status.set(svc().status());
        ui.installed.set(svc().proton_installed());
        let mut page = w.page;
        let mut failed = w.failed;
        match r {
            Ok(Ok(_)) => page.set(Page::Done),
            Ok(Err(e)) => failed.set(Some(e.to_string())),
            Err(e) => failed.set(Some(e.to_string())),
        }
    });
}

fn language_name() -> &'static str {
    let loc = svc()
        .config()
        .locale
        .or_else(system_locale)
        .unwrap_or_default();
    match loc.get(..2) {
        Some("fr") => "Français",
        Some("de") => "Deutsch",
        Some("es") => "Español",
        Some("ru") => "Русский",
        Some("ja") => "日本語",
        _ => "English",
    }
}

fn system_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
}

fn source(mut w: Wiz, hover: bool) -> impl IntoElement {
    let mut failed = w.failed;
    let error = failed.read().clone();
    let browse = move |_| {
        spawn_in_window(async move {
            let picked = tokio::spawn(async {
                rfd::AsyncFileDialog::new()
                    .set_title("SomniumSpaceSetup.exe")
                    .add_filter("Windows installer", &["exe", "EXE"])
                    .pick_file()
                    .await
                    .map(|h| h.path().to_path_buf())
            })
            .await
            .ok()
            .flatten();
            if let Some(p) = picked {
                if is_exe(&p) {
                    start(w, Some(p));
                } else {
                    failed.set(Some(t("wiz.source.not_exe").into()));
                }
            }
        });
    };
    if hover {
        return rect()
            .width(Size::fill())
            .cross_align(Alignment::Center)
            .spacing(space::M)
            .padding((space::XXL, 0.))
            .child(centered(title(t("wiz.source.drop_now")).color(T.accent)))
            .child(centered(body(t("wiz.source.drop_hint")).color(T.text)));
    }
    rect()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .spacing(space::L)
        .child(centered(title(t("wiz.source.title"))))
        .child(centered(lead(format!(
            "{} {}.",
            t("wiz.source.body"),
            language_name()
        ))))
        .child(actions(vec![
            primary(t("wiz.source.download"))
                .on_press(move |_| start(w, None))
                .into_element(),
        ]))
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(1.))
                .background(alpha(T.text, 0.1)),
        )
        .child(
            rect()
                .width(Size::fill())
                .horizontal()
                .content(Content::Flex)
                .cross_align(Alignment::Center)
                .spacing(space::M)
                .child(
                    rect()
                        .width(Size::flex(1.))
                        .spacing(2.)
                        .child(body(t("wiz.source.have")))
                        .child(muted(t("wiz.source.drop")).color(alpha(T.text, 0.7))),
                )
                .child(
                    Button::new()
                        .corner_radius(RADIUS_PILL)
                        .on_press(browse)
                        .child(t("wiz.source.browse")),
                ),
        )
        .maybe_child(error.map(|e| centered(body(e).color(T.error))))
        .child(actions(vec![
            link(t("wiz.back"))
                .on_press(move |_| w.page.set(Page::Account))
                .into_element(),
        ]))
}

/// Optional: sign in to Somnium Space for the events on the main screen.
fn account(mut w: Wiz) -> impl IntoElement {
    let signed_in = w.ui.account.read().is_some();
    let next: Element = if signed_in {
        primary(t("wiz.next"))
            .on_press(move |_| w.page.set(Page::Source))
            .into_element()
    } else {
        Button::new()
            .corner_radius(RADIUS_PILL)
            .padding((12., 28.))
            .on_press(move |_| w.page.set(Page::Source))
            .child(crate::i18n::tr("Skip", "Passer"))
            .into_element()
    };
    rect()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .spacing(space::L)
        .child(centered(title(crate::i18n::tr(
            "Your Somnium Space account",
            "Votre compte Somnium Space",
        ))))
        .child(centered(lead(crate::i18n::tr(
            "Sign in to see upcoming events on the main screen. You can do it later in Settings.",
            "Connectez-vous pour voir les prochains événements sur l'écran principal. Vous pourrez le faire plus tard dans les Réglages.",
        ))))
        .child(crate::events::AccountForm(w.ui))
        .child(actions(vec![
            link(t("wiz.back"))
                .on_press(move |_| w.page.set(Page::Proton))
                .into_element(),
            next,
        ]))
}

/// Secondary copy on the sheet: body size, bright enough to read.
fn lead(text: impl Into<String>) -> Label {
    body(text).color(alpha(T.text, 0.78))
}

fn installing(mut w: Wiz, phrase: usize) -> impl IntoElement {
    let ui = w.ui;
    let done = ui.done_steps.read().clone();
    let current = ui.step.read().clone();
    let progress = ui.progress.read().clone();
    let error = w.failed.read().clone();
    let finished = done.iter().filter(|d| STEPS.contains(&d.as_str())).count();
    let sub = progress
        .as_ref()
        .and_then(|(_, d, tot)| percent(*d, *tot))
        .unwrap_or(0.)
        / 100.;
    let overall = ((finished as f32 + sub.min(0.95)) / STEPS.len() as f32 * 100.).min(100.);
    let index = current
        .as_deref()
        .and_then(|c| STEPS.iter().position(|s| *s == c))
        .map(|i| i + 1)
        .unwrap_or(finished.max(1));
    let heading = current
        .as_deref()
        .map(step)
        .unwrap_or_else(|| t("wiz.installing.title").to_string());
    if let Some(e) = error {
        return rect()
            .width(Size::fill())
            .cross_align(Alignment::Center)
            .spacing(space::L)
            .child(centered(title(t("wiz.failed.title"))))
            .child(centered(body(e).color(T.error)))
            .child(centered(muted(t("wiz.failed.hint"))))
            .child(actions(vec![
                link(t("wiz.back"))
                    .on_press(move |_| {
                        w.failed.set(None);
                        w.page.set(Page::Source)
                    })
                    .into_element(),
                primary(t("wiz.failed.retry"))
                    .on_press(move |_| {
                        let exe = w.picked.peek().clone();
                        start(w, exe)
                    })
                    .into_element(),
            ]));
    }
    let detail = progress
        .as_ref()
        .map(|(task, d, tot)| match percent(*d, *tot) {
            Some(p) => format!("{task}  {p:.0} %"),
            None => format!("{task}  {:.0} MB", *d as f64 / 1e6),
        });
    rect()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .spacing(space::L)
        .child(crate::loader::OrbitLoader)
        .child(centered(title(heading)))
        .child(centered(lead(t(&format!("load.{}", phrase % PHRASES)))))
        .child(
            rect()
                .width(Size::px(420.))
                .spacing(space::S)
                .child(
                    ProgressBar::new(overall)
                        .show_progress(false)
                        .width(Size::fill()),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .horizontal()
                        .main_align(Alignment::SpaceBetween)
                        .child(muted(detail.unwrap_or_default()))
                        .child(muted(format!(
                            "{} {index} / {}",
                            t("wiz.installing.step"),
                            STEPS.len()
                        ))),
                ),
        )
        .child(centered(muted(t("wiz.installing.hint"))))
}

fn done(mut w: Wiz) -> impl IntoElement {
    rect()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .spacing(space::L)
        .child(app::logo(112.))
        .child(centered(display(t("wiz.done.title"))))
        .child(centered(lead(t("wiz.done.body"))))
        .child(actions(vec![
            primary(t("wiz.done.go"))
                .on_press(move |_| w.ui.screen.set(Screen::Launcher))
                .into_element(),
        ]))
}

/// The desktop entry should start the `unilux` CLI that sits next to us.
pub fn cli_path() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let cli = me.with_file_name("unilux");
    Some(if cli.exists() { cli } else { me })
}
