//! The setup tunnel: pick a Proton, pick the fixes that go on it, then
//! build and install the whole thing. One idea per page, one way forward.

use aasl_components::ProtonId;
use aasl_patches::recipe::{DEFAULT_RECIPE, Ingredient, Kind};
use aasl_service::{PatchSource, Recipe};
use freya::prelude::*;

use crate::i18n::{fix_summary, fix_title, step, t};
use crate::loader::OrbitLoader;
use crate::state::Ui;
use crate::svc;
use crate::theme::{RADIUS_PANEL, RADIUS_PILL, T, alpha, size, space};
use crate::widgets::{Tone, body, chip, group, muted, percent, row, title};

#[derive(Clone, Copy, PartialEq)]
pub enum Step {
    Proton,
    Fixes,
    Build,
}

impl Step {
    fn index(self) -> usize {
        match self {
            Step::Proton => 0,
            Step::Fixes => 1,
            Step::Build => 2,
        }
    }
}

/// What the tunnel does with the recipe it started from.
#[derive(Clone, Copy, PartialEq)]
pub enum Intent {
    /// Use it as it is; a changed selection is saved as a new recipe.
    Use,
    /// Copy a recipe into a new one of the user's.
    Fork,
    /// A recipe from scratch, starting from the one in use.
    New,
    /// Change one of the user's recipes in place.
    Edit,
}

#[derive(Clone, PartialEq)]
pub enum Phase {
    Review,
    Running,
    Done,
    Failed(String),
}

/// Everything the tunnel has decided so far.
#[derive(Clone, PartialEq)]
pub struct Flow {
    pub step: Step,
    pub proton: Option<ProtonId>,
    /// The recipe the fixes started from.
    pub base: String,
    pub ingredients: Vec<String>,
    pub intent: Intent,
    pub phase: Phase,
}

impl Flow {
    pub fn start(base: Option<&str>, step: Step, intent: Intent) -> Self {
        let id = base
            .map(str::to_string)
            .or_else(|| svc().active_plan().ok().map(|p| p.recipe.id))
            .unwrap_or_else(|| DEFAULT_RECIPE.into());
        let ingredients = svc().recipe(&id).map(|r| r.ingredients).unwrap_or_default();
        Self {
            step,
            proton: svc().current_proton(),
            base: id,
            ingredients,
            intent,
            phase: Phase::Review,
        }
    }

