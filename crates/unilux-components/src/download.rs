//! Resumable, checksum-verified downloads.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use sha2::{Digest, Sha256, Sha512};
use tokio::io::AsyncWriteExt;
use unilux_core::{Error, IoContext, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checksum {
    Sha256(String),
    Sha512(String),
}

impl Checksum {
    /// Parse a `sha512sum` file and return the digest for `file_name`.
    pub fn from_sum_file(text: &str, file_name: &str) -> Option<Self> {
        text.lines().find_map(|l| {
            let mut it = l.split_whitespace();
            let digest = it.next()?;
            let name = it.next()?.trim_start_matches('*');
            let name = Path::new(name).file_name()?.to_str()?;
            (name == file_name).then(|| match digest.len() {
                64 => Self::Sha256(digest.to_lowercase()),
                _ => Self::Sha512(digest.to_lowercase()),
            })
        })
    }

    pub fn expected(&self) -> &str {
        match self {
            Self::Sha256(s) | Self::Sha512(s) => s,
        }
    }

    /// Hash a file on disk (blocking; call from spawn_blocking for big files).
    pub fn compute(&self, path: &Path) -> Result<String> {
        use std::io::Read;
        let mut f = std::fs::File::open(path).ctx(|| format!("opening {}", path.display()))?;
        let mut buf = vec![0u8; 1 << 20];
        let mut s256 = Sha256::new();
        let mut s512 = Sha512::new();
        loop {
            let n = f
                .read(&mut buf)
                .ctx(|| format!("reading {}", path.display()))?;
            if n == 0 {
                break;
            }
            match self {
                Self::Sha256(_) => s256.update(&buf[..n]),
                Self::Sha512(_) => s512.update(&buf[..n]),
            }
        }
        Ok(match self {
            Self::Sha256(_) => hex::encode(s256.finalize()),
            Self::Sha512(_) => hex::encode(s512.finalize()),
        })
    }

    pub fn verify(&self, path: &Path) -> Result<()> {
        let actual = self.compute(path)?;
        if actual == self.expected() {
            Ok(())
        } else {
            Err(Error::Checksum {
                file: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                expected: self.expected().into(),
                actual,
            })
        }
    }
}

#[derive(Clone)]
pub struct Downloader {
    client: reqwest::Client,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("unilux/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("HTTP client");
        Self { client }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub async fn text(&self, url: &str) -> Result<String> {
        self.client
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::Network(format!("{url}: {e}")))?
            .text()
            .await
            .map_err(|e| Error::Network(format!("{url}: {e}")))
    }

    /// Download `url` to `dest`. A partial `dest.part` from an earlier
    /// attempt is resumed with an HTTP Range request. When a checksum is
    /// given the file is verified before being moved into place; a bad
    /// file is deleted so the next attempt starts clean.
    pub async fn fetch(
        &self,
        url: &str,
        dest: &Path,
        checksum: Option<&Checksum>,
        mut progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<PathBuf> {
        if let Some(dir) = dest.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .ctx(|| format!("creating {}", dir.display()))?;
        }
        if let Some(sum) = checksum
            && dest.exists()
            && sum.verify(dest).is_ok()
        {
            return Ok(dest.to_path_buf());
        }
        let part = dest.with_extension("part");
        let mut have = tokio::fs::metadata(&part)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let mut req = self.client.get(url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = req
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::Network(format!("{url}: {e}")))?;
        let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if !resumed {
            have = 0;
        }
        let total = resp.content_length().map(|l| l + have);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)
            .await
            .ctx(|| format!("opening {}", part.display()))?;
        let mut stream = resp.bytes_stream();
        progress(have, total);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::Network(format!("{url}: {e}")))?;
            file.write_all(&chunk)
                .await
                .ctx(|| format!("writing {}", part.display()))?;
            have += chunk.len() as u64;
            progress(have, total);
        }
        file.flush()
            .await
            .ctx(|| format!("flushing {}", part.display()))?;
        drop(file);
        if let Some(sum) = checksum {
            let sum = sum.clone();
            let p = part.clone();
            let res = tokio::task::spawn_blocking(move || sum.verify(&p))
                .await
                .map_err(|e| Error::Invalid(e.to_string()))?;
            if let Err(e) = res {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(e);
            }
        }
        tokio::fs::rename(&part, dest)
            .await
            .ctx(|| format!("moving into {}", dest.display()))?;
        Ok(dest.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sha256(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }

    #[test]
    fn sum_file_parsing() {
        let t = "abc123  GE-Proton11-5-x86_64.tar.gz\n";
        assert_eq!(
            Checksum::from_sum_file(t, "GE-Proton11-5-x86_64.tar.gz"),
            Some(Checksum::Sha512("abc123".into()))
        );
        assert_eq!(Checksum::from_sum_file(t, "other"), None);
    }

    #[tokio::test]
    async fn downloads_and_verifies() {
        let server = MockServer::start().await;
        let body = b"hello world".to_vec();
        Mock::given(method("GET"))
            .and(path("/f"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let dest = d.path().join("f.bin");
        let sum = Checksum::Sha256(sha256(&body));
        let mut last = 0;
        Downloader::new()
            .fetch(&format!("{}/f", server.uri()), &dest, Some(&sum), |n, _| {
                last = n
            })
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(last, body.len() as u64);
    }

    #[tokio::test]
    async fn bad_checksum_removes_partial() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"tampered".to_vec()))
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let dest = d.path().join("f.bin");
        let err = Downloader::new()
            .fetch(
                &server.uri(),
                &dest,
                Some(&Checksum::Sha256("00".repeat(32))),
                |_, _| {},
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Checksum { .. }));
        assert!(!dest.exists());
        assert!(!dest.with_extension("part").exists());
    }

    #[tokio::test]
    async fn resumes_with_range() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("range", "bytes=6-"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(b"world".to_vec()))
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let dest = d.path().join("f.bin");
        std::fs::write(dest.with_extension("part"), b"hello ").unwrap();
        let sum = Checksum::Sha256(sha256(b"hello world"));
        Downloader::new()
            .fetch(&server.uri(), &dest, Some(&sum), |_, _| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello world");
    }

    #[tokio::test]
    async fn http_error_is_network_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let err = Downloader::new()
            .fetch(&server.uri(), &d.path().join("x"), None, |_, _| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Network(_)));
    }
}
