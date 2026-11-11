//! `aasl proton ...`, `aasl patch ...` and `aasl recipe ...`.

use std::path::PathBuf;
use std::process::ExitCode;

use aasl_service::Service;
use clap::Subcommand;

use crate::output::Out;

#[derive(Subcommand)]
pub enum ProtonCmd {
    /// Installed Proton builds.
    List,
    /// Releases available upstream for a provider (ge, cachyos).
    Available {
        #[arg(default_value = "ge")]
        provider: String,
        #[arg(long)]
        refresh: bool,
    },
    /// Download and install a build, e.g. cachyos:cachyos-11.0-20261015-slr.
    Install { id: String },
    /// Make Somnium use this build (downloads and patches it).
    Use {
        id: String,
        /// Use it even without a patch bundle.
        #[arg(long)]
        force: bool,
    },
    /// Delete an installed build.
    Remove { id: String },
}

#[derive(Subcommand)]
pub enum PatchCmd {
    /// Patch bundle installed in the current Proton, and whether it is intact.
    Status,
    /// Patch files of the ingredient library.
    List,
    /// Bundles in the catalog for a Proton build.
    Catalog { id: Option<String> },
    /// What building the recipe in use would do (Wine commit, trees, container command).
    Plan { id: String },
    /// Rebuild the recipe in use for a Proton build in a container (slow).
    Build {
        id: String,
        /// Install the result into the current Proton when done.
        #[arg(long)]
        apply: bool,
    },
    /// Install a bundle file into the current Proton.
    Apply { bundle: PathBuf },
    /// Put Proton's stock files back.
    Restore,
}

#[derive(Subcommand)]
pub enum RecipeCmd {
    /// Built-in and your own recipes; * marks the one in use.
    List,
    /// Print a recipe as TOML.
    Show { id: String },
    /// The ingredients recipes pick from.
    Ingredients,
    /// Print a self-contained copy (every patch inlined) to share.
    Export { id: String },
    /// Check a recipe file without saving it.
    Check {
        file: PathBuf,
        /// Print it in canonical form (library ingredients by reference).
        #[arg(long)]
        print: bool,
    },
    /// Save a recipe file as one of yours.
    Import { file: PathBuf },
    /// Delete one of your recipes.
    Remove { id: String },
    /// Build a recipe's bundle (default: the one in use, on its Proton).
    Build {
        id: Option<String>,
        /// Proton to build for (provider:tag); default: the recipe's.
        #[arg(long)]
        proton: Option<String>,
        /// Rebuild even when a bundle for it is cached.
        #[arg(long)]
        force: bool,
    },
    /// Make Somnium use a recipe (builds it when nothing fits).
    Use { id: String },
    /// How a recipe stands on your own session reports (same rules as CI).
    Verdict { id: String },
    /// The key a recipe file has on a Proton: session reports carry it.
    Key { file: PathBuf, proton: String },
    /// Submit a recipe with its session reports to the community (GitHub form).
    Share {
        id: String,
        /// Only these reports (ids from `aasl session list`); default: all of this recipe.
        #[arg(long = "report")]
        reports: Vec<String>,
        /// Open the form in the browser.
        #[arg(long)]
        open: bool,
    },
}

#[derive(Subcommand)]
pub enum SessionCmd {
    /// Saved session reports, newest first.
    List,
    /// One report as JSON.
    Show { id: String },
    /// Make the report of the last session (after `aasl launch --detach`).
    Record,
}

#[derive(Subcommand)]
pub enum CommunityCmd {
    /// Community recipes and their standing.
    List,
    /// Save a community recipe as one of yours.
    Install { id: String },
    /// Judge one community recipe directory (recipe.toml + reports/*.json). For CI.
    Verify { dir: PathBuf },
    /// Rebuild the index of a community directory (recipes/*). For CI.
    Index {
        root: PathBuf,
        /// Write root/index.json instead of printing it.
        #[arg(long)]
        write: bool,
        /// Fail when a recipe directory does not check (CI).
        #[arg(long)]
        strict: bool,
    },
}

pub fn parse_id(s: &str) -> Result<aasl_components::ProtonId, aasl_core::Error> {
    aasl_components::ProtonId::parse(s)
}

fn joined<T>(v: &[T], f: impl Fn(&T) -> String) -> String {
    v.iter().map(f).collect::<Vec<_>>().join("\n")
}