    /// The name a fork starts with.
    fn suggested_name(&self) -> String {
        match self.intent {
            Intent::Fork => svc()
                .recipe(&self.base)
                .map(|r| {
                    let name = if r.id == DEFAULT_RECIPE {
                        crate::i18n::recipe_title(&r.id, &r.title)
                    } else {
                        r.title
                    };
                    format!("{name} ({})", t("recipes.fork.suffix"))
                })
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// The fixes (or the Proton pin) differ from the recipe it started from,
    /// so applying saves a new recipe instead of switching to the old one.
    fn modified(&self, base: &Recipe) -> bool {
        match self.intent {
            Intent::Fork | Intent::New | Intent::Edit => return true,
            Intent::Use => {}
        }
        let chosen = self.proton.as_ref().map(|p| p.to_string());
        self.ingredients != base.ingredients
            || base
                .proton
                .as_deref()
                .is_some_and(|p| !p.ends_with(":latest") && Some(p) != chosen.as_deref())
    }

    /// The recipe as it would be saved (or used), for checks and previews.
    fn effective(&self, base: &Recipe, title: &str) -> Recipe {
        let mut r = base.clone();
        if self.modified(base) {
            if self.intent != Intent::Edit {
                r.id = slug(title);
                r.title = title.to_string();
            }
            r.ingredients = self.ingredients.clone();
            r.proton = self.proton.as_ref().map(|p| p.to_string());
        }
        r
    }
}

#[cfg(test)]
thread_local! {
    /// Lets the snapshot test open the tunnel on a page (0 to 2).
    pub static TEST_STEP: std::cell::Cell<Option<u8>> = const { std::cell::Cell::new(None) };
}

pub fn first_flow() -> Option<Flow> {
    #[cfg(test)]
    {
        return TEST_STEP.with(|s| s.get()).map(|n| {
            Flow::start(
                None,
                match n {
                    0 => Step::Proton,
                    1 => Step::Fixes,
                    _ => Step::Build,
                },
                Intent::Use,
            )
        });
    }
    #[allow(unreachable_code)]
    None
}

fn edit(mut flow: State<Option<Flow>>, f: impl FnOnce(&mut Flow)) {
    if let Some(x) = flow.write().as_mut() {
        f(x)
    }
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "recipe".into()
    } else {
        out.into()
    }
}

fn recipe_title(name: &str) -> String {
    let n = name.trim();
    if n.is_empty() {
        t("setup.mine").to_string()
    } else {
        n.to_string()
    }
}

/// Free id for a new recipe called `title`.
fn free_id(title: &str) -> String {
    let base = slug(title);
    let mut id = base.clone();
    let mut n = 2;
    while svc().recipe(&id).is_ok() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

fn proton_name(id: &ProtonId) -> (String, String) {
    let provider = aasl_components::proton::provider(&id.provider)
        .map(|p| p.title.to_string())
        .unwrap_or_else(|| id.provider.clone());
    (id.tag.clone(), provider)
}

fn check_icon() -> impl IntoElement {
    const SVG: &str = r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="white" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>"#;
    SvgViewer::new(SVG.as_bytes())
        .stroke(T.accent_text)
        .width(Size::px(14.))
        .height(Size::px(14.))
}

fn stepper(current: usize, all_done: bool) -> Element {
    const KEYS: [&str; 3] = ["setup.step.proton", "setup.step.fixes", "setup.step.build"];
    let mut line = rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(space::M);
    for (i, key) in KEYS.iter().enumerate() {
        let done = all_done || i < current;
        let active = i == current && !all_done;
        let (bg, ring) = if done {
            (alpha(T.accent, 1.0), alpha(T.accent, 1.0))
        } else if active {
            (alpha(T.accent, 0.16), alpha(T.accent, 1.0))
        } else {
            (alpha(T.raised, 1.0), alpha(T.border, 1.0))
        };
        let mark: Element = if done {
            check_icon().into_element()
        } else {
            label()
                .text((i + 1).to_string())
                .font_size(size::SMALL)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(if active { T.text } else { T.muted })
                .into_element()
        };
        line = line.child(
            rect()
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(space::S)
                .child(
                    rect()
                        .width(Size::px(28.))
                        .height(Size::px(28.))
                        .corner_radius(14.)
                        .background(bg)
                        .border(Border::new().fill(ring).width(1.))
                        .center()
                        .child(mark),
                )
                .child(
                    label()
                        .text(t(key))
                        .color(if done || active { T.text } else { T.muted })
                        .font_weight(if active {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::NORMAL
                        }),
                ),
        );
        if i + 1 < KEYS.len() {
            line = line.child(
                rect()
                    .width(Size::flex(1.))
                    .height(Size::px(2.))
                    .corner_radius(1.)
                    .background(if done {
                        alpha(T.accent, 0.7)
                    } else {
                        alpha(T.border, 1.0)
                    }),
            );
        }
    }
    line.into_element()
}

fn header(title_key: &str, lead_key: &str) -> Rect {
    rect()
        .width(Size::fill())
        .spacing(space::XS)
        .child(title(t(title_key)))
        .child(muted(t(lead_key)))
}

/// Navigation under a page: a hint on the left, the buttons on the right.
fn footer(hint: Option<Element>, buttons: Vec<Element>) -> Element {
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(space::S)
        .padding((space::M, 0., 0., 0.))
        .child(rect().width(Size::flex(1.)).maybe_child(hint))
        .children(buttons)
        .into_element()
}

fn primary(text: &str, enabled: bool, on: impl FnMut(Event<PressEventData>) + 'static) -> Element {
    Button::new()
        .filled()
        .enabled(enabled)
        .on_press(on)
        .child(
            label()
                .text(text.to_string())
                .font_weight(FontWeight::SEMI_BOLD)
                .color(T.accent_text),
        )
        .into_element()
}

fn secondary(text: &str, on: impl FnMut(Event<PressEventData>) + 'static) -> Element {
    Button::new()
        .flat()
        .on_press(on)
        .child(text.to_string())
        .into_element()
}

#[derive(PartialEq)]
pub struct SetupFlow {
    pub ui: Ui,
    pub flow: State<Option<Flow>>,
}

impl Component for SetupFlow {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let flow = self.flow;
        let name = use_state(|| {
            flow.peek()
                .as_ref()
                .map(|f| f.suggested_name())
                .unwrap_or_default()
        });
        let Some(f) = flow.read().clone() else {
            return rect().into_element();
        };
        let page: Element = match f.step {
            Step::Proton => ProtonStep { ui, flow }.into_element(),
            Step::Fixes => FixesStep { flow, name }.into_element(),
            Step::Build => BuildStep { ui, flow, name }.into_element(),
        };
        rect()
            .width(Size::fill())
            .spacing(space::XL)
            .child(stepper(f.step.index(), f.phase == Phase::Done))
            .child(page)
            .into_element()
    }
}

// ---- 1. Proton ---------------------------------------------------------

#[derive(PartialEq)]
struct ProtonStep {
    ui: Ui,
    flow: State<Option<Flow>>,
}

fn pick_cell(flow: State<Option<Flow>>, id: ProtonId, selected: bool) -> Element {
    if selected {
        return chip(t("setup.chosen"), Tone::Accent).into_element();
    }
    Button::new()
        .compact()
        .on_press(move |_| edit(flow, |f| f.proton = Some(id.clone())))
        .child(t("setup.choose"))
        .into_element()
}

impl Component for ProtonStep {
    fn render(&self) -> impl IntoElement {
        let mut ui = self.ui;
        let flow = self.flow;
        let mut show_all = use_state(|| false);
        use_hook(move || {
            ui.releases.set(Vec::new());
            ui.load_releases(false)
        });
        let busy = ui.status.read().operation.is_some();
        let progress = ui.progress.read().clone();
        let chosen = flow.read().as_ref().and_then(|f| f.proton.clone());
        let recommended = svc().catalog_now().recommended_proton.clone();
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
                let (tag, provider) = proton_name(&id);
                let selected = chosen.as_ref() == Some(&id);
                let removable = !p.in_use && !selected;
                let id_remove = id.clone();
                let trailing = rect()
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(space::S)
                    .maybe_child(
                        (id.to_string() == recommended)
                            .then(|| chip(t("setup.recommended"), Tone::Ok)),
                    )
                    .maybe_child(
                        p.in_use
                            .then(|| chip(t("prefs.proton.in_use"), Tone::Neutral)),
                    )
                    .maybe_child(removable.then(|| {
                        Button::new()
                            .compact()
                            .flat()
                            .enabled(!busy)
                            .on_press(move |_| {
                                if let Err(e) = svc().proton_remove(&id_remove) {
                                    ui.error.set(Some(e.to_string()));
                                }
                                reload(ui);
                            })
                            .child(t("prefs.proton.remove"))
                    }))
                    .child(pick_cell(flow, id, selected))
                    .into_element();
                row(tag, Some(provider), Some(trailing))
            })
            .collect();

