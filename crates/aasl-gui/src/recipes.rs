//! Compatibility recipes: one place for the Proton, the fixes on top of
//! it, the evidence from play sessions and the community.

use aasl_core::Severity;
use aasl_patches::community::{Crash, Status as Standing, Verdict};
use aasl_patches::recipe::{Ingredient, Kind};
use aasl_service::{PatchSource, Recipe, RecipeInfo, SavedReport, Share};
use freya::prelude::*;

use crate::i18n::t;
use crate::state::Ui;
use crate::svc;
use crate::theme::{RADIUS_PILL, T, size, space};
use crate::widgets::{Tone, body, chip, group, muted, row, title};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Recipes,
    Create,
    Proton,
    Sessions,
    Community,
}

const TABS: &[(Tab, &str)] = &[
    (Tab::Recipes, "recipes.tab.mine"),
    (Tab::Create, "recipes.tab.create"),
    (Tab::Proton, "recipes.tab.proton"),
    (Tab::Sessions, "recipes.tab.sessions"),
    (Tab::Community, "recipes.tab.community"),
];

/// What the editor starts from.
#[derive(Clone, PartialEq, Default)]
struct Draft {
    id: String,
    title: String,
    description: String,
    proton: Option<String>,
    ingredients: Vec<String>,
    custom: Vec<Ingredient>,
    env: std::collections::BTreeMap<String, String>,
}

impl Draft {
    fn from_recipe(r: &Recipe, copy: bool) -> Self {
        Self {
            id: if copy {
                format!("{}-copy", r.id)
            } else {
                r.id.clone()
            },
            title: if copy {
                format!("{} ({})", r.title, t("recipes.copy"))
            } else {
                r.title.clone()
            },
            description: r.description.clone(),
            proton: r.proton.clone(),
            ingredients: r.ingredients.clone(),
            custom: r.custom.clone(),
            env: r.env.clone(),
        }
    }

    fn recipe(&self) -> Recipe {
        Recipe {
            format: aasl_patches::recipe::FORMAT,
            id: self.id.trim().to_string(),
            title: self.title.trim().to_string(),
            description: self.description.trim().to_string(),
            proton: self.proton.clone(),
            ingredients: self.ingredients.clone(),
            env: self.env.clone(),
            custom: self.custom.clone(),
        }
    }
}

fn tab_button(
    label_text: &'static str,
    selected: bool,
    on: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    Button::new()
        .key(label_text)
        .compact()
        .corner_radius(RADIUS_PILL)
        .maybe(selected, |b| b.filled())
        .maybe(!selected, |b| b.flat())
        .on_press(on)
        .child(
            label()
                .text(t(label_text))
                .color(if selected { T.accent_text } else { T.text }),
        )
        .into_element()
}

fn standing_chip(v: &Verdict) -> Rect {
    match v.status {
        Standing::Community => chip(t("recipes.standing.community"), Tone::Ok),
        Standing::Proposed => chip(t("recipes.standing.proposed"), Tone::Accent),
        Standing::Untested => chip(t("recipes.standing.untested"), Tone::Neutral),
    }
}

