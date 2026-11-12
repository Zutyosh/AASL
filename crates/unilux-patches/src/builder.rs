//! Building a patch bundle for any Proton build.
//!
//! The build itself runs in `tools/patch-builder/build.sh`, normally in a
//! container so the host needs only podman or docker. This module resolves
//! everything the script needs (which Wine commit the Proton tag uses,
//! which patches to apply) and assembles the command, so the GUI, the CLI
//! and agents all start the same build.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use unilux_components::{Downloader, ProtonId, proton};
use unilux_core::exec::CommandSpec;
use unilux_core::{Error, Result};

use crate::ARCHIVE_FORMAT;

pub const IMAGE: &str = "localhost/unilux-patch-builder:1";
pub const CONTAINERFILE: &str = include_str!("../../../tools/patch-builder/Containerfile");
pub const BUILD_SCRIPT: &str = include_str!("../../../tools/patch-builder/build.sh");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Podman,
    Docker,
    /// Run the script directly; the host must have the toolchain.
    Host,
}

impl Engine {
    pub fn detect(find: impl Fn(&str) -> bool) -> Option<Self> {
        if find("podman") {
            Some(Self::Podman)
        } else if find("docker") {
            Some(Self::Docker)
        } else {
            None
        }
    }
    fn bin(self) -> &'static str {
        match self {
            Self::Podman => "podman",
            Self::Docker => "docker",
            Self::Host => "bash",
        }
    }
}

/// The Wine tree a Proton tag is built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WineSource {
    pub repo: String,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildPlan {
    pub proton: ProtonId,
    pub proton_repo: String,
    pub wine: WineSource,
    /// Group ids from the recipe's materialised series.toml.
    pub series: Vec<String>,
    /// The recipe, materialised (see `recipe::Plan::materialise`).
    pub patches_dir: PathBuf,
    pub bundle_name: String,
    pub work_dir: PathBuf,
    pub out_dir: PathBuf,
    pub with_ffmpeg: bool,
    pub engine: Engine,
}

#[derive(Deserialize)]
struct Contents {
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    #[serde(default)]
    submodule_git_url: Option<String>,
}

/// Ask GitHub which Wine commit a Proton tag pins as its `wine` submodule.
pub async fn resolve_wine(
    dl: &Downloader,
    api: &str,
    id: &ProtonId,
) -> Result<(String, WineSource)> {
    let p = proton::provider(&id.provider)
        .ok_or_else(|| Error::Invalid(format!("unknown provider {}", id.provider)))?;
    let url = format!("{api}/repos/{}/contents/wine?ref={}", p.repo, id.tag);
    let text = dl.text(&url).await?;
    let c: Contents =
        serde_json::from_str(&text).map_err(|e| Error::Invalid(format!("{url}: {e}")))?;
    if c.kind != "submodule" {
        return Err(Error::Invalid(format!(
            "{} at {} has no wine submodule",
            p.repo, id.tag
        )));
    }
    let repo = c
        .submodule_git_url
        .map(|u| u.replace("git@github.com:", "https://github.com/"))
        .ok_or_else(|| Error::Invalid("submodule without URL".into()))?;
    Ok((
        format!("https://github.com/{}", p.repo),
        WineSource {
            repo,
            commit: c.sha,
        },
    ))
}