        let provider = *ui.provider.read();
        let releases = ui.releases.read().clone();
        let total = releases.len();
        let limit = if *show_all.read() { total } else { 8 };
        let available: Vec<Element> = releases
            .into_iter()
            .take(limit)
            .map(|r| {
                let id = r.id.clone();
                let id_dl = id.clone();
                let date = r
                    .published_at
                    .as_deref()
                    .and_then(|d| d.get(..10))
                    .unwrap_or("")
                    .to_string();
                let mut sub = date;
                if r.size > 0 {
                    if !sub.is_empty() {
                        sub.push_str(", ");
                    }
                    sub.push_str(&format!("{:.0} MB", r.size as f64 / 1e6));
                }
                if r.prerelease {
                    sub.push_str(&format!(", {}", t("prefs.proton.pre")));
                }
                let downloading = progress.as_ref().filter(|(task, _, _)| *task == r.id.tag);
                let selected = chosen.as_ref() == Some(&id);
                let usable = r.variant.is_some();
                let trailing = rect()
                    .horizontal()
                    .spacing(space::S)
                    .cross_align(Alignment::Center)
                    .maybe_child(
                        (id.to_string() == recommended)
                            .then(|| chip(t("setup.recommended"), Tone::Ok)),
                    )
                    .maybe_child(downloading.map(|(_, d, tot)| {
                        rect().width(Size::px(120.)).child(
                            ProgressBar::new(percent(*d, *tot).unwrap_or(0.))
                                .show_progress(false)
                                .width(Size::fill()),
                        )
                    }))
                    .maybe_child(
                        (r.installed && downloading.is_none())
                            .then(|| chip(t("setup.downloaded"), Tone::Neutral)),
                    )
                    .maybe_child((!r.installed && usable && downloading.is_none()).then(|| {
                        Button::new()
                            .compact()
                            .flat()
                            .enabled(!busy)
                            .on_press(move |_| {
                                let id = id_dl.clone();
                                ui.run(async move { svc().proton_install(id).await }, move |_| {
                                    reload(ui)
                                })
                            })
                            .child(t("prefs.proton.download"))
                    }))
                    .maybe_child(usable.then(|| pick_cell(flow, id.clone(), selected)))
                    .into_element();
                row(r.id.tag.clone(), Some(sub), Some(trailing))
            })
            .collect();

