//! Self-update from GitHub releases.

use std::path::{Path, PathBuf};

use aasl_components::Checksum;
use aasl_core::{Error, IoContext, Result};
use serde::{Deserialize, Serialize};

pub const REPO: &str = "Zutyosh/AASL";
pub const APPIMAGE_ASSET: &str = "AASL-x86_64.AppImage";
pub const TARBALL_ASSET: &str = "aasl-x86_64-linux.tar.gz";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Pre-release number (`0.2.0-beta.3` is 3); None sorts after any.
    pub pre: Option<u64>,
}

impl Version {
    /// `v1.2.3`, `1.2.3` or `1.2.3-beta.4` (any pre-release label).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().strip_prefix('v').unwrap_or(s.trim());
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p.rsplit('.').next()?.parse().ok()?)),
            None => (s, None),
        };
        let mut it = core.split('.');
        let v = Self {
            major: it.next()?.parse().ok()?,
            minor: it.next()?.parse().ok()?,
            patch: it.next()?.parse().ok()?,
            pre,
        };
        it.next().is_none().then_some(v)
    }

    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("crate version is semver")
    }
}

impl Ord for Version {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(o.major, o.minor, o.patch))
            .then(match (self.pre, o.pre) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(a), Some(b)) => a.cmp(&b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(p) = self.pre {
            write!(f, "-beta.{p}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub version: Version,
    pub prerelease: bool,
    pub notes: String,
    pub url: String,
    pub assets: Vec<Asset>,
}

/// How this copy of AASL was installed, which decides how it updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallKind {
    /// Running from an AppImage: replace that file.
    AppImage { path: PathBuf },
    /// Binaries in a directory the user can write: replace them.
    Portable { dir: PathBuf },
    /// Installed by a package manager (or not writable): only notify.
    Managed { dir: PathBuf },
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
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// AASL releases from a GitHub `/releases` page. Drafts and tags that
/// are not versions (patch-bundle releases share the repository) are
/// dropped.
pub fn parse_releases(json: &str) -> Result<Vec<Release>> {
    let raw: Vec<GhRelease> =
        serde_json::from_str(json).map_err(|e| Error::Invalid(format!("GitHub releases: {e}")))?;
    Ok(raw
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            Some(Release {
                version: Version::parse(&r.tag_name)?,
                tag: r.tag_name,
                prerelease: r.prerelease,
                notes: r.body.unwrap_or_default(),
                url: r.html_url,
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
        })
        .collect())
}

/// The newest release on `channel` that is newer than `current`.
pub fn newest(releases: &[Release], current: Version, channel: Channel) -> Option<Release> {
    releases
        .iter()
        .filter(|r| channel == Channel::Beta || (!r.prerelease && r.version.pre.is_none()))
        .filter(|r| r.version > current)
        .max_by_key(|r| r.version)
        .cloned()
}

/// Directories that belong to the system package manager.
const MANAGED_PREFIXES: &[&str] = &[
    "/usr/", "/bin/", "/sbin/", "/nix/", "/gnu/", "/app/", "/snap/",
];

pub fn detect_install(
    appimage_env: Option<&str>,
    exe: &Path,
    writable: impl Fn(&Path) -> bool,
) -> InstallKind {
    if let Some(p) = appimage_env.filter(|p| !p.is_empty()) {
        return InstallKind::AppImage {
            path: PathBuf::from(p),
        };
    }
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let s = format!("{}/", dir.display());
    if MANAGED_PREFIXES.iter().any(|m| s.starts_with(m)) || !writable(&dir) {
        InstallKind::Managed { dir }
    } else {
        InstallKind::Portable { dir }
    }
}

/// What an update changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Applied {
    pub version: Version,
    pub replaced: Vec<PathBuf>,
    /// The running process is still the old version until restarted.
    pub restart_needed: bool,
}

pub struct Updater {
    pub api_base: String,
    pub repo: String,
    pub token: Option<String>,
    pub downloader: aasl_components::Downloader,
}

impl Updater {
    pub fn new() -> Self {
        Self {
            api_base: "https://api.github.com".into(),
            repo: REPO.into(),
            token: std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()),
            downloader: aasl_components::Downloader::new(),
        }
    }

    /// Ask GitHub for releases and return the one to update to, if any.
    pub async fn check(&self, current: Version, channel: Channel) -> Result<Option<Release>> {
        let url = format!("{}/repos/{}/releases?per_page=30", self.api_base, self.repo);
        let mut req = self
            .downloader
            .client()
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let text = req
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::Network(format!("{url}: {e}")))?
            .text()
            .await
            .map_err(|e| Error::Network(format!("{url}: {e}")))?;
        Ok(newest(&parse_releases(&text)?, current, channel))
    }

    /// The asset and its expected sha256: GitHub's digest, else a
    /// `<asset>.sha256` file published next to it. No checksum, no install.
    async fn verified_asset<'a>(
        &self,
        release: &'a Release,
        name: &str,
    ) -> Result<(&'a Asset, Checksum)> {
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| Error::Invalid(format!("release {} has no {name}", release.tag)))?;
        if let Some(h) = asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
        {
            return Ok((asset, Checksum::Sha256(h.to_lowercase())));
        }
        if let Some(sum) = release
            .assets
            .iter()
            .find(|a| a.name == format!("{name}.sha256"))
        {
            let text = self.downloader.text(&sum.url).await?;
            if let Some(c) = Checksum::from_sum_file(&text, name) {
                return Ok((asset, c));
            }
        }
        Err(Error::Invalid(format!(
            "{name} in {} has no published checksum; refusing to install it",
            release.tag
        )))
    }

    /// Download, verify and install `release` over this installation.
    /// Files are written next to their target and renamed into place, so
    /// a failed download never leaves a broken AASL behind.
    pub async fn apply(
        &self,
        release: &Release,
        kind: &InstallKind,
        progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<Applied> {
        let replaced = match kind {
            InstallKind::Managed { dir } => {
                return Err(Error::Invalid(format!(
                    "AASL in {} was installed by your package manager; update it with that (AUR, distro package).",
                    dir.display()
                )));
            }
            InstallKind::AppImage { path } => {
                let (asset, sum) = self.verified_asset(release, APPIMAGE_ASSET).await?;
                let new = path.with_extension("AppImage.new");
                self.downloader
                    .fetch(&asset.url, &new, Some(&sum), progress)
                    .await?;
                make_executable(&new)?;
                let old = PathBuf::from(format!("{}.old", path.display()));
                std::fs::copy(path, &old).ctx(|| format!("backing up {}", path.display()))?;
                std::fs::rename(&new, path).ctx(|| format!("replacing {}", path.display()))?;
                vec![path.clone()]
            }
            InstallKind::Portable { dir } => {
                let (asset, sum) = self.verified_asset(release, TARBALL_ASSET).await?;
                let tgz = dir.join(format!(".{TARBALL_ASSET}"));
                self.downloader
                    .fetch(&asset.url, &tgz, Some(&sum), progress)
                    .await?;
                let dir = dir.clone();
                tokio::task::spawn_blocking(move || install_tarball(&tgz, &dir))
                    .await
                    .map_err(|e| Error::Invalid(e.to_string()))??
            }
        };
        Ok(Applied {
            version: release.version,
            replaced,
            restart_needed: true,
        })
    }
}

/// Binaries shipped in the tarball.
const BINARIES: &[&str] = &["aasl", "aasl-gui"];

fn install_tarball(tgz: &Path, dir: &Path) -> Result<Vec<PathBuf>> {
    let staging = aasl_components::extract::extract_to_staging(tgz, dir)?;
    let _ = std::fs::remove_file(tgz);
    let mut out = Vec::new();
    for name in BINARIES {
        let src = staging.path().join(name);
        if !src.is_file() {
            continue;
        }
        let dest = dir.join(name);
        let tmp = dir.join(format!(".{name}.new"));
        std::fs::copy(&src, &tmp).ctx(|| format!("writing {}", tmp.display()))?;
        make_executable(&tmp)?;
        std::fs::rename(&tmp, &dest).ctx(|| format!("replacing {}", dest.display()))?;
        out.push(dest);
    }
    if out.is_empty() {
        return Err(Error::Invalid(format!(
            "{TARBALL_ASSET} contains no AASL binaries"
        )));
    }
    Ok(out)
}

fn make_executable(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))
        .ctx(|| format!("chmod {}", p.display()))
}

impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod apply_tests;
