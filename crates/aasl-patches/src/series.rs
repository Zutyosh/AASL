//! Patch series: the source side of the bundles.

use std::path::Path;

use aasl_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const EMBEDDED_SERIES: &str = include_str!("../../../patches/series.toml");

/// Patch files compiled into the binary, so `aasl patch build` works from
/// an AppImage without a checkout.
pub const EMBEDDED_PATCHES: &[(&str, &str)] = &[
    (
        "wine/0001-crypt32-Add-OID.-prefix-for-unrecognized-RDN-OIDs-in.patch",
        include_str!(
            "../../../patches/wine/0001-crypt32-Add-OID.-prefix-for-unrecognized-RDN-OIDs-in.patch"
        ),
    ),
    (
        "wine/0002-secur32-Request-a-client-certificate-for-ASC_REQ_MUT.patch",
        include_str!(
            "../../../patches/wine/0002-secur32-Request-a-client-certificate-for-ASC_REQ_MUT.patch"
        ),
    ),
    (
        "wine/0003-secur32-Support-resuming-a-parked-handshake-after-SE.patch",
        include_str!(
            "../../../patches/wine/0003-secur32-Support-resuming-a-parked-handshake-after-SE.patch"
        ),
    ),
    (
        "wine/0004-secur32-Implement-SECPKG_ATTR_ISSUER_LIST_EX.patch",
        include_str!(
            "../../../patches/wine/0004-secur32-Implement-SECPKG_ATTR_ISSUER_LIST_EX.patch"
        ),
    ),
    (
        "ge/0001-winedmo-Open-RTSP-streams-by-URL.patch",
        include_str!("../../../patches/ge/0001-winedmo-Open-RTSP-streams-by-URL.patch"),
    ),
    (
        "ge/0002-winedmo-Let-FFmpeg-open-remote-http-s-media-by-URL.patch",
        include_str!(
            "../../../patches/ge/0002-winedmo-Let-FFmpeg-open-remote-http-s-media-by-URL.patch"
        ),
    ),
    (
        "ge/0003-winedmo-Do-not-block-in-IMFSchemeHandler-BeginCreate.patch",
        include_str!(
            "../../../patches/ge/0003-winedmo-Do-not-block-in-IMFSchemeHandler-BeginCreate.patch"
        ),
    ),
    (
        "ge/0004-kernelbase-Redirect-yt-dlp.exe-and-allow-appending-e.patch",
        include_str!(
            "../../../patches/ge/0004-kernelbase-Redirect-yt-dlp.exe-and-allow-appending-e.patch"
        ),
    ),
    ("series.toml", EMBEDDED_SERIES),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    Upstream,
    Proton,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Series {
    pub id: String,
    pub title: String,
    pub dir: String,
    pub base: Base,
    #[serde(default)]
    pub upstream_repo: Option<String>,
    #[serde(default)]
    pub upstream_commit: Option<String>,
    #[serde(default)]
    pub requires_prep: Option<String>,
    pub dlls: Vec<String>,
    #[serde(default)]
    pub unix_libs: Vec<String>,
}

#[derive(Deserialize)]
struct File {
    series: Vec<Series>,
}

pub fn parse(text: &str) -> Result<Vec<Series>> {
    let f: File = toml::from_str(text).map_err(|e| Error::Invalid(format!("series.toml: {e}")))?;
    for s in &f.series {
        if s.base == Base::Upstream && (s.upstream_repo.is_none() || s.upstream_commit.is_none()) {
            return Err(Error::Invalid(format!(
                "series {} builds from upstream but has no upstream_repo/upstream_commit",
                s.id
            )));
        }
    }
    Ok(f.series)
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchFile {
    pub series: String,
    pub name: String,
    pub subject: String,
    pub sha256: String,
}

/// Every patch in a patches directory, with its subject line and hash.
pub fn list(dir: &Path) -> Result<Vec<PatchFile>> {
    let series = parse(
        &std::fs::read_to_string(dir.join("series.toml"))
            .map_err(|e| Error::io("reading series.toml", e))?,
    )?;
    let mut out = Vec::new();
    for s in series {
        let mut files: Vec<_> = std::fs::read_dir(dir.join(&s.dir))
            .map_err(|e| Error::io(format!("reading {}", s.dir), e))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "patch"))
            .collect();
        files.sort();
        for p in files {
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let subject = text
                .lines()
                .find_map(|l| l.strip_prefix("Subject: "))
                .map(|s| {
                    s.trim_start_matches(|c| c != ']')
                        .trim_start_matches(']')
                        .trim()
                        .to_string()
                })
                .unwrap_or_default();
            out.push(PatchFile {
                series: s.id.clone(),
                name: p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                subject,
                sha256: hex::encode(Sha256::digest(text.as_bytes())),
            });
        }
    }
    Ok(out)
}

/// Write the embedded patches to `dir` (used when no checkout is around).
pub fn materialise(dir: &Path) -> Result<()> {
    for (rel, text) in EMBEDDED_PATCHES {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
        }
        std::fs::write(&p, text).map_err(|e| Error::io(format!("writing {}", p.display()), e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_series_and_listing() {
        let s = parse(EMBEDDED_SERIES).unwrap();
        assert_eq!(s.len(), 2);
        let d = tempfile::tempdir().unwrap();
        materialise(d.path()).unwrap();
        let l = list(d.path()).unwrap();
        assert_eq!(l.len(), 8);
        assert_eq!(l[0].series, "tls");
        assert!(
            l.iter()
                .any(|p| p.subject.contains("SECPKG_ATTR_ISSUER_LIST_EX"))
        );
    }
}