fn verdict_line(v: &Verdict) -> String {
    let pct = |x: Option<f32>| {
        x.map(|s| format!("{:.0} %", s * 100.0))
            .unwrap_or_else(|| "–".into())
    };
    format!(
        "{} {} · {:.1} h · {} {} · {} {} · {} {}",
        v.reports,
        t("recipes.sessions"),
        v.hours,
        t("recipes.crashes"),
        v.crashes_in_game,
        t("recipes.videos"),
        pct(v.video_success),
        t("recipes.pacing"),
        pct(v.pacing)
    )
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn copy(mut ui: Ui, text: String) {
    match freya::clipboard::Clipboard::set(text) {
        Ok(()) => ui.notice(Severity::Info, t("recipes.copied")),
        Err(e) => ui.error.set(Some(format!("{e:?}"))),
    }
}

#[cfg(test)]
thread_local! {
    /// Lets the snapshot test open a given tab (index into TABS).
    pub static TEST_TAB: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

fn first_tab() -> Tab {
    #[cfg(test)]
    {
        return TABS[TEST_TAB.with(|t| t.get()) as usize % TABS.len()].0;
    }
    #[allow(unreachable_code)]
    Tab::Recipes
}

#[derive(PartialEq)]
pub struct RecipesSection(pub Ui);

impl Component for RecipesSection {
    fn render(&self) -> impl IntoElement {
        let ui = self.0;
        let mut tab = use_state(first_tab);
        let draft = use_state(Draft::default);
        let current = *tab.read();
        let content: Element = match current {
            Tab::Recipes => MineTab { ui, tab, draft }.into_element(),
            Tab::Create => CreateTab { ui, tab, draft }.into_element(),
            Tab::Proton => crate::settings::ProtonSection(ui).into_element(),
            Tab::Sessions => SessionsTab(ui).into_element(),
            Tab::Community => CommunityTab(ui).into_element(),
        };
        rect()
            .spacing(space::XL)
            .child(
                rect()
                    .spacing(space::XS)
                    .child(title(t("prefs.recipes")))
                    .child(muted(t("recipes.lead"))),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .spacing(space::XS)
                    .children(
                        TABS.iter()
                            .map(|(tb, key)| {
                                let tb = *tb;
                                tab_button(key, tb == current, move |_| tab.set(tb))
                            })
                            .collect::<Vec<_>>(),
                    ),
            )
            .child(content)
    }
}

#[derive(PartialEq)]
struct MineTab {
    ui: Ui,
    tab: State<Tab>,
    draft: State<Draft>,
}

fn load_mine() -> Vec<(RecipeInfo, Option<Verdict>)> {
    svc()
        .recipes()
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let v = svc().recipe_verdict(&r.id).ok();
            (r, v)
        })
        .collect()
}

impl Component for MineTab {
    fn render(&self) -> impl IntoElement {
        let mut ui = self.ui;
        let mut tab = self.tab;
        let mut draft = self.draft;
        let mut list = use_state(load_mine);
        let mut share = use_state(|| None::<(String, Share)>);
        let st = ui.status.read().clone();
        let busy = st.operation.is_some();
        let mut reload = move || list.set(load_mine());

        // The recipe in use, its Proton and the bundle on it.
        let plan = svc().active_plan().ok();
        let proton = svc().current_proton();
        let source = match (&plan, &proton) {
            (Some(p), Some(id)) => Some(svc().bundle_source(p, id)),
            _ => None,
        };
        let can_build = svc().patch_builder().is_some();
        let bundle_chip = match &st.patches {
            Some(p) if p.bundle.is_some() && p.intact => chip(t("stat.intact"), Tone::Ok),
            Some(p) if p.bundle.is_some() => chip(t("stat.changed"), Tone::Error),
            _ if source == Some(PatchSource::NotNeeded) => {
                chip(t("prefs.patch.not_needed"), Tone::Ok)
            }
            _ => chip(t("stat.none"), Tone::Warn),
        };
        let source_text = match source {
            Some(PatchSource::Published(aasl_patches::TargetStatus::Verified)) => {
                t("prefs.patch.verified")
            }
            Some(PatchSource::Published(_)) => t("prefs.patch.untested"),
            Some(PatchSource::Built) => t("prefs.patch.built"),
            Some(PatchSource::NotNeeded) => t("prefs.patch.not_needed"),
            Some(PatchSource::MustBuild) if can_build => t("prefs.patch.on_use"),
            Some(PatchSource::MustBuild) => t("prefs.patch.need_builder"),
            None => "",
        };
        let needs_build = plan.as_ref().is_some_and(|p| p.needs_build());
        let active_title = plan
            .as_ref()
            .map(|p| p.recipe.title.clone())
            .unwrap_or_else(|| st.recipe.clone());
        let in_use = vec![
            row(
                active_title,
                Some(format!(
                    "{} · {} · {source_text}",
                    st.recipe,
                    proton
                        .as_ref()
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| t("stat.none").into())
                )),
                Some(bundle_chip.into_element()),
            ),
            row(
                t("recipes.build"),
                Some(if !can_build {
                    match svc().podman_install_hint() {
                        Some(cmd) => format!("{} {cmd}", t("prefs.patches.build.missing.cmd")),
                        None => t("prefs.patches.build.missing").to_string(),
                    }
                } else {
                    t("recipes.build.sub").to_string()
                }),
                Some(
                    Button::new()
                        .maybe(source != Some(PatchSource::MustBuild), |b| b.flat())
                        .enabled(!busy && needs_build && can_build && proton.is_some())
                        .on_press(move |_| {
                            ui.run(
                                async {
                                    let bundle = svc().recipe_build(None, None, false).await?;
                                    svc().patch_apply(bundle).await
                                },
                                |_| {},
                            )
                        })
                        .child(t("recipes.build.now"))
                        .into_element(),
                ),
            ),
            row(
                t("prefs.patches.restore"),
                Some(t("recipes.restore.sub").into()),
                Some(
                    Button::new()
                        .flat()
                        .enabled(!busy)
                        .on_press(move |_| ui.run(async { svc().patch_restore() }, |_| {}))
                        .child(t("recipes.restore"))
                        .into_element(),
                ),
            ),
        ];