        let desc = aasl_components::proton::provider(provider)
            .map(|p| p.description)
            .unwrap_or("");
        let pills = rect()
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
            );

        let hint = chosen.as_ref().map(|id| {
            let (tag, _) = proton_name(id);
            rect()
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(space::S)
                .child(muted(t("setup.picked")))
                .child(body(tag).font_weight(FontWeight::SEMI_BOLD))
                .into_element()
        });

        rect()
            .width(Size::fill())
            .spacing(space::XL)
            .child(header("setup.proton.title", "setup.proton.lead"))
            .maybe_child(
                (!installed.is_empty()).then(|| group(t("prefs.proton.installed"), installed)),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(space::M)
                    .child(pills)
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
            .child(footer(
                hint,
                vec![
                    secondary(t("recipes.cancel"), move |_| {
                        let mut flow = flow;
                        flow.set(None)
                    }),
                    primary(t("setup.next"), chosen.is_some(), move |_| {
                        edit(flow, |f| f.step = Step::Fixes)
                    }),
                ],
            ))
    }
}

// ---- 2. Fixes ----------------------------------------------------------

#[derive(PartialEq)]
struct FixesStep {
    flow: State<Option<Flow>>,
    name: State<String>,
}

fn kind_chip(ing: &Ingredient) -> Rect {
    match ing.kind {
        Kind::Patch => chip(t("recipes.kind.patch"), Tone::Accent),
        Kind::Env => chip(t("recipes.kind.env"), Tone::Neutral),
    }
}

