//! Compatibility recipes: the setup tunnel for the Proton and its fixes,
//! the recipes you keep, the evidence from play sessions and the community.

use aasl_core::Severity;
use aasl_patches::community::{Status as Standing, Verdict};
use aasl_service::{PatchSource, RecipeInfo, Share};
use freya::prelude::*;

use crate::i18n::t;
use crate::setup::{Flow, Intent, SetupFlow, Step};
use crate::state::Ui;
use crate::svc;
use crate::theme::{RADIUS_PANEL, RADIUS_PILL, T, size, space};
use crate::widgets::{Tone, chip, group, muted, row, title};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Setup,
    Recipes,
    Sessions,
    Community,
}

const TABS: &[(Tab, &str)] = &[
    (Tab::Setup, "recipes.tab.setup"),
    (Tab::Recipes, "recipes.tab.mine"),
    (Tab::Sessions, "recipes.tab.sessions"),
    (Tab::Community, "recipes.tab.community"),
];

fn tab_button(
    label_text: &'static str,
    selected: bool,
    on: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    Button::new()
        .key(label_text)
        .compact()
        .corner_radius(RADIUS_PILL)
        .flat()
        .maybe(selected, |b| b.background(T.raised))
        .on_press(on)
        .child(
            label()
                .text(t(label_text))
                .color(if selected { T.text } else { T.muted }),
        )
        .into_element()
}

/// Only a recipe the community vouches for gets a badge; "proposed" and
/// "untested" are what every recipe is until then.
fn standing_chip(v: &Verdict) -> Option<Rect> {
    (v.status == Standing::Community).then(|| chip(t("recipes.standing.community"), Tone::Ok))
}

/// What the reports say, in words: "3 sessions, 4.2 h, no crash, videos played 100 %".
fn verdict_line(v: &Verdict) -> String {
    let mut parts = vec![
        if v.reports == 1 {
            t("recipes.stat.one").to_string()
        } else {
            format!("{} {}", v.reports, t("recipes.stat.many"))
        },
        format!("{:.1} h", v.hours),
        match v.crashes_in_game {
            0 => t("recipes.stat.nocrash").to_string(),
            1 => t("recipes.stat.crash.one").to_string(),
            n => format!("{n} {}", t("recipes.stat.crash.many")),
        },
    ];
    if let Some(s) = v.video_success {
        parts.push(format!("{} {:.0} %", t("recipes.stat.videos"), s * 100.0));
    }
    parts.join(", ")
}

/// "4 fixes", or "1 fix".
fn fix_count(n: usize) -> String {
    if n == 1 {
        t("recipes.stat.fixes.one").to_string()
    } else {
        format!("{n} {}", t("recipes.stat.fixes.many"))
    }
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
        let flow = use_state(crate::setup::first_flow);
        let current = *tab.read();
        let in_tunnel = flow.read().is_some();
        let content: Element = if in_tunnel {
            SetupFlow { ui, flow }.into_element()
        } else {
            match current {
                Tab::Setup => HomeTab { ui, flow }.into_element(),
                Tab::Recipes => MineTab { ui, flow }.into_element(),
                Tab::Sessions => crate::sessions::SessionsTab(ui).into_element(),
                Tab::Community => CommunityTab(ui).into_element(),
            }
        };
        rect()
            .spacing(space::XL)
            .child(
                rect()
                    .spacing(space::XS)
                    .child(title(t("prefs.recipes")))
                    .maybe_child((!in_tunnel).then(|| muted(t("recipes.lead")))),
            )
            .maybe_child((!in_tunnel).then(|| {
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
                    )
            }))
            .child(content)
    }
}

/// Where things stand, and the one way into the tunnel.
#[derive(PartialEq)]
struct HomeTab {
    ui: Ui,
    flow: State<Option<Flow>>,
}

impl Component for HomeTab {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let mut flow = self.flow;
        let st = ui.status.read().clone();
        let busy = st.operation.is_some();
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
        let recipe_title = plan
            .as_ref()
            .map(|p| p.recipe.title.clone())
            .unwrap_or_else(|| st.recipe.clone());
        let proton_tag = proton
            .as_ref()
            .map(|p| p.tag.clone())
            .unwrap_or_else(|| t("stat.none").to_string());
        let build_now = needs_build && source == Some(PatchSource::MustBuild);
        rect()
            .width(Size::fill())
            .spacing(space::XL)
            .child(
                rect()
                    .width(Size::fill())
                    .padding(space::XL)
                    .spacing(space::L)
                    .corner_radius(RADIUS_PANEL)
                    .background(T.surface)
                    .border(Border::new().fill(T.border).width(1.))
                    .child(muted(t("setup.current")))
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
                                    .spacing(space::XS)
                                    .child(
                                        label()
                                            .text(proton_tag)
                                            .font_size(size::TITLE)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(T.text),
                                    )
                                    .child(muted(format!("{recipe_title}, {source_text}"))),
                            )
                            .child(bundle_chip),
                    )
                    .child(
                        rect()
                            .horizontal()
                            .spacing(space::S)
                            .child(
                                Button::new()
                                    .filled()
                                    .on_press(move |_| {
                                        flow.set(Some(Flow::start(None, Step::Proton, Intent::Use)))
                                    })
                                    .child(
                                        label()
                                            .text(if proton.is_some() {
                                                t("setup.change")
                                            } else {
                                                t("setup.configure")
                                            })
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(T.accent_text),
                                    ),
                            )
                            .maybe_child(build_now.then(|| {
                                Button::new()
                                    .enabled(!busy && can_build)
                                    .on_press(move |_| {
                                        ui.run(
                                            async {
                                                let bundle =
                                                    svc().recipe_build(None, None, false).await?;
                                                svc().patch_apply(bundle).await
                                            },
                                            |_| {},
                                        )
                                    })
                                    .child(t("recipes.build.now"))
                            })),
                    ),
            )
            .child(group(
                t("setup.repair"),
                vec![row(
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
                )],
            ))
    }
}

