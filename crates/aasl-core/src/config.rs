use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, IoContext, Result};

pub const CONFIG_VERSION: u32 = 1;

/// AASL catalog: recommended Proton and published patch bundles.
pub const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/Zutyosh/AASL/main/data/catalog.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// Wine prefix holding Somnium. Defaults to Paths::default_prefix().
    pub prefix: Option<PathBuf>,
    /// Proton build as `provider:tag` (`ge:GE-Proton11-5`). None means the
    /// catalog's recommendation.
    pub proton: Option<String>,
    /// Compatibility recipe id. None means the built-in "recommended".
    pub recipe: Option<String>,
    /// Measure each session (crashes, worlds, videos, frame pacing with
    /// MangoHud when installed) into a local report. Nothing is sent.
    pub session_reports: bool,
    /// Where the catalog of Proton recommendations and patch bundles lives.
    pub catalog_url: String,
    /// GitHub token for the release API (higher rate limit). GITHUB_TOKEN
    /// in the environment works too.
    pub github_token: Option<String>,
    /// Wait for the OpenXR runtime manifest before launching instead of
    /// starting in flat mode.
    pub wait_for_headset: bool,
    pub headset_timeout_secs: u64,
    /// When set, replaces OpenXR detection entirely (same rule as
    /// Somnilux's openxr.conf).
    pub openxr_env: Option<BTreeMap<String, String>>,
    /// Extra environment for every launch, applied last.
    pub env: BTreeMap<String, String>,
    pub locale: Option<String>,
    /// Look for a newer AASL at start-up.
    pub check_updates: bool,
    /// `stable` or `beta` (pre-releases too).
    pub update_channel: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            prefix: None,
            proton: None,
            recipe: None,
            session_reports: true,
            catalog_url: DEFAULT_CATALOG_URL.into(),
            github_token: None,
            wait_for_headset: true,
            headset_timeout_secs: 120,
            openxr_env: None,
            env: BTreeMap::new(),
            locale: None,
            check_updates: true,
            update_channel: "stable".into(),
        }
    }
}

impl Config {
    /// Load, returning defaults when the file does not exist.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(Error::io(format!("reading {}", path.display()), e)),
        };
        let mut cfg: Config = toml::from_str(&text).map_err(|e| Error::Parse {
            path: path.into(),
            message: e.to_string(),
        })?;
        cfg.migrate()?;
        Ok(cfg)
    }

    /// Write atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).map_err(|e| Error::Invalid(e.to_string()))?;
        write_atomic(path, text.as_bytes())
    }

    fn migrate(&mut self) -> Result<()> {
        match self.version {
            0 => {
                self.version = CONFIG_VERSION;
                Ok(())
            }
            CONFIG_VERSION => Ok(()),
            v => Err(Error::Invalid(format!(
                "config version {v} is newer than this AASL understands ({CONFIG_VERSION}). Update AASL."
            ))),
        }
    }
}

/// Write a file through a sibling temp file and rename, so readers never see
/// a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).ctx(|| format!("creating {}", dir.display()))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).ctx(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).ctx(|| format!("renaming to {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            Config::load(&d.path().join("none.toml")).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn round_trip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c/config.toml");
        let mut cfg = Config {
            proton: Some("GE-Proton10-4".into()),
            ..Default::default()
        };
        cfg.env.insert("DXVK_HUD".into(), "fps".into());
        cfg.save(&p).unwrap();
        assert_eq!(Config::load(&p).unwrap(), cfg);
    }

    #[test]
    fn future_version_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        std::fs::write(&p, "version = 99").unwrap();
        assert!(Config::load(&p).is_err());
    }

    #[test]
    fn partial_file_fills_defaults() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        std::fs::write(&p, "version = 1\nwait_for_headset = false").unwrap();
        let cfg = Config::load(&p).unwrap();
        assert!(!cfg.wait_for_headset);
        assert_eq!(cfg.headset_timeout_secs, 120);
    }
}
