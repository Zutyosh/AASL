//! Installing a patch bundle into a Proton build, and checking later that
//! nothing (a Proton update, a manual edit) has replaced the files.

use std::path::{Path, PathBuf};

use aasl_core::{Error, IoContext, Result, write_atomic};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Bundle layout this AASL installs. Matched exactly: a number names a
/// shape, not a minimum (same rule as Somnilux's archive format).
pub const ARCHIVE_FORMAT: u32 = 2;

const RECORD: &str = ".aasl-patches.json";

/// Top-level directories a format-2 archive may contain, and where each one
/// goes inside the Proton tree.
pub const ARCHIVE_DIRS: &[(&str, &str)] = &[
    ("x86_64-unix", "files/lib/wine/x86_64-unix"),
    ("x86_64-windows", "files/lib/wine/x86_64-windows"),
    ("x86_64-linux-gnu", "files/lib/x86_64-linux-gnu"),
];

/// Files planned for installation: (source in archive, destination in Proton).
pub type Plan = Vec<(PathBuf, PathBuf)>;

/// Check an extracted archive and work out what goes where, without
/// touching Proton. Refuses unstamped archives, the wrong format, unknown
/// directories, and files Proton does not already ship.
pub fn plan(extracted: &Path, proton_dir: &Path, expected_format: u32) -> Result<Plan> {
    let stamp = std::fs::read_to_string(extracted.join("archive-format.txt")).unwrap_or_default();
    let stamp = stamp.lines().next().unwrap_or("").trim();
    if stamp.is_empty() {
        return Err(Error::Invalid(
            "this archive does not declare its format, so it cannot be installed safely".into(),
        ));
    }
    if stamp != expected_format.to_string() {
        return Err(Error::Invalid(format!(
            "these fixes are format {stamp} and this AASL handles format {expected_format}; nothing was changed"
        )));
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(extracted).ctx(|| format!("reading {}", extracted.display()))? {
        let entry = entry.ctx(|| "reading archive".into())?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((_, rel)) = ARCHIVE_DIRS.iter().find(|(d, _)| *d == name) else {
            return Err(Error::Invalid(format!(
                "the archive contains '{name}', which this AASL does not know how to install"
            )));
        };
        for f in std::fs::read_dir(entry.path()).ctx(|| format!("reading {name}"))? {
            let f = f.ctx(|| "reading archive".into())?;
            if !f.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let dest = proton_dir.join(rel).join(f.file_name());
            if !dest.is_file() {
                return Err(Error::Invalid(format!(
                    "{} is missing from Proton; wrong Proton version?",
                    dest.display()
                )));
            }
            out.push((f.path(), dest));
        }
    }
    if out.is_empty() {
        return Err(Error::Invalid(
            "the archive contains nothing to install".into(),
        ));
    }
    out.sort();
    Ok(out)
}

/// What was installed, kept inside the Proton directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub bundle: String,
    pub format: u32,
    /// Path relative to the Proton root, and its sha256.
    pub files: Vec<(String, String)>,
    pub applied_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PatchStatus {
    pub bundle: Option<String>,
    /// Every recorded file still has the hash it was installed with.
    pub intact: bool,
    /// Files whose content changed since the bundle was applied.
    pub changed: Vec<String>,
}

fn sha256_file(p: &Path) -> String {
    std::fs::read(p)
        .map(|b| hex::encode(Sha256::digest(&b)))
        .unwrap_or_default()
}

/// Apply a plan and record it. Each original is kept once as
/// `<file>.orig`, so stock Proton can always be restored.
pub fn apply_recorded(plan: &Plan, proton_dir: &Path, bundle_id: &str) -> Result<usize> {
    let n = apply(plan)?;
    let files = plan
        .iter()
        .map(|(_, d)| {
            let rel = d
                .strip_prefix(proton_dir)
                .unwrap_or(d)
                .display()
                .to_string();
            (rel, sha256_file(d))
        })
        .collect();
    let rec = Record {
        bundle: bundle_id.into(),
        format: ARCHIVE_FORMAT,
        files,
        applied_at: now(),
    };
    write_atomic(
        &proton_dir.join(RECORD),
        &serde_json::to_vec_pretty(&rec).unwrap_or_default(),
    )?;
    Ok(n)
}

pub fn record(proton_dir: &Path) -> Option<Record> {
    serde_json::from_str(&std::fs::read_to_string(proton_dir.join(RECORD)).ok()?).ok()
}

/// Compare installed files with the record.
pub fn status(proton_dir: &Path) -> PatchStatus {
    match record(proton_dir) {
        None => PatchStatus {
            bundle: None,
            intact: false,
            changed: vec![],
        },
        Some(r) => {
            let changed: Vec<String> = r
                .files
                .iter()
                .filter(|(rel, h)| sha256_file(&proton_dir.join(rel)) != *h)
                .map(|(rel, _)| rel.clone())
                .collect();
            PatchStatus {
                bundle: Some(r.bundle),
                intact: changed.is_empty(),
                changed,
            }
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Copy a plan's files over Proton's, keeping `<file>.orig` backups.
pub fn apply(plan: &Plan) -> Result<usize> {
    for (src, dest) in plan {
        let orig = PathBuf::from(format!("{}.orig", dest.display()));
        if !orig.exists() {
            std::fs::copy(dest, &orig).ctx(|| format!("backing up {}", dest.display()))?;
        }
        let tmp = PathBuf::from(format!("{}.aasl-new", dest.display()));
        std::fs::copy(src, &tmp).ctx(|| format!("copying {}", src.display()))?;
        std::fs::rename(&tmp, dest).ctx(|| format!("replacing {}", dest.display()))?;
    }
    Ok(plan.len())
}

/// Put the stock files back and forget the record.
pub fn restore(proton_dir: &Path) -> Result<usize> {
    let _ = std::fs::remove_file(proton_dir.join(RECORD));
    let mut n = 0;
    for (_, rel) in ARCHIVE_DIRS {
        let dir = proton_dir.join(rel);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if let Some(stem) = p.to_str().and_then(|s| s.strip_suffix(".orig")) {
                std::fs::rename(&p, stem).ctx(|| format!("restoring {stem}"))?;
                n += 1;
            }
        }
    }
    Ok(n)
}

/// True when every file of a plan matches its installed copy.
pub fn is_applied(plan: &Plan) -> bool {
    plan.iter()
        .all(|(s, d)| std::fs::read(s).ok() == std::fs::read(d).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let ex = d.path().join("ex");
        let proton = d.path().join("proton");
        std::fs::create_dir_all(ex.join("x86_64-windows")).unwrap();
        std::fs::write(ex.join("archive-format.txt"), "2\n").unwrap();
        std::fs::write(ex.join("x86_64-windows/secur32.dll"), "patched").unwrap();
        std::fs::create_dir_all(proton.join("files/lib/wine/x86_64-windows")).unwrap();
        std::fs::write(
            proton.join("files/lib/wine/x86_64-windows/secur32.dll"),
            "stock",
        )
        .unwrap();
        (d, ex, proton)
    }

    #[test]
    fn plan_apply_restore() {
        let (_d, ex, proton) = setup();
        let p = plan(&ex, &proton, 2).unwrap();
        assert_eq!(p.len(), 1);
        assert!(!is_applied(&p));
        apply(&p).unwrap();
        let dll = proton.join("files/lib/wine/x86_64-windows/secur32.dll");
        assert_eq!(std::fs::read_to_string(&dll).unwrap(), "patched");
        assert!(is_applied(&p));
        apply_recorded(&p, &proton, "b1").unwrap();
        let st = status(&proton);
        assert_eq!(st.bundle.as_deref(), Some("b1"));
        assert!(st.intact);
        std::fs::write(&dll, "proton update").unwrap();
        let st = status(&proton);
        assert!(!st.intact);
        assert_eq!(
            st.changed,
            vec!["files/lib/wine/x86_64-windows/secur32.dll".to_string()]
        );
        apply(&p).unwrap();
        assert_eq!(
            std::fs::read_to_string(format!("{}.orig", dll.display())).unwrap(),
            "stock"
        );
        assert_eq!(restore(&proton).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(&dll).unwrap(), "stock");
        assert!(status(&proton).bundle.is_none());
    }

    #[test]
    fn refusals() {
        let (_d, ex, proton) = setup();
        assert!(plan(&ex, &proton, 3).is_err());
        std::fs::create_dir_all(ex.join("aarch64")).unwrap();
        assert!(
            plan(&ex, &proton, 2)
                .unwrap_err()
                .to_string()
                .contains("aarch64")
        );
        std::fs::remove_dir(ex.join("aarch64")).unwrap();
        std::fs::remove_file(ex.join("archive-format.txt")).unwrap();
        assert!(plan(&ex, &proton, 2).is_err());
    }

    #[test]
    fn file_missing_from_proton() {
        let (_d, ex, proton) = setup();
        std::fs::write(ex.join("x86_64-windows/crypt32.dll"), "p").unwrap();
        assert!(
            plan(&ex, &proton, 2)
                .unwrap_err()
                .to_string()
                .contains("crypt32")
        );
    }
}