impl Component for FixesStep {
    fn render(&self) -> impl IntoElement {
        let flow = self.flow;
        let name = self.name;
        let Some(f) = flow.read().clone() else {
            return rect().into_element();
        };
        let lib = svc().library().ok();
        let base = svc().recipe(&f.base).ok();
        let modified = base.as_ref().is_some_and(|b| f.modified(b));
        let title_now = recipe_title(&name.read());

        let starts: Vec<Element> = svc()
            .recipes()
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let selected = r.id == f.base;
                let id = r.id.clone();
                Button::new()
                    .key(r.id.clone())
                    .compact()
                    .corner_radius(RADIUS_PILL)
                    .maybe(selected, |b| b.filled())
                    .maybe(!selected, |b| b.flat())
                    .on_press(move |_| {
                        if let Ok(rec) = svc().recipe(&id) {
                            edit(flow, |f| {
                                f.base = id.clone();
                                f.ingredients = rec.ingredients.clone();
                            })
                        }
                    })
                    .child(
                        label()
                            .text(if r.builtin {
                                crate::i18n::recipe_title(&r.id, &r.title)
                            } else {
                                r.title.clone()
                            })
                            .color(if selected { T.accent_text } else { T.text }),
                    )
                    .into_element()
            })
            .collect();

        let mut all: Vec<Ingredient> = lib
            .as_ref()
            .map(|l| l.ingredients.clone())
            .unwrap_or_default();
        if let Some(b) = &base {
            for c in &b.custom {
                if !all.iter().any(|i| i.id == c.id) {
                    all.push(c.clone());
                }
            }
        }
        let rows: Vec<Element> = all
            .into_iter()
            .map(|ing| {
                let on = f.ingredients.contains(&ing.id);
                let is_custom = base
                    .as_ref()
                    .is_some_and(|b| b.custom.iter().any(|c| c.id == ing.id));
                let key = ing.id.clone();
                let sub = fix_summary(
                    &ing.id,
                    &if ing.summary.is_empty() {
                        ing.id.clone()
                    } else {
                        ing.summary.clone()
                    },
                );
                Button::new()
                    .key(ing.id.clone())
                    .flat()
                    .expanded()
                    .on_press(move |_| {
                        edit(flow, |f| {
                            if let Some(p) = f.ingredients.iter().position(|x| *x == key) {
                                f.ingredients.remove(p);
                            } else {
                                f.ingredients.push(key.clone());
                            }
                        })
                    })
                    .child(
                        rect()
                            .width(Size::fill())
                            .horizontal()
                            .content(Content::Flex)
                            .cross_align(Alignment::Center)
                            .spacing(space::M)
                            .child(Checkbox::new().selected(on))
                            .child(
                                rect()
                                    .width(Size::flex(1.))
                                    .spacing(2.)
                                    .child(body(fix_title(&ing.id, &ing.title)))
                                    .child(muted(sub)),
                            )
                            .maybe_child(is_custom.then(|| chip(t("recipes.custom"), Tone::Warn)))
                            .child(kind_chip(&ing)),
                    )
                    .into_element()
            })
            .collect();

        let check = match (&lib, &base) {
            (Some(l), Some(b)) => Some(l.plan(&f.effective(b, &title_now))),
            _ => None,
        };
        let (check_text, check_tone, ok) = match &check {
            Some(Ok(p)) if p.needs_build() => {
                (t("recipes.check.build").to_string(), Tone::Accent, true)
            }
            Some(Ok(_)) => (t("recipes.check.env").to_string(), Tone::Ok, true),
            Some(Err(e)) => (e.to_string(), Tone::Error, false),
            None => ("?".into(), Tone::Error, false),
        };
        let custom = check
            .as_ref()
            .and_then(|c| c.as_ref().ok())
            .is_some_and(|p| p.has_custom_code());

        let proton_line = f.proton.as_ref().map(|id| {
            let (tag, _) = proton_name(id);
            format!("{} {tag}", t("setup.on"))
        });

        rect()
            .width(Size::fill())
            .spacing(space::XL)
            .child(
                header("setup.fixes.title", "setup.fixes.lead").maybe_child(proton_line.map(muted)),
            )
            .maybe_child((f.intent == Intent::Use).then(|| {
                rect()
                    .width(Size::fill())
                    .spacing(space::S)
                    .child(muted(t("setup.start_from")))
                    .child(
                        rect()
                            .width(Size::fill())
                            .horizontal()
                            .spacing(space::XS)
                            .children(starts),
                    )
            }))
            .child(group(t("recipes.ingredients"), rows))
            .maybe_child((modified && f.intent != Intent::Edit).then(|| {
                rect()
                    .width(Size::fill())
                    .spacing(space::XS)
                    .child(body(t("setup.name")).font_weight(FontWeight::SEMI_BOLD))
                    .child(muted(t("setup.name.sub")))
                    .child(
                        Input::new(name)
                            .placeholder(t("setup.mine"))
                            .width(Size::fill()),
                    )
            }))
            .maybe_child(custom.then(|| muted(t("recipes.custom.warn"))))
            .child(footer(
                Some(chip(check_text, check_tone).into_element()),
                vec![
                    secondary(t("setup.back"), move |_| {
                        edit(flow, |f| f.step = Step::Proton)
                    }),
                    primary(t("setup.next"), ok, move |_| {
                        edit(flow, |f| {
                            f.step = Step::Build;
                            f.phase = Phase::Review;
                        })
                    }),
                ],
            ))
            .into_element()
    }
}

