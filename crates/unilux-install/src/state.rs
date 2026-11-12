use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use unilux_components::ProtonId;
use unilux_core::{Error, Result, write_atomic};

/// What the last install or repair did. Reporting only: every step checks
/// the real filesystem before deciding to skip.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstallState {
    pub schema: u32,
    pub prefix: Option<PathBuf>,
    /// `provider:tag`.
    pub proton: Option<String>,
    pub umu: Option<String>,
    pub patch_bundle: Option<String>,
    pub installed_at: Option<u64>,
    pub steps: BTreeMap<String, u64>,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

impl InstallState {
    pub fn load(path: &Path) -> Result<Self> {
        let mut s: Self = match std::fs::read_to_string(path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| Error::Parse {
                path: path.into(),
                message: e.to_string(),
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(Error::io(format!("reading {}", path.display()), e)),
        };
        // Schema 1 stored a bare GE tag.
        if let Some(p) = &s.proton
            && !p.contains(':')
        {
            s.proton = Some(format!("ge:{p}"));
        }
        Ok(s)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut s = self.clone();
        s.schema = 2;
        let text = serde_json::to_vec_pretty(&s).map_err(|e| Error::Invalid(e.to_string()))?;
        write_atomic(path, &text)
    }

    pub fn is_installed(&self) -> bool {
        self.installed_at.is_some()
    }

    pub fn proton_id(&self) -> Option<ProtonId> {
        self.proton.as_deref().and_then(|p| ProtonId::parse(p).ok())
    }
}
