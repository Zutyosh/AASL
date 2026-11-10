//! Archive extraction into a staging directory, then an atomic move.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use aasl_core::{Error, IoContext, Result};

fn decoder(path: &Path) -> Result<Box<dyn Read>> {
    let f = BufReader::new(File::open(path).ctx(|| format!("opening {}", path.display()))?);
    let name = path.to_string_lossy();
    Ok(if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Box::new(flate2::read::GzDecoder::new(f))
    } else if name.ends_with(".tar.xz") {
        Box::new(xz2::read::XzDecoder::new(f))
    } else if name.ends_with(".tar.zst") {
        Box::new(zstd::stream::read::Decoder::new(f).map_err(|e| Error::io("zstd", e))?)
    } else if name.ends_with(".tar") {
        Box::new(f)
    } else {
        return Err(Error::Invalid(format!("unknown archive type: {name}")));
    })
}

/// Extract `archive` into a fresh directory next to `staging_parent`.
/// Entries escaping the target (`..`, absolute paths) are skipped by the
/// tar crate.
pub fn extract_to_staging(archive: &Path, staging_parent: &Path) -> Result<tempdir::Staging> {
    std::fs::create_dir_all(staging_parent)
        .ctx(|| format!("creating {}", staging_parent.display()))?;
    let staging = tempdir::Staging::new(staging_parent)?;
    let mut ar = tar::Archive::new(decoder(archive)?);
    ar.set_preserve_permissions(true);
    ar.unpack(staging.path())
        .ctx(|| format!("extracting {}", archive.display()))?;
    Ok(staging)
}

/// Move `src` to `dest`, replacing nothing: fails if `dest` exists.
/// Both must be on the same filesystem, which holds because staging lives
/// beside the destination.
pub fn move_into_place(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Err(Error::Invalid(format!("{} already exists", dest.display())));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ctx(|| format!("creating {}", parent.display()))?;
    }
    std::fs::rename(src, dest).ctx(|| format!("moving {} to {}", src.display(), dest.display()))
}

pub mod tempdir {
    use super::*;

    /// A uniquely named directory removed on drop unless kept.
    pub struct Staging {
        path: PathBuf,
    }

    impl Staging {
        pub fn new(parent: &Path) -> Result<Self> {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let path = parent.join(format!(".aasl-staging-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&path).ctx(|| format!("creating {}", path.display()))?;
            Ok(Self { path })
        }
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Staging {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn make_tar_gz(path: &Path, files: &[(&str, &[u8])]) {
        let f = File::create(path).unwrap();
        let gz = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        for (name, data) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, *data).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn extract_and_move() {
        let d = tempfile::tempdir().unwrap();
        let ar = d.path().join("a.tar.gz");
        make_tar_gz(&ar, &[("top/x.txt", b"x")]);
        let st = extract_to_staging(&ar, d.path()).unwrap();
        let dest = d.path().join("final");
        move_into_place(&st.path().join("top"), &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("x.txt")).unwrap(), b"x");
        let staging_path = st.path().to_path_buf();
        drop(st);
        assert!(!staging_path.exists());
    }

    #[test]
    fn unknown_type_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.zip");
        std::fs::write(&p, b"").unwrap();
        assert!(extract_to_staging(&p, d.path()).is_err());
    }
}
