//! The catalog: recommended Proton and published patch bundles.

use unilux_components::ProtonId;
use unilux_core::{Error, Result};
use serde::{Deserialize, Serialize};

pub const EMBEDDED: &str = include_str!("../../../data/catalog.toml");
pub const DEFAULT_URL: &str =
    "https://raw.githubusercontent.com/Zutyosh/Unilux/main/data/catalog.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetStatus {
    /// Tested end to end with Somnium on this exact Proton.
    Verified,
    /// Expected to work, nobody has reported back.
    Untested,
    /// Known not to work; listed so Unilux can say so.
    Broken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    /// `provider:tag`, where the tag may end in `*`.
    pub proton: String,
    pub status: TargetStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    pub id: String,
    pub title: String,
    pub archive_format: u32,
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub series: Vec<String>,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    pub recommended_proton: String,
    #[serde(default, rename = "bundle")]
    pub bundles: Vec<Bundle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Match {
    pub bundle: Bundle,
    pub status: TargetStatus,
}

fn matches(pattern: &str, id: &ProtonId) -> bool {
    let Some((prov, tag)) = pattern.split_once(':') else {
        return false;
    };
    if prov != id.provider {
        return false;
    }
    match tag.strip_suffix('*') {
        Some(prefix) => id.tag.starts_with(prefix),
        None => tag == id.tag,
    }
}

impl Catalog {
    pub fn parse(text: &str) -> Result<Self> {
        let c: Catalog =
            toml::from_str(text).map_err(|e| Error::Invalid(format!("catalog: {e}")))?;
        if c.schema != 1 {
            return Err(Error::Invalid(format!(
                "catalog schema {} is newer than this Unilux understands",
                c.schema
            )));
        }
        Ok(c)
    }

    pub fn embedded() -> Self {
        Self::parse(EMBEDDED).expect("embedded catalog is valid")
    }

    /// Fetch the live catalog; fall back to the embedded copy when offline.
    pub async fn fetch(client: &reqwest_like::Client, url: &str) -> (Self, Option<String>) {
        match client.text(url).await.and_then(|t| Self::parse(&t)) {
            Ok(c) => (c, None),
            Err(e) => (
                Self::embedded(),
                Some(format!("using the built-in catalog: {e}")),
            ),
        }
    }

    pub fn recommended(&self) -> Result<ProtonId> {
        ProtonId::parse(&self.recommended_proton)
    }

    /// Bundles usable with this Proton, best first. The most specific
    /// matching target of each bundle decides its status.
    pub fn resolve(&self, id: &ProtonId) -> Vec<Match> {
        let mut out: Vec<Match> = self
            .bundles
            .iter()
            .filter_map(|b| {
                let exact = b
                    .targets
                    .iter()
                    .find(|t| !t.proton.ends_with('*') && matches(&t.proton, id));
                let t = exact.or_else(|| b.targets.iter().find(|t| matches(&t.proton, id)))?;
                (t.status != TargetStatus::Broken).then(|| Match {
                    bundle: b.clone(),
                    status: t.status,
                })
            })
            .collect();
        out.sort_by_key(|m| m.status);
        out
    }

    /// Proton builds with a verified bundle.
    pub fn verified_protons(&self) -> Vec<String> {
        self.bundles
            .iter()
            .flat_map(|b| b.targets.iter())
            .filter(|t| t.status == TargetStatus::Verified && !t.proton.ends_with('*'))
            .map(|t| t.proton.clone())
            .collect()
    }
}

/// Minimal adapter so the catalog does not depend on reqwest directly.
pub mod reqwest_like {
    pub type Client = unilux_components::Downloader;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_is_consistent() {
        let c = Catalog::embedded();
        let rec = c.recommended().unwrap();
        let m = c.resolve(&rec);
        assert_eq!(
            m[0].status,
            TargetStatus::Verified,
            "recommended Proton needs a verified bundle"
        );
        assert!(c.verified_protons().contains(&c.recommended_proton));
    }

    #[test]
    fn wildcard_and_exact() {
        let t = "schema = 1\nrecommended_proton = \"ge:GE-Proton11-5\"\n[[bundle]]\nid = \"x\"\ntitle = \"x\"\narchive_format = 2\nurl = \"u\"\ntargets = [\n  { proton = \"ge:GE-Proton11-5\", status = \"verified\" },\n  { proton = \"ge:GE-Proton11-*\", status = \"untested\" },\n]\n";
        let c = Catalog::parse(t).unwrap();
        let m = c.resolve(&ProtonId::new("ge", "GE-Proton11-12"));
        assert_eq!(m[0].status, TargetStatus::Untested);
        let e = Catalog::embedded();
        let m = e.resolve(&ProtonId::new("ge", "GE-Proton11-12"));
        assert!(
            m.iter().all(|m| m.bundle.id != "somnilux-build-1"),
            "11-5's winedmo must not replace a newer GE's"
        );
        assert_eq!(m[0].status, TargetStatus::Verified);
        assert!(c.resolve(&ProtonId::new("ge", "GE-Proton10-4")).is_empty());
        assert!(
            c.resolve(&ProtonId::new("cachyos", "cachyos-11.0-20261015-slr"))
                .is_empty()
        );
    }

    #[test]
    fn broken_is_excluded_and_future_schema_refused() {
        let t = r#"schema = 1
recommended_proton = "ge:A"
[[bundle]]
id = "x"
title = "x"
archive_format = 2
url = "u"
targets = [{ proton = "ge:A", status = "broken" }]"#;
        assert!(
            Catalog::parse(t)
                .unwrap()
                .resolve(&ProtonId::new("ge", "A"))
                .is_empty()
        );
        assert!(Catalog::parse("schema = 2\nrecommended_proton = \"ge:A\"").is_err());
    }
}