        let rows: Vec<Element> = list
            .read()
            .iter()
            .map(|(r, v)| {
                let id = r.id.clone();
                let (id_use, id_dup, id_exp, id_share, id_del) =
                    (id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
                let mut tags = vec![
                    if r.builtin {
                        t("recipes.builtin")
                    } else {
                        t("recipes.yours")
                    }
                    .to_string(),
                ];
                tags.push(
                    r.proton
                        .clone()
                        .unwrap_or_else(|| t("recipes.proton.picked").into()),
                );
                tags.push(r.ingredients.join(", "));
                let mut sub = tags.join(" · ");
                if let Some(v) = v.as_ref().filter(|v| v.reports > 0) {
                    sub.push_str(&format!("\n{}", verdict_line(v)));
                }
                let trailing = rect()
                    .horizontal()
                    .spacing(space::S)
                    .cross_align(Alignment::Center)
                    .maybe_child(r.custom_code.then(|| chip(t("recipes.custom"), Tone::Warn)))
                    .maybe_child(v.as_ref().map(standing_chip))
                    .maybe_child(r.active.then(|| chip(t("recipes.active"), Tone::Accent)))
                    .maybe_child((!r.active).then(|| {
                        Button::new()
                            .compact()
                            .enabled(!busy)
                            .on_press(move |_| {
                                let id = id_use.clone();
                                ui.run(async move { svc().recipe_use(&id).await }, move |_| {
                                    reload()
                                })
                            })
                            .child(t("prefs.proton.use"))
                    }))
                    .child(
                        Button::new()
                            .compact()
                            .flat()
                            .on_press(move |_| {
                                if let Ok(r) = svc().recipe(&id_dup) {
                                    draft.set(Draft::from_recipe(&r, true));
                                    tab.set(Tab::Create);
                                }
                            })
                            .child(t("recipes.duplicate")),
                    )
                    .child(
                        Button::new()
                            .compact()
                            .flat()
                            .on_press(move |_| match svc().recipe_export(&id_exp) {
                                Ok(text) => copy(ui, text),
                                Err(e) => ui.error.set(Some(e.to_string())),
                            })
                            .child(t("recipes.export")),
                    )
                    .maybe_child((!r.builtin).then(|| {
                        Button::new()
                            .compact()
                            .flat()
                            .on_press(move |_| match svc().recipe_share(&id_share, &[]) {
                                Ok(s) => {
                                    open_url(&s.url);
                                    share.set(Some((id_share.clone(), s)));
                                }
                                Err(e) => ui.error.set(Some(e.to_string())),
                            })
                            .child(t("recipes.share"))
                    }))
                    .maybe_child((!r.builtin && !r.active).then(|| {
                        Button::new()
                            .compact()
                            .flat()
                            .on_press(move |_| {
                                if let Err(e) = svc().recipe_delete(&id_del) {
                                    ui.error.set(Some(e.to_string()));
                                }
                                reload();
                            })
                            .child(t("prefs.proton.remove"))
                    }))
                    .into_element();
                row(r.title.clone(), Some(sub), Some(trailing))
            })
            .collect();

        let share_panel = share.read().clone().map(|(id, s)| {
            let (recipe_text, reports_text) = (s.recipe.clone(), s.reports.clone());
            let url = s.url.clone();
            group(
                &format!("{} {id}", t("recipes.share")),
                vec![
                    row(
                        if s.prefilled {
                            t("recipes.share.prefilled")
                        } else {
                            t("recipes.share.paste")
                        },
                        Some(t("recipes.share.sub").into()),
                        Some(
                            Button::new()
                                .compact()
                                .on_press(move |_| open_url(&url))
                                .child(t("recipes.share.open"))
                                .into_element(),
                        ),
                    ),
                    row(
                        t("recipes.share.recipe"),
                        None,
                        Some(
                            Button::new()
                                .compact()
                                .flat()
                                .on_press(move |_| copy(ui, recipe_text.clone()))
                                .child(t("recipes.copy.button"))
                                .into_element(),
                        ),
                    ),
                    row(
                        t("recipes.share.reports"),
                        None,
                        Some(
                            Button::new()
                                .compact()
                                .flat()
                                .on_press(move |_| copy(ui, reports_text.clone()))
                                .child(t("recipes.copy.button"))
                                .into_element(),
                        ),
                    ),
                ],
            )
        });

        rect()
            .spacing(space::XL)
            .child(group(t("recipes.in_use"), in_use))
            .maybe_child(share_panel)
            .child(group(t("prefs.recipes"), rows))
            .child(
                rect()
                    .horizontal()
                    .spacing(space::S)
                    .child(
                        Button::new()
                            .on_press(move |_| {
                                draft.set(Draft {
                                    ingredients: svc()
                                        .recipe(aasl_patches::recipe::DEFAULT_RECIPE)
                                        .map(|r| r.ingredients)
                                        .unwrap_or_default(),
                                    ..Default::default()
                                });
                                tab.set(Tab::Create);
                            })
                            .child(t("recipes.new")),
                    )
                    .child(
                        Button::new()
                            .flat()
                            .on_press(move |_| {
                                let text = freya::clipboard::Clipboard::get().unwrap_or_default();
                                match svc().recipe_import(&text) {
                                    Ok(r) => {
                                        ui.notice(
                                            Severity::Info,
                                            format!("{} {}", t("recipes.imported"), r.id),
                                        );
                                        reload();
                                    }
                                    Err(e) => {
                                        ui.error.set(Some(format!("{}: {e}", t("recipes.import"))))
                                    }
                                }
                            })
                            .child(t("recipes.import")),
                    ),
            )
            .child(muted(t("recipes.import.sub")))
    }
}