pub async fn proton(out: &Out, svc: &Service, cmd: ProtonCmd) -> ExitCode {
    match cmd {
        ProtonCmd::List => out.ok(&svc.proton_installed(), |v| {
            if v.is_empty() {
                return "No Proton installed by AASL.".into();
            }
            joined(v, |p| {
                let patches = match (&p.patches.bundle, p.patches.intact) {
                    (Some(b), true) => b.clone(),
                    (Some(b), false) => format!("{b} (changed)"),
                    (None, _) => "no patches".into(),
                };
                format!(
                    "{}{}  {}  {patches}",
                    if p.in_use { "* " } else { "  " },
                    p.proton.id,
                    p.proton.variant
                )
            })
        }),
        ProtonCmd::Available { provider, refresh } => {
            out.result(svc.proton_releases(&provider, refresh).await, |v| {
                joined(v, |r| {
                    let p = match r.patches {
                        Some(aasl_patches::TargetStatus::Verified) => "patches verified",
                        Some(aasl_patches::TargetStatus::Untested) => "patches untested",
                        _ => "no patches",
                    };
                    let date = r
                        .published_at
                        .as_deref()
                        .and_then(|d| d.get(..10))
                        .unwrap_or("");
                    format!(
                        "{}{}  {date}  {p}{}",
                        if r.in_use { "* " } else { "  " },
                        r.id,
                        if r.installed { "  installed" } else { "" }
                    )
                })
            })
        }
        ProtonCmd::Install { id } => match parse_id(&id) {
            Ok(id) => out.result(svc.proton_install(id).await, |p| {
                format!("Installed in {}", p.display())
            }),
            Err(e) => out.error(&e),
        },
        ProtonCmd::Use { id, force } => match parse_id(&id) {
            Ok(id) => out.result(svc.proton_use(id.clone(), force).await, |_| {
                format!("Somnium now uses {id}.")
            }),
            Err(e) => out.error(&e),
        },
        ProtonCmd::Remove { id } => match parse_id(&id) {
            Ok(id) => out.result(svc.proton_remove(&id), |_| format!("Removed {id}.")),
            Err(e) => out.error(&e),
        },
    }
}