impl BuildPlan {
    fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("PROVIDER", self.proton.provider.clone()),
            ("PROTON_TAG", self.proton.tag.clone()),
            ("PROTON_REPO", self.proton_repo.clone()),
            ("WINE_REPO", self.wine.repo.clone()),
            ("WINE_COMMIT", self.wine.commit.clone()),
            ("SERIES", self.series.join(" ")),
            ("ARCHIVE_FORMAT", ARCHIVE_FORMAT.to_string()),
            (
                "WITH_FFMPEG",
                if self.with_ffmpeg { "1" } else { "0" }.into(),
            ),
            ("BUNDLE_NAME", self.bundle_name.clone()),
        ]
    }

    /// Command that builds the container image (skipped by the caller when
    /// the image already exists).
    pub fn image_command(&self, context: &Path) -> Option<CommandSpec> {
        match self.engine {
            Engine::Host => None,
            e => Some(
                CommandSpec::new(e.bin())
                    .arg("build")
                    .arg("-t")
                    .arg(IMAGE)
                    .arg(context.display().to_string()),
            ),
        }
    }

    /// Command that runs the build.
    pub fn command(&self, script_dir: &Path) -> CommandSpec {
        match self.engine {
            Engine::Host => {
                let mut c =
                    CommandSpec::new("bash").arg(script_dir.join("build.sh").display().to_string());
                for (k, v) in self.env() {
                    c = c.env(k, v);
                }
                c.env("PATCHES_DIR", self.patches_dir.display().to_string())
                    .env("WORK_DIR", self.work_dir.display().to_string())
                    .env("OUT_DIR", self.out_dir.display().to_string())
            }
            e => {
                let mut c = CommandSpec::new(e.bin()).arg("run").arg("--rm");
                for (k, v) in self.env() {
                    c = c.arg("-e").arg(format!("{k}={v}"));
                }
                c.arg("-e")
                    .arg("PATCHES_DIR=/patches")
                    .arg("-e")
                    .arg("WORK_DIR=/work")
                    .arg("-e")
                    .arg("OUT_DIR=/out")
                    .arg("-v")
                    .arg(format!("{}:/patches:ro,Z", self.patches_dir.display()))
                    .arg("-v")
                    .arg(format!("{}:/work:Z", self.work_dir.display()))
                    .arg("-v")
                    .arg(format!("{}:/out:Z", self.out_dir.display()))
                    .arg(IMAGE)
            }
        }
    }
}

/// File name of the bundle a recipe builds for `id` (`key` from
/// `recipe::Plan::bundle_key`).
pub fn bundle_name(id: &ProtonId, key: &str) -> String {
    format!("unilux-{}-{}-{key}.tar.gz", id.provider, id.tag)
}

/// Where that bundle ends up, under Unilux's cache.
pub fn built_bundle(cache_dir: &Path, id: &ProtonId, key: &str) -> PathBuf {
    cache_dir
        .join("patch-builder/out")
        .join(bundle_name(id, key))
}

/// Write the Containerfile and build script to `dir` (the build context).
pub fn materialise_tools(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .map_err(|e| Error::io(format!("creating {}", dir.display()), e))?;
    std::fs::write(dir.join("Containerfile"), CONTAINERFILE)
        .map_err(|e| Error::io("writing Containerfile", e))?;
    std::fs::write(dir.join("build.sh"), BUILD_SCRIPT)
        .map_err(|e| Error::io("writing build.sh", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(engine: Engine) -> BuildPlan {
        BuildPlan {
            proton: ProtonId::new("ge", "GE-Proton11-12"),
            proton_repo: "https://github.com/GloriousEggroll/proton-ge-custom".into(),
            wine: WineSource {
                repo: "https://github.com/ValveSoftware/wine".into(),
                commit: "abc".into(),
            },
            series: vec!["upstream".into(), "proton".into()],
            patches_dir: "/p".into(),
            bundle_name: "b.tar.gz".into(),
            work_dir: "/w".into(),
            out_dir: "/o".into(),
            with_ffmpeg: true,
            engine,
        }
    }

    #[test]
    fn container_command_mounts_and_env() {
        let c = plan(Engine::Podman).command(Path::new("/t"));
        assert_eq!(c.program, PathBuf::from("podman"));
        assert!(c.args.contains(&"WINE_COMMIT=abc".to_string()));
        assert!(c.args.contains(&"SERIES=upstream proton".to_string()));
        assert!(c.args.contains(&"/p:/patches:ro,Z".to_string()));
        assert_eq!(c.args.last().unwrap(), IMAGE);
    }

    #[test]
    fn host_command_uses_env() {
        let c = plan(Engine::Host).command(Path::new("/t"));
        assert_eq!(c.env["OUT_DIR"], "/o");
        assert_eq!(c.env["BUNDLE_NAME"], "b.tar.gz");
        assert_eq!(
            bundle_name(&ProtonId::new("ge", "GE-Proton11-12"), "k"),
            "unilux-ge-GE-Proton11-12-k.tar.gz"
        );
        assert!(plan(Engine::Host).image_command(Path::new("/t")).is_none());
    }

    #[test]
    fn engine_detection() {
        assert_eq!(Engine::detect(|b| b == "docker"), Some(Engine::Docker));
        assert_eq!(Engine::detect(|_| false), None);
    }
}