#[derive(PartialEq)]
struct CreateTab {
    ui: Ui,
    tab: State<Tab>,
    draft: State<Draft>,
}

impl Component for CreateTab {
    fn render(&self) -> impl IntoElement {
        let mut ui = self.ui;
        let mut tab = self.tab;
        let mut draft = self.draft;
        let lib = svc().library().ok();
        let d = draft.read().clone();
        let id = use_state(|| d.id.clone());
        let name = use_state(|| d.title.clone());
        let desc = use_state(|| d.description.clone());
        // Keep the text fields and the draft in step.
        let (i, n, ds) = (id.read().clone(), name.read().clone(), desc.read().clone());
        if i != d.id || n != d.title || ds != d.description {
            let mut w = draft.write();
            w.id = i;
            w.title = n;
            w.description = ds;
        }
        let d = draft.read().clone();
        let recipe = d.recipe();
        let check = lib.as_ref().map(|l| l.plan(&recipe));
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

        let mut protons: Vec<(Option<String>, String)> = vec![
            (None, t("recipes.proton.picked").to_string()),
            (
                Some("ge:latest".into()),
                format!("GE-Proton · {}", t("recipes.proton.latest")),
            ),
            (
                Some("cachyos:latest".into()),
                format!("CachyOS · {}", t("recipes.proton.latest")),
            ),
        ];
        for p in svc().proton_installed() {
            protons.push((Some(p.proton.id.to_string()), p.proton.id.tag.clone()));
        }
        if let Some(p) = &d.proton
            && !protons.iter().any(|(k, _)| k.as_ref() == Some(p))
        {
            protons.push((Some(p.clone()), p.clone()));
        }
        let proton_pills: Vec<Element> = protons
            .into_iter()
            .map(|(key, text)| {
                let selected = key == d.proton;
                Button::new()
                    .compact()
                    .corner_radius(RADIUS_PILL)
                    .maybe(selected, |b| b.filled())
                    .maybe(!selected, |b| b.flat())
                    .on_press(move |_| draft.write().proton = key.clone())
                    .child(
                        label()
                            .text(text)
                            .color(if selected { T.accent_text } else { T.text }),
                    )
                    .into_element()
            })
            .collect();

        let mut all: Vec<Ingredient> = lib
            .as_ref()
            .map(|l| l.ingredients.clone())
            .unwrap_or_default();
        for c in &d.custom {
            if !all.iter().any(|i| i.id == c.id) {
                all.push(c.clone());
            }
        }
        let ingredient_rows: Vec<Element> = all
            .into_iter()
            .map(|ing| {
                let on = d.ingredients.contains(&ing.id);
                let is_custom = d.custom.iter().any(|c| c.id == ing.id);
                let key = ing.id.clone();
                let kind = match ing.kind {
                    Kind::Patch => format!("{} · {}", t("recipes.kind.patch"), ing.dlls.join(", ")),
                    Kind::Env => format!(
                        "{} · {}",
                        t("recipes.kind.env"),
                        ing.env.keys().cloned().collect::<Vec<_>>().join(", ")
                    ),
                };
                Button::new()
                    .key(ing.id.clone())
                    .flat()
                    .expanded()
                    .on_press(move |_| {
                        let mut w = draft.write();
                        if let Some(p) = w.ingredients.iter().position(|x| *x == key) {
                            w.ingredients.remove(p);
                        } else {
                            w.ingredients.push(key.clone());
                        }
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
                                    .child(body(ing.title.clone()))
                                    .child(muted(if ing.summary.is_empty() {
                                        ing.id.clone()
                                    } else {
                                        ing.summary.clone()
                                    }))
                                    .child(muted(kind)),
                            )
                            .maybe_child(is_custom.then(|| chip(t("recipes.custom"), Tone::Warn))),
                    )
                    .into_element()
            })
            .collect();

        let field = |label_key: &'static str, input: Element| {
            rect()
                .width(Size::fill())
                .spacing(space::XS)
                .child(muted(t(label_key)))
                .child(input)
                .into_element()
        };
        let mut save = move |use_it: bool| {
            let r = draft.read().recipe();
            match svc().recipe_save(&r) {
                Ok(info) => {
                    ui.notice(
                        Severity::Info,
                        format!("{} {}", t("recipes.saved"), info.id),
                    );
                    if use_it {
                        let id = info.id.clone();
                        ui.run(async move { svc().recipe_use(&id).await }, |_| {});
                    }
                    tab.set(Tab::Recipes);
                }
                Err(e) => ui.error.set(Some(e.to_string())),
            }
        };
        let order = if d.ingredients.is_empty() {
            t("stat.none").to_string()
        } else {
            d.ingredients.join(" → ")
        };
        rect()
            .spacing(space::XL)
            .child(muted(t("recipes.create.lead")))
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(space::M)
                    .child(field(
                        "recipes.field.title",
                        Input::new(name)
                            .placeholder(t("recipes.field.title.ph"))
                            .width(Size::fill())
                            .into_element(),
                    ))
                    .child(field(
                        "recipes.field.id",
                        Input::new(id)
                            .placeholder("my-recipe")
                            .width(Size::fill())
                            .into_element(),
                    ))
                    .child(field(
                        "recipes.field.description",
                        Input::new(desc).width(Size::fill()).into_element(),
                    )),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(space::S)
                    .child(body(t("recipes.field.proton")).font_weight(FontWeight::SEMI_BOLD))
                    .child(muted(t("recipes.field.proton.sub")))
                    .child(
                        rect()
                            .width(Size::fill())
                            .horizontal()
                            .spacing(space::XS)
                            .children(proton_pills),
                    ),
            )
            .child(group(t("recipes.ingredients"), ingredient_rows))
            .child(muted(format!("{}: {order}", t("recipes.order"))))
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .spacing(space::S)
                    .child(
                        rect()
                            .width(Size::flex(1.))
                            .spacing(space::XS)
                            .child(chip(check_text, check_tone))
                            .maybe_child(custom.then(|| muted(t("recipes.custom.warn")))),
                    )
                    .child(
                        Button::new()
                            .flat()
                            .on_press(move |_| tab.set(Tab::Recipes))
                            .child(t("recipes.cancel")),
                    )
                    .child(
                        Button::new()
                            .enabled(ok)
                            .on_press(move |_| save(false))
                            .child(t("recipes.save")),
                    )
                    .child(
                        Button::new()
                            .filled()
                            .enabled(ok)
                            .on_press(move |_| save(true))
                            .child(label().text(t("recipes.save_use")).color(T.accent_text)),
                    ),
            )
    }
}

