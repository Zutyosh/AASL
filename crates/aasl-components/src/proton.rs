//! Proton manager: list upstream releases, pick the right build for this
//! CPU, download with the upstream digest, install side by side.
//!
//! The model follows ProtonPlus (Vysp3r/ProtonPlus): a provider is plain
//! configuration (repository, asset name templates, CPU variants) and one
//! generic GitHub release source serves them all. This is a clean Rust
//! implementation of that design, not a port of its GPL code.

use std::path::{Path, PathBuf};

use aasl_core::{Error, IoContext, Result, write_atomic};
use serde::{Deserialize, Serialize};

use crate::download::{Checksum, Downloader};
use crate::extract::{extract_to_staging, move_into_place};

/// x86-64 micro-architecture level, as used by CachyOS builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CpuLevel {
    X86_64,
    X86_64V2,
    X86_64V3,
    X86_64V4,
}

impl CpuLevel {
    /// Read /proc/cpuinfo flags.
    pub fn detect() -> Self {
        let info = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        Self::from_flags(info.lines().find(|l| l.starts_with("flags")).unwrap_or(""))
    }

    pub fn from_flags(line: &str) -> Self {
        let has = |f: &str| line.split_whitespace().any(|w| w == f);
        let v2 = ["cx16", "lahf_lm", "popcnt", "sse4_1", "sse4_2", "ssse3"]
            .iter()
            .all(|f| has(f));
        let v3 = v2
            && [
                "avx", "avx2", "bmi1", "bmi2", "f16c", "fma", "abm", "movbe", "xsave",
            ]
            .iter()
            .all(|f| has(f));
        let v4 = v3
            && ["avx512f", "avx512bw", "avx512cd", "avx512dq", "avx512vl"]
                .iter()
                .all(|f| has(f));
        match (v2, v3, v4) {
            (_, _, true) => Self::X86_64V4,
            (_, true, _) => Self::X86_64V3,
            (true, _, _) => Self::X86_64V2,
            _ => Self::X86_64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Variant {
    pub id: &'static str,
    /// Asset name with `$tag` substituted, without the archive extension.
    pub template: &'static str,
    pub min_level: CpuLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Provider {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// GitHub owner/repo.
    pub repo: &'static str,
    /// Most specific first; the first one this CPU can run wins.
    pub variants: &'static [Variant],
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "ge",
        title: "GE-Proton",
        description: "GloriousEggroll's Proton with media fixes. The patches AASL ships target it first.",
        repo: "GloriousEggroll/proton-ge-custom",
        variants: &[Variant {
            id: "x86-64",
            template: "$tag-x86_64",
            min_level: CpuLevel::X86_64,
        }],
    },
    Provider {
        id: "rtsp",
        title: "GE-Proton RTSP",
        description: "GE-Proton with RTSP and extra media patches, useful for video streams in VR worlds.",
        repo: "SpookySkeletons/proton-rtsp",
        variants: &[Variant {
            id: "x86-64",
            template: "$tag",
            min_level: CpuLevel::X86_64,
        }],
    },
    Provider {
        id: "cachyos",
        title: "Proton-CachyOS",
        description: "CachyOS's Proton, built with newer compilers and optional x86-64-v3 optimisations.",
        repo: "CachyOS/proton-cachyos",
        variants: &[
            Variant {
                id: "x86-64-v3",
                template: "proton-$tag-x86_64_v3",
                min_level: CpuLevel::X86_64V3,
            },
            Variant {
                id: "x86-64",
                template: "proton-$tag-x86_64",
                min_level: CpuLevel::X86_64,
            },
        ],
    },
    Provider {
        id: "em",
        title: "Proton-EM",
        description: "Proton with HDR, Wayland and newer DXVK/VKD3D work by Etaash Mathamsetty.",
        repo: "BananaWorks07/Proton",
        variants: &[Variant {
            id: "x86-64",
            template: "proton-$tag",
            min_level: CpuLevel::X86_64,
        }],
    },
    Provider {
        id: "sarek",
        title: "Proton-Sarek",
        description: "Proton tuned for older GPUs (DXVK for Vulkan 1.1 and earlier).",
        repo: "pythonlover02/Proton-Sarek",
        variants: &[Variant {
            id: "x86-64",
            template: "$tag",
            min_level: CpuLevel::X86_64,
        }],
    },
];

const ARCHIVE_EXTS: [&str; 3] = [".tar.gz", ".tar.xz", ".tar.zst"];

/// Last resort when no template matches (a provider renamed its assets):
/// the first x86-64 archive, like ProtonPlus's position-based pick.
fn fallback_asset(assets: &[Asset]) -> Option<&Asset> {
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        ARCHIVE_EXTS.iter().any(|e| n.ends_with(e))
            && !["aarch64", "arm64", "debug", "-src", "source"]
                .iter()
                .any(|bad| n.contains(bad))
    })
}

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// A specific Proton build: `provider:tag`, e.g. `ge:GE-Proton11-5`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProtonId {
    pub provider: String,
    pub tag: String,
}