// ---- 3. Build ----------------------------------------------------------

#[derive(PartialEq)]
struct BuildStep {
    ui: Ui,
    flow: State<Option<Flow>>,
    name: State<String>,
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

/// Save the recipe if it is new, select it, then move Somnium to the Proton.
/// Downloading, building and patching all happen inside `proton_use`.
async fn run(f: Flow, proton: ProtonId, name: String) -> Result<(), String> {
    let s = svc();
    let base = s.recipe(&f.base).map_err(|e| e.to_string())?;
    let target = if f.modified(&base) {
        let mut r = base.clone();
        if f.intent != Intent::Edit {
            let title = recipe_title(&name);
            r.id = free_id(&title);
            r.title = title;
        }
        r.ingredients = f.ingredients.clone();
        r.proton = Some(proton.to_string());
        s.recipe_save(&r).map_err(|e| e.to_string())?.id
    } else {
        base.id.clone()
    };
    let previous = s.config().recipe;
    let mut cfg = s.config();
    cfg.recipe = (target != DEFAULT_RECIPE).then(|| target.clone());
    s.set_config(cfg).map_err(|e| e.to_string())?;
    if let Err(e) = s.proton_use(proton, false).await {
        let mut cfg = s.config();
        cfg.recipe = previous;
        let _ = s.set_config(cfg);
        return Err(e.to_string());
    }
    Ok(())
}

fn apply(mut ui: Ui, flow: State<Option<Flow>>, name: String) {
    let Some(f) = flow.peek().clone() else { return };
    let Some(proton) = f.proton.clone() else {
        return;
    };
    edit(flow, |x| x.phase = Phase::Running);
    ui.error.set(None);
    spawn_in_window(async move {
        let res = match tokio::spawn(run(f, proton, name)).await {
            Ok(r) => r,
            Err(e) => Err(e.to_string()),
        };
        edit(flow, |x| {
            x.phase = match res {
                Ok(()) => Phase::Done,
                Err(e) => Phase::Failed(e),
            }
        });
        ui.status.set(svc().status());
        ui.installed.set(svc().proton_installed());
    });
}

impl Component for BuildStep {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let mut flow = self.flow;
        let name = self.name;
        let Some(f) = flow.read().clone() else {
            return rect().into_element();
        };
        let Some(proton) = f.proton.clone() else {
            return rect().into_element();
        };
        let (tag, provider) = proton_name(&proton);
        match f.phase.clone() {
            Phase::Review => {
                let can_build = svc().patch_builder().is_some();
                let base = svc().recipe(&f.base).ok();
                let title_now = recipe_title(&name.read());
                let eff = base.as_ref().map(|b| f.effective(b, &title_now));
                let plan = match (svc().library(), &eff) {
                    (Ok(l), Some(r)) => l.plan(r).ok(),
                    _ => None,
                };
                let source = plan.as_ref().map(|p| svc().bundle_source(p, &proton));
                let blocked = source == Some(PatchSource::MustBuild) && !can_build;
                let must_build = source == Some(PatchSource::MustBuild);
                let fixes = svc()
                    .library()
                    .map(|l| {
                        f.ingredients
                            .iter()
                            .map(|id| {
                                l.ingredients
                                    .iter()
                                    .find(|i| i.id == *id)
                                    .map(|i| fix_title(&i.id, &i.title))
                                    .unwrap_or_else(|| id.clone())
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let recipe_label = eff
                    .as_ref()
                    .map(|r| r.title.clone())
                    .unwrap_or_else(|| f.base.clone());
                let rows = vec![
                    row(
                        t("setup.sum.proton"),
                        Some(provider),
                        Some(body(tag).into_element()),
                    ),
                    row(
                        t("setup.sum.recipe"),
                        None,
                        Some(body(recipe_label).into_element()),
                    ),
                    row(
                        t("setup.sum.fixes"),
                        Some(if fixes.is_empty() {
                            t("stat.none").to_string()
                        } else {
                            fixes
                        }),
                        None,
                    ),
                    row(
                        t("setup.sum.patches"),
                        None,
                        source.map(|s| source_chip(s, can_build).into_element()),
                    ),
                ];
                let hint = blocked.then(|| {
                    let text = match svc().podman_install_hint() {
                        Some(cmd) => format!("{} {cmd}", t("prefs.patches.build.missing.cmd")),
                        None => t("prefs.patches.build.missing").to_string(),
                    };
                    muted(text).into_element()
                });
                let busy = ui.status.read().operation.is_some();
                rect()
                    .width(Size::fill())
                    .spacing(space::XL)
                    .child(header("setup.build.title", "setup.build.lead"))
                    .child(group(t("setup.sum"), rows))
                    .child(footer(
                        hint,
                        vec![
                            secondary(t("setup.back"), move |_| {
                                edit(flow, |f| f.step = Step::Fixes)
                            }),
                            primary(
                                if must_build {
                                    t("setup.apply.build")
                                } else {
                                    t("setup.apply")
                                },
                                !blocked && !busy && plan.is_some(),
                                move |_| apply(ui, flow, name.read().clone()),
                            ),
                        ],
                    ))
                    .into_element()
            }
            Phase::Running => {
                let step_line = ui
                    .step
                    .read()
                    .as_ref()
                    .map(|s| step(s))
                    .unwrap_or_else(|| t("setup.running.sub").to_string());
                let bar = ui
                    .progress
                    .read()
                    .as_ref()
                    .and_then(|(_, d, tot)| percent(*d, *tot));
                rect()
                    .width(Size::fill())
                    .padding((space::XXL, 0.))
                    .cross_align(Alignment::Center)
                    .spacing(space::L)
                    .child(OrbitLoader)
                    .child(title(t("setup.running")))
                    .child(muted(step_line))
                    .maybe_child(bar.map(|p| {
                        rect()
                            .width(Size::px(360.))
                            .child(ProgressBar::new(p).show_progress(true).width(Size::fill()))
                    }))
                    .into_element()
            }
            Phase::Done => rect()
                .width(Size::fill())
                .padding((space::XXL, 0.))
                .cross_align(Alignment::Center)
                .spacing(space::L)
                .child(chip(t("setup.done"), Tone::Ok))
                .child(title(tag))
                .child(muted(t("setup.done.sub")))
                .child(primary(t("setup.finish"), true, move |_| flow.set(None)))
                .into_element(),
            Phase::Failed(msg) => rect()
                .width(Size::fill())
                .spacing(space::L)
                .child(header("setup.failed", "setup.failed.lead"))
                .child(
                    rect()
                        .width(Size::fill())
                        .padding(space::L)
                        .corner_radius(RADIUS_PANEL)
                        .background(T.surface)
                        .border(Border::new().fill(alpha(T.error, 0.6)).width(1.))
                        .child(label().text(msg).font_size(size::SMALL).color(T.error)),
                )
                .child(footer(
                    None,
                    vec![
                        secondary(t("setup.back"), move |_| {
                            edit(flow, |f| f.phase = Phase::Review)
                        }),
                        primary(t("setup.retry"), true, move |_| {
                            apply(ui, flow, name.read().clone())
                        }),
                    ],
                ))
                .into_element(),
        }
    }
}