#[derive(PartialEq)]
struct SessionsTab(Ui);

fn session_row(s: &SavedReport) -> Element {
    let r = &s.report;
    let when = r.started_at;
    let mut parts = vec![
        format!("{} min", r.duration_s / 60),
        format!("{} {}", r.worlds, t("recipes.worlds")),
        format!(
            "{} {}/{}",
            t("recipes.videos"),
            r.video.opened - r.video.failed.min(r.video.opened),
            r.video.opened
        ),
    ];
    if let Some(p) = r.perf {
        parts.push(format!(
            "{:.0} fps · 1 % low {:.0} · p99 {:.1} ms",
            p.avg_fps, p.low1_fps, p.p99_frametime_ms
        ));
    }
    parts.push(r.hardware.gpu.clone());
    let crash = match r.crash {
        Some(Crash::InGame) => Some(chip(t("recipes.crash"), Tone::Error)),
        Some(Crash::OnExit) => Some(chip(t("recipes.crash.exit"), Tone::Neutral)),
        None => None,
    };
    let trailing = rect()
        .horizontal()
        .spacing(space::S)
        .maybe_child(r.flat.then(|| chip(t("recipes.desktop"), Tone::Neutral)))
        .maybe_child(crash)
        .into_element();
    row(
        format!("{} · {} · {}", r.recipe.id, r.recipe.proton, fmt_time(when)),
        Some(parts.join(" · ")),
        Some(trailing),
    )
}