#[derive(PartialEq)]
struct MineTab {
    ui: Ui,
    flow: State<Option<Flow>>,
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
        let mut flow = self.flow;
        let mut list = use_state(load_mine);
        let mut share = use_state(|| None::<(String, Share)>);
        let busy = ui.status.read().operation.is_some();
        let mut reload = move || list.set(load_mine());

        let lib = svc().library().ok();
        let fix_titles = |ids: &[String]| -> String {
            ids.iter()
                .map(|id| {
                    let en = lib
                        .as_ref()
                        .and_then(|l| l.ingredients.iter().find(|i| i.id == *id))
                        .map(|i| i.title.clone())
                        .unwrap_or_else(|| id.clone());
                    crate::i18n::fix_title(id, &en)
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut official: Vec<Element> = Vec::new();
        let mut yours: Vec<Element> = Vec::new();
        for (r, v) in list.read().iter() {
            let id = r.id.clone();
            let (id_use, id_fork, id_edit, id_share, id_del) =
                (id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
            // First line: what the recipe is for (or, failing that, its fixes).
            // Second: how many fixes, the pinned Proton, what play sessions showed.
            let first = if r.description.is_empty() {
                fix_titles(&r.ingredients)
            } else if r.builtin {
                crate::i18n::recipe_desc(&r.id, &r.description)
            } else {
                r.description.clone()
            };
            let mut facts = vec![fix_count(r.ingredients.len())];
            if let Some(p) = &r.proton {
                facts.push(format!(
                    "{} {}",
                    t("recipes.stat.pinned"),
                    p.replace(':', " ")
                ));
            }
            if let Some(v) = v.as_ref().filter(|v| v.reports > 0) {
                facts.push(format!("{} {}", t("recipes.stat.tested"), verdict_line(v)));
            }
            let sub = format!("{first}\n{}", facts.join(" · "));
            let trailing = rect()
                .horizontal()
                .spacing(space::S)
                .cross_align(Alignment::Center)
                .maybe_child(r.custom_code.then(|| chip(t("recipes.custom"), Tone::Warn)))
                .maybe_child(v.as_ref().and_then(standing_chip))
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
                .maybe_child(r.builtin.then(|| {
                    Button::new()
                        .compact()
                        .flat()
                        .on_press(move |_| {
                            flow.set(Some(Flow::start(Some(&id_fork), Step::Fixes, Intent::Fork)))
                        })
                        .child(t("recipes.fork"))
                }))
                .maybe_child((!r.builtin).then(|| {
                    Button::new()
                        .compact()
                        .flat()
                        .on_press(move |_| {
                            flow.set(Some(Flow::start(Some(&id_edit), Step::Fixes, Intent::Edit)))
                        })
                        .child(t("recipes.edit"))
                }))
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
            let name = if r.builtin {
                crate::i18n::recipe_title(&r.id, &r.title)
            } else {
                r.title.clone()
            };
            let el = row(name, Some(sub), Some(trailing));
            if r.builtin {
                official.push(el)
            } else {
                yours.push(el)
            }
        }
        if yours.is_empty() {
            yours.push(row(
                t("recipes.yours.none"),
                Some(t("recipes.yours.none.sub").into()),
                None,
            ));
        }

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
            .maybe_child(share_panel)
            .child(group(t("recipes.official"), official))
            .child(group(t("recipes.yours"), yours))
            .child(
                rect()
                    .horizontal()
                    .spacing(space::S)
                    .child(
                        Button::new()
                            .on_press(move |_| {
                                flow.set(Some(Flow::start(None, Step::Proton, Intent::New)));
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
                        let fixes = e
                            .ingredients
                            .iter()
                            .map(|i| crate::i18n::fix_title(i, i))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let first = if e.description.is_empty() {
                            fixes
                        } else {
                            e.description.clone()
                        };
                        let sub = format!(
                            "{first}\n{} · {}",
                            fix_count(e.ingredients.len()),
                            verdict_line(&e.verdict)
                        );
                        let trailing = rect()
                            .horizontal()
                            .spacing(space::S)
                            .cross_align(Alignment::Center)
                            .maybe_child(
                                e.custom_code.then(|| chip(t("recipes.custom"), Tone::Warn)),
                            )
                            .maybe_child(standing_chip(&e.verdict))
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
