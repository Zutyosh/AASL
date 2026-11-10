//! `aasl proton ...` and `aasl patch ...`.

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
    /// Patch files and series.
    List,
    /// Bundles in the catalog for a Proton build.
    Catalog { id: Option<String> },
    /// What a build would do (Wine commit, series, container command).
    Plan {
        id: String,
        #[arg(long)]
        ffmpeg: bool,
        /// Only these series (repeatable), e.g. --series tls.
        #[arg(long)]
        series: Vec<String>,
    },
    /// Build a bundle for a Proton build in a container (slow).
    Build {
        id: String,
        /// Also rebuild ffmpeg with dav1d (adds a lot of time).
        #[arg(long)]
        ffmpeg: bool,
        /// Install the result into the current Proton when done.
        #[arg(long)]
        apply: bool,
        /// Only these series (repeatable), e.g. --series tls.
        #[arg(long)]
        series: Vec<String>,
    },
    /// Install a bundle file into the current Proton.
    Apply { bundle: PathBuf },
    /// Put Proton's stock files back.
    Restore,
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
            joined(v, |p| format!("{:6} {}  {}", p.series, p.name, p.subject))
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
        PatchCmd::Plan { id, ffmpeg, series } => match parse_id(&id) {
            Ok(id) => out.result(svc.patch_plan(&id, ffmpeg, &series).await, |p| {
                serde_json::to_string_pretty(p).unwrap_or_default()
            }),
            Err(e) => out.error(&e),
        },
        PatchCmd::Build {
            id,
            ffmpeg,
            apply,
            series,
        } => match parse_id(&id) {
            Ok(id) => {
                let r = svc.patch_build(id, ffmpeg, &series).await;
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