/// UTC date and time, enough to tell sessions apart.
fn fmt_time(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let (h, m) = ((secs % 86400) / 3600, (secs % 3600) / 60);
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

impl Component for SessionsTab {
    fn render(&self) -> impl IntoElement {
        let ui = self.0;
        let mut list = use_state(|| svc().sessions());
        let mut on = use_state(|| svc().config().session_reports);
        let mut ui2 = ui;
        let setter = WritableSetter::new(on, move |v: bool| {
            let mut cfg = svc().config();
            cfg.session_reports = v;
            match svc().set_config(cfg) {
                Ok(()) => on.set(v),
                Err(e) => ui2.error.set(Some(e.to_string())),
            }
        });
        let rows: Vec<Element> = list.read().iter().map(session_row).collect();
        let empty = rows.is_empty();
        rect()
            .spacing(space::XL)
            .child(group(
                t("recipes.sessions.title"),
                vec![
                    row(
                        t("recipes.sessions.record"),
                        Some(t("recipes.sessions.record.sub").into()),
                        Some(Switch::new(setter).into_element()),
                    ),
                    row(
                        t("recipes.sessions.last"),
                        Some(t("recipes.sessions.last.sub").into()),
                        Some(
                            Button::new()
                                .flat()
                                .compact()
                                .on_press(move |_| {
                                    ui.run(async { svc().record_session(None) }, move |_| {
                                        list.set(svc().sessions())
                                    })
                                })
                                .child(t("recipes.sessions.last.button"))
                                .into_element(),
                        ),
                    ),
                ],
            ))
            .child(if empty {
                group(
                    t("recipes.sessions.saved"),
                    vec![row(
                        t("recipes.sessions.none"),
                        Some(t("recipes.sessions.none.sub").into()),
                        None,
                    )],
                )
            } else {
                group(t("recipes.sessions.saved"), rows)
            })
    }
}

#[derive(PartialEq)]
struct CommunityTab(Ui);

impl Component for CommunityTab {
    fn render(&self) -> impl IntoElement {
        let ui = self.0;
        let mut index = use_state(|| None::<Result<aasl_patches::community::Index, String>>);
        use_hook(move || {
            spawn(async move {
                let r = tokio::spawn(async { svc().community().await })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));
                index.set(Some(r));
            });
        });
        let body_el: Element = match index.read().clone() {
            None => rect()
                .width(Size::fill())
                .padding(space::XL)
                .center()
                .child(CircularLoader::new().size(28.))
                .into_element(),
            Some(Err(e)) => group(
                t("recipes.tab.community"),
                vec![row(t("recipes.community.offline"), Some(e), None)],
            )
            .into_element(),
            Some(Ok(idx)) if idx.recipes.is_empty() => group(
                t("recipes.tab.community"),
                vec![row(
                    t("recipes.community.empty"),
                    Some(t("recipes.community.empty.sub").into()),
                    None,
                )],
            )
            .into_element(),
            Some(Ok(idx)) => group(
                t("recipes.tab.community"),
                idx.recipes
                    .iter()
                    .map(|e| {
                        let id = e.id.clone();
                        let mut sub = format!(
                            "{} · {}",
                            e.ingredients.join(", "),
                            verdict_line(&e.verdict)
                        );
                        if !e.description.is_empty() {
                            sub = format!("{}\n{sub}", e.description);
                        }
                        let trailing = rect()
                            .horizontal()
                            .spacing(space::S)
                            .cross_align(Alignment::Center)
                            .maybe_child(
                                e.custom_code.then(|| chip(t("recipes.custom"), Tone::Warn)),
                            )
                            .child(standing_chip(&e.verdict))
                            .child(
                                Button::new()
                                    .compact()
                                    .on_press(move |_| {
                                        let id = id.clone();
                                        ui.run(
                                            async move { svc().community_install(&id).await },
                                            move |r| {
                                                ui.notice(
                                                    Severity::Info,
                                                    format!("{} {}", t("recipes.imported"), r.id),
                                                )
                                            },
                                        )
                                    })
                                    .child(t("recipes.community.add")),
                            )
                            .into_element();
                        row(e.title.clone(), Some(sub), Some(trailing))
                    })
                    .collect(),
            )
            .into_element(),
        };
        rect()
            .spacing(space::XL)
            .child(muted(t("recipes.community.lead")))
            .child(body_el)
            .child(muted(aasl_patches::community::RULES).font_size(size::SMALL))
    }
}