impl ProtonId {
    pub fn new(provider: &str, tag: &str) -> Self {
        Self {
            provider: provider.into(),
            tag: tag.into(),
        }
    }

    /// Parse `provider:tag`. A bare tag is read as GE for compatibility
    /// with older configs.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        let (p, t) = match s.split_once(':') {
            Some((p, t)) => (p, t),
            None => ("ge", s),
        };
        if t.is_empty() || provider(p).is_none() {
            return Err(Error::Invalid(format!(
                "unknown Proton '{s}'. Use provider:tag, providers: {}",
                PROVIDERS
                    .iter()
                    .map(|p| p.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        Ok(Self::new(p, t))
    }

    /// Major Wine/Proton version, when the tag says it (GE-Proton11-5 is
    /// 11, cachyos-11.0-20261015-slr is 11).
    pub fn major(&self) -> Option<u32> {
        let digits: String = self
            .tag
            .trim_start_matches(|c: char| !c.is_ascii_digit())
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    }
}

impl std::fmt::Display for ProtonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.provider, self.tag)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Upstream digest as published by GitHub (`sha256:...`).
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub provider: String,
    pub tag: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn id(&self) -> ProtonId {
        ProtonId::new(&self.provider, &self.tag)
    }

    /// The archive to install on a CPU of `level`, and the sha512sum file
    /// next to it if there is one.
    pub fn pick(&self, level: CpuLevel) -> Option<(Variant, &Asset, Option<&Asset>)> {
        let p = provider(&self.provider)?;
        for v in p.variants.iter().filter(|v| v.min_level <= level) {
            let stem = v.template.replace("$tag", &self.tag);
            let archive = self.assets.iter().find(|a| {
                ARCHIVE_EXTS
                    .iter()
                    .any(|ext| a.name == format!("{stem}{ext}"))
            });
            if let Some(a) = archive {
                let sum = self
                    .assets
                    .iter()
                    .find(|s| s.name == format!("{stem}.sha512sum"));
                return Some((*v, a, sum));
            }
        }
        let v = p
            .variants
            .iter()
            .find(|v| v.min_level == CpuLevel::X86_64)?;
        fallback_asset(&self.assets).map(|a| (*v, a, None))
    }
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// Parse a GitHub `/releases` page.
pub fn parse_releases(provider: &str, json: &str) -> Result<Vec<Release>> {
    let raw: Vec<GhRelease> = serde_json::from_str(json)
        .map_err(|e| Error::Invalid(format!("GitHub releases for {provider}: {e}")))?;
    Ok(raw
        .into_iter()
        .filter(|r| !r.draft)
        .map(|r| Release {
            provider: provider.into(),
            tag: r.tag_name,
            published_at: r.published_at,
            prerelease: r.prerelease,
            assets: r
                .assets
                .into_iter()
                .map(|a| Asset {
                    name: a.name,
                    url: a.browser_download_url,
                    size: a.size,
                    digest: a.digest,
                })
                .collect(),
        })
        .collect())
}

/// What is recorded inside each installed Proton directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledProton {
    pub id: ProtonId,
    pub variant: String,
    pub asset: String,
    pub installed_at: u64,
    #[serde(skip)]
    pub path: PathBuf,
}

const MARKER: &str = ".aasl-proton.json";

pub struct ProtonManager {
    pub root: PathBuf,
    pub cache_dir: PathBuf,
    pub downloads: PathBuf,
    pub api_base: String,
    pub token: Option<String>,
    pub downloader: Downloader,
}

#[derive(Serialize, Deserialize)]
struct CachedList {
    fetched_at: u64,
    releases: Vec<Release>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Cached release lists are reused for this long.
const CACHE_TTL: u64 = 3600;

impl ProtonManager {
    pub fn new(root: PathBuf, cache_dir: PathBuf, downloads: PathBuf) -> Self {
        Self {
            root,
            cache_dir,
            downloads,
            api_base: "https://api.github.com".into(),
            token: std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()),
            downloader: Downloader::new(),
        }
    }

    pub fn dir_for(&self, id: &ProtonId) -> PathBuf {
        self.root.join(&id.provider).join(&id.tag)
    }

    /// Releases of one provider, newest first. Uses a one-hour cache and
    /// falls back to a stale cache when GitHub cannot be reached.
    pub async fn releases(&self, provider_id: &str, refresh: bool) -> Result<Vec<Release>> {
        let p = provider(provider_id)
            .ok_or_else(|| Error::Invalid(format!("unknown Proton provider '{provider_id}'")))?;
        let cache = self
            .cache_dir
            .join(format!("proton-releases-{}.json", p.id));
        let cached: Option<CachedList> = std::fs::read_to_string(&cache)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        if !refresh
            && let Some(c) = &cached
            && now().saturating_sub(c.fetched_at) < CACHE_TTL
        {
            return Ok(c.releases.clone());
        }
        let url = format!("{}/repos/{}/releases?per_page=50", self.api_base, p.repo);
        let mut req = self
            .downloader
            .client()
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let fetched = async {
            let r = req
                .send()
                .await
                .map_err(|e| Error::Network(format!("{url}: {e}")))?;
            if r.status() == reqwest::StatusCode::FORBIDDEN
                || r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
            {
                return Err(Error::Network(
                    "GitHub API rate limit reached. Wait an hour or set GITHUB_TOKEN.".into(),
                ));
            }
            let r = r
                .error_for_status()
                .map_err(|e| Error::Network(format!("{url}: {e}")))?;
            let text = r
                .text()
                .await
                .map_err(|e| Error::Network(format!("{url}: {e}")))?;
            parse_releases(p.id, &text)
        }
        .await;
        match fetched {
            Ok(releases) => {
                let body = serde_json::to_vec(&CachedList {
                    fetched_at: now(),
                    releases: releases.clone(),
                })
                .unwrap_or_default();
                let _ = write_atomic(&cache, &body);
                Ok(releases)
            }
            Err(e) => match cached {
                Some(c) => Ok(c.releases),
                None => Err(e),
            },
        }
    }

    pub async fn find(&self, id: &ProtonId) -> Result<Release> {
        let list = self.releases(&id.provider, false).await?;
        if let Some(r) = list.iter().find(|r| r.tag == id.tag) {
            return Ok(r.clone());
        }
        let list = self.releases(&id.provider, true).await?;
        list.into_iter().find(|r| r.tag == id.tag).ok_or_else(|| {
            Error::Invalid(format!(
                "{id} is not among the latest {} releases",
                id.provider
            ))
        })
    }

    pub fn installed(&self) -> Vec<InstalledProton> {
        let mut out = Vec::new();
        for p in PROVIDERS {
            let Ok(rd) = std::fs::read_dir(self.root.join(p.id)) else {
                continue;
            };
            for e in rd.flatten() {
                let path = e.path();
                if let Some(mut rec) = std::fs::read_to_string(path.join(MARKER))
                    .ok()
                    .and_then(|t| serde_json::from_str::<InstalledProton>(&t).ok())
                {
                    rec.path = path;
                    out.push(rec);
                }
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn is_installed(&self, id: &ProtonId) -> bool {
        let d = self.dir_for(id);
        d.join("proton").exists() && d.join(MARKER).exists()
    }

    /// Download, verify and install one release. Idempotent.
    pub async fn install(
        &self,
        release: &Release,
        level: CpuLevel,
        mut progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<PathBuf> {
        let id = release.id();
        let dest = self.dir_for(&id);
        if self.is_installed(&id) {
            return Ok(dest);
        }
        let (variant, asset, sum_asset) = release
            .pick(level)
            .ok_or_else(|| Error::Invalid(format!("{id} has no build for this CPU ({level:?})")))?;
        let checksum = match asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
        {
            Some(h) => Some(Checksum::Sha256(h.to_lowercase())),
            None => match sum_asset {
                Some(s) => {
                    Checksum::from_sum_file(&self.downloader.text(&s.url).await?, &asset.name)
                }
                None => None,
            },
        };
        let Some(checksum) = checksum else {
            return Err(Error::Invalid(format!(
                "{} has no published checksum; refusing to install it unverified",
                asset.name
            )));
        };
        let dl = self.downloads.join(&asset.name);
        self.downloader
            .fetch(&asset.url, &dl, Some(&checksum), &mut progress)
            .await?;
        let parent = dest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.clone());
        let rec = InstalledProton {
            id: id.clone(),
            variant: variant.id.into(),
            asset: asset.name.clone(),
            installed_at: now(),
            path: dest.clone(),
        };
        let dest2 = dest.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let st = extract_to_staging(&dl, &parent)?;
            let top = find_proton_root(st.path())?;
            if dest2.exists() {
                std::fs::remove_dir_all(&dest2)
                    .ctx(|| format!("removing incomplete {}", dest2.display()))?;
            }
            move_into_place(&top, &dest2)?;
            let body = serde_json::to_vec_pretty(&rec).unwrap_or_default();
            write_atomic(&dest2.join(MARKER), &body)?;
            let _ = std::fs::remove_file(&dl);
            Ok(())
        })
        .await
        .map_err(|e| Error::Invalid(e.to_string()))??;
        Ok(dest)
    }

    pub fn remove(&self, id: &ProtonId) -> Result<()> {
        let d = self.dir_for(id);
        if !d.join(MARKER).exists() {
            return Err(Error::Invalid(format!("{id} is not installed by AASL")));
        }
        std::fs::remove_dir_all(&d).ctx(|| format!("removing {}", d.display()))
    }
}

/// The directory inside an extracted archive that holds the `proton`
/// script: the archive root itself or its single top-level directory.
fn find_proton_root(dir: &Path) -> Result<PathBuf> {
    if dir.join("proton").is_file() {
        return Ok(dir.to_path_buf());
    }
    let subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ctx(|| format!("reading {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    subdirs
        .into_iter()
        .find(|p| p.join("proton").is_file())
        .ok_or_else(|| {
            Error::Invalid(
                "the archive does not contain a Proton build (no 'proton' script)".into(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GE_JSON: &str = r#"[{"tag_name":"GE-Proton11-12","prerelease":false,"draft":false,"published_at":"2026-10-01T00:00:00Z","assets":[
        {"name":"GE-Proton11-12-aarch64.tar.gz","browser_download_url":"https://x/a","size":1,"digest":"sha256:aa"},
        {"name":"GE-Proton11-12-x86_64.tar.gz","browser_download_url":"https://x/b","size":2,"digest":"sha256:bb"},
        {"name":"GE-Proton11-12-x86_64.sha512sum","browser_download_url":"https://x/c","size":3}]},
        {"tag_name":"draft","draft":true,"assets":[]}]"#;

    const CACHY_JSON: &str = r#"[{"tag_name":"cachyos-11.0-20261015-slr","assets":[
        {"name":"proton-cachyos-11.0-20261015-slr-x86_64.tar.xz","browser_download_url":"https://x/1","size":1},
        {"name":"proton-cachyos-11.0-20261015-slr-x86_64_v3.tar.xz","browser_download_url":"https://x/3","size":1},
        {"name":"proton-cachyos-11.0-20261015-slr-x86_64_v3.sha512sum","browser_download_url":"https://x/3s","size":1}]}]"#;

    #[test]
    fn ids() {
        let id = ProtonId::parse("cachyos:cachyos-11.0-20261015-slr").unwrap();
        assert_eq!(id.major(), Some(11));
        assert_eq!(
            ProtonId::parse("GE-Proton9-27").unwrap(),
            ProtonId::new("ge", "GE-Proton9-27")
        );
        assert_eq!(ProtonId::parse("GE-Proton9-27").unwrap().major(), Some(9));
        assert!(ProtonId::parse("valve:9").is_err());
        assert_eq!(
            ProtonId::new("ge", "GE-Proton11-5").to_string(),
            "ge:GE-Proton11-5"
        );
    }

    #[test]
    fn cpu_levels() {
        assert_eq!(CpuLevel::from_flags("flags : fpu sse2"), CpuLevel::X86_64);
        let v3 = "flags : cx16 lahf_lm popcnt sse4_1 sse4_2 ssse3 avx avx2 bmi1 bmi2 f16c fma abm movbe xsave";
        assert_eq!(CpuLevel::from_flags(v3), CpuLevel::X86_64V3);
    }

    #[test]
    fn ge_picks_x86_64_with_digest() {
        let r = parse_releases("ge", GE_JSON).unwrap();
        assert_eq!(r.len(), 1, "drafts dropped");
        let (v, a, s) = r[0].pick(CpuLevel::X86_64V3).unwrap();
        assert_eq!(v.id, "x86-64");
        assert_eq!(a.name, "GE-Proton11-12-x86_64.tar.gz");
        assert_eq!(a.digest.as_deref(), Some("sha256:bb"));
        assert!(s.is_some());
    }

    #[test]
    fn cachyos_variant_follows_cpu() {
        let r = &parse_releases("cachyos", CACHY_JSON).unwrap()[0];
        assert_eq!(r.pick(CpuLevel::X86_64V3).unwrap().0.id, "x86-64-v3");
        assert_eq!(r.pick(CpuLevel::X86_64V2).unwrap().0.id, "x86-64");
    }

    #[test]
    fn proton_root_detection() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("GE-Proton11-12")).unwrap();
        std::fs::write(d.path().join("GE-Proton11-12/proton"), "").unwrap();
        assert_eq!(
            find_proton_root(d.path()).unwrap(),
            d.path().join("GE-Proton11-12")
        );
        let e = tempfile::tempdir().unwrap();
        assert!(find_proton_root(e.path()).is_err());
    }
}