pub async fn patch(out: &Out, svc: &Service, cmd: PatchCmd) -> ExitCode {
    match cmd {
        PatchCmd::Status => out.ok(&svc.status().patches, |p| match p {
            None => "No Proton selected.".into(),
            Some(s) => match (&s.bundle, s.intact) {
                (None, _) => "No patch bundle installed.".into(),
                (Some(b), true) => format!("{b}: installed, files intact."),
                (Some(b), false) => {
                    format!("{b}: files changed since install: {}", s.changed.join(", "))
                }
            },
        }),
        PatchCmd::List => out.result(svc.patch_list(), |v| {
            joined(v, |p| {
                format!("{:22} {}  {}", p.ingredient, p.name, p.subject)
            })
        }),
        PatchCmd::Catalog { id } => {
            let cat = svc.catalog(true).await;
            match id
                .and_then(|s| parse_id(&s).ok())
                .or_else(|| svc.current_proton())
            {
                Some(id) => out.ok(&cat.resolve(&id), |v| {
                    if v.is_empty() {
                        format!("No bundle for {id}.")
                    } else {
                        joined(v, |m| format!("{} ({:?})", m.bundle.id, m.status))
                    }
                }),
                None => out.ok(&cat, |c| {
                    format!(
                        "Recommended: {}\nBundles: {}",
                        c.recommended_proton,
                        c.bundles
                            .iter()
                            .map(|b| b.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }),
            }
        }
        PatchCmd::Plan { id } => match parse_id(&id) {
            Ok(id) => out.result(svc.patch_plan(&id).await, |p| {
                serde_json::to_string_pretty(p).unwrap_or_default()
            }),
            Err(e) => out.error(&e),
        },
        PatchCmd::Build { id, apply } => match parse_id(&id) {
            Ok(id) => {
                let r = svc.patch_build(id).await;
                if let (Ok(p), true) = (&r, apply)
                    && let Err(e) = svc.patch_apply(p.clone()).await
                {
                    return out.error(&e);
                }
                out.result(r, |p| format!("Bundle: {}", p.display()))
            }
            Err(e) => out.error(&e),
        },
        PatchCmd::Apply { bundle } => out.result(svc.patch_apply(bundle).await, |_| {
            "Bundle installed.".into()
        }),
        PatchCmd::Restore => out.result(svc.patch_restore(), |n| {
            format!("Restored {n} stock files.")
        }),
    }
}

fn read_file(p: &std::path::Path) -> Result<String, aasl_core::Error> {
    std::fs::read_to_string(p)
        .map_err(|e| aasl_core::Error::io(format!("reading {}", p.display()), e))
}

fn recipe_line(r: &aasl_service::RecipeInfo) -> String {
    let mut tags = Vec::new();
    if r.builtin {
        tags.push("built-in");
    }
    if r.custom_code {
        tags.push("custom patches");
    }
    if !r.needs_build {
        tags.push("no build");
    }
    format!(
        "{}{}  {}  proton: {}  [{}]{}",
        if r.active { "* " } else { "  " },
        r.id,
        r.title,
        r.proton.as_deref().unwrap_or("as picked"),
        r.ingredients.join(", "),
        if tags.is_empty() {
            String::new()
        } else {
            format!("  ({})", tags.join(", "))
        }
    )
}

pub async fn recipe(out: &Out, svc: &Service, cmd: RecipeCmd) -> ExitCode {
    match cmd {
        RecipeCmd::List => out.result(svc.recipes(), |v| joined(v, recipe_line)),
        RecipeCmd::Show { id } => out.result(svc.recipe(&id).and_then(|r| r.to_toml()), |t| {
            t.trim_end().to_string()
        }),
        RecipeCmd::Ingredients => out.result(svc.library().map(|l| l.ingredients), |v| {
            joined(v, |i| format!("{:22} {:?}  {}", i.id, i.kind, i.title))
        }),
        RecipeCmd::Export { id } => {
            out.result(svc.recipe_export(&id), |t| t.trim_end().to_string())
        }
        RecipeCmd::Check { file, print: false } => out.result(
            read_file(&file).and_then(|t| svc.recipe_check(&t)),
            recipe_line,
        ),
        RecipeCmd::Check { file, print: true } => out.result(
            read_file(&file).and_then(|t| svc.library()?.import(&t)?.to_toml()),
            |t| t.trim_end().to_string(),
        ),
        RecipeCmd::Import { file } => {
            out.result(read_file(&file).and_then(|t| svc.recipe_import(&t)), |r| {
                format!(
                    "Saved recipe {}.{}",
                    r.id,
                    if r.custom_code {
                        " It brings its own Wine patches: read them before building."
                    } else {
                        ""
                    }
                )
            })
        }
        RecipeCmd::Remove { id } => {
            out.result(svc.recipe_delete(&id), |_| format!("Removed {id}."))
        }
        RecipeCmd::Build { id, proton, force } => {
            let proton = match proton.map(|p| parse_id(&p)).transpose() {
                Ok(p) => p,
                Err(e) => return out.error(&e),
            };
            out.result(svc.recipe_build(id.as_deref(), proton, force).await, |p| {
                format!("Bundle: {}", p.display())
            })
        }
        RecipeCmd::Use { id } => out.result(svc.recipe_use(&id).await, |_| {
            format!("Somnium now uses the recipe {id}.")
        }),
        RecipeCmd::Key { file, proton } => {
            let r = parse_id(&proton).and_then(|p| {
                let lib = svc.library()?;
                let recipe = lib.import(&read_file(&file)?)?;
                Ok(lib.plan(&recipe)?.bundle_key(&p))
            });
            out.result(r, |k| k.clone())
        }
        cmd @ (RecipeCmd::Verdict { .. } | RecipeCmd::Share { .. }) => {
            recipe_evidence(out, svc, cmd).await
        }
    }
}

fn verdict_text(v: &aasl_patches::community::Verdict) -> String {
    let pct = |x: Option<f32>| {
        x.map(|s| format!("{:.0} %", s * 100.0))
            .unwrap_or_else(|| "n/a".into())
    };
    let mut s = format!(
        "{:?}: {} report(s), {} configuration(s), {:.1} h, {} world(s), crashes in game {} (on exit {}), videos {}, pacing (1 % low / avg) {}{}",
        v.status,
        v.reports,
        v.configs,
        v.hours,
        v.worlds,
        v.crashes_in_game,
        v.crashes_on_exit,
        pct(v.video_success),
        pct(v.pacing),
        v.avg_fps
            .map(|f| format!(", {f:.0} fps"))
            .unwrap_or_default()
    );
    for p in &v.problems {
        s.push_str(&format!("\n  - {p}"));
    }
    s
}

async fn recipe_evidence(out: &Out, svc: &Service, cmd: RecipeCmd) -> ExitCode {
    match cmd {
        RecipeCmd::Verdict { id } => out.result(svc.recipe_verdict(&id), verdict_text),
        RecipeCmd::Share { id, reports, open } => {
            let r = svc.recipe_share(&id, &reports);
            if let (Ok(s), true) = (&r, open) {
                let _ = std::process::Command::new("xdg-open").arg(&s.url).spawn();
            }
            out.result(r, |s| {
                if s.prefilled {
                    format!("Open this to submit (everything is filled in):\n{}", s.url)
                } else {
                    format!(
                        "Too long for a link. Open:\n{}\nand paste the recipe into \"Recipe\":\n\n{}\nand this into \"Session reports\":\n\n{}",
                        s.url, s.recipe, s.reports
                    )
                }
            })
        }
        _ => unreachable!("recipe() routes only Verdict and Share here"),
    }
}

pub async fn session(out: &Out, svc: &Service, cmd: SessionCmd) -> ExitCode {
    match cmd {
        SessionCmd::List => out.ok(&svc.sessions(), |v| {
            if v.is_empty() {
                return "No session report yet.".into();
            }
            joined(v, |s| {
                let r = &s.report;
                format!(
                    "{}  {}  {} min  {} world(s)  videos {}/{}  {}{}{}",
                    s.id,
                    r.recipe.proton,
                    r.duration_s / 60,
                    r.worlds,
                    r.video.opened - r.video.failed.min(r.video.opened),
                    r.video.opened,
                    r.perf
                        .map(|p| format!("{:.0} fps, 1 % low {:.0}", p.avg_fps, p.low1_fps))
                        .unwrap_or_else(|| "no fps".into()),
                    match r.crash {
                        Some(aasl_patches::community::Crash::InGame) => "  CRASH",
                        Some(aasl_patches::community::Crash::OnExit) => "  crash on exit",
                        None => "",
                    },
                    if r.flat { "  (desktop)" } else { "" }
                )
            })
        }),
        SessionCmd::Show { id } => match svc.sessions().into_iter().find(|s| s.id == id) {
            Some(s) => out.ok(&s.report, |r| {
                serde_json::to_string_pretty(r).unwrap_or_default()
            }),
            None => out.error(&aasl_core::Error::Invalid(format!("no report {id}"))),
        },
        SessionCmd::Record => out.result(svc.record_session(None), |s| format!("Saved {}.", s.id)),
    }
}

pub async fn community(out: &Out, svc: &Service, cmd: CommunityCmd) -> ExitCode {
    match cmd {
        CommunityCmd::List => out.result(svc.community().await, |i| {
            if i.recipes.is_empty() {
                return "No community recipe yet.".into();
            }
            joined(&i.recipes, |e| {
                format!(
                    "{:10} {}  {}  [{}]{}",
                    format!("{:?}", e.verdict.status),
                    e.id,
                    e.title,
                    e.ingredients.join(", "),
                    if e.custom_code {
                        "  (custom patches)"
                    } else {
                        ""
                    }
                )
            })
        }),
        CommunityCmd::Install { id } => out.result(svc.community_install(&id).await, |r| {
            format!("Saved {}. Try it with `aasl recipe use {}`.", r.id, r.id)
        }),
        CommunityCmd::Verify { dir } => {
            let r = svc
                .library()
                .and_then(|lib| aasl_patches::community::judge_dir(&lib, &dir));
            out.result(r.map(|(_, e)| e), |e| {
                format!("{} ({})\n{}", e.id, e.title, verdict_text(&e.verdict))
            })
        }
        CommunityCmd::Index {
            root,
            write,
            strict,
        } => {
            let r = svc.library().and_then(|lib| {
                let (idx, errors) = aasl_patches::community::index(&lib, &root);
                for e in &errors {
                    eprintln!("warning: {e}");
                }
                if strict && !errors.is_empty() {
                    return Err(aasl_core::Error::Invalid(format!(
                        "{} community recipe(s) do not check",
                        errors.len()
                    )));
                }
                if write {
                    let text = serde_json::to_string_pretty(&idx).unwrap_or_default();
                    std::fs::write(root.join("index.json"), text + "\n")
                        .map_err(|e| aasl_core::Error::io("writing index.json", e))?;
                }
                Ok(idx)
            });
            out.result(r, |i| format!("{} recipe(s) indexed.", i.recipes.len()))
        }
    }
}
