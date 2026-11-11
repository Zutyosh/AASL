//! Compatibility recipes: a Proton plus the ingredients that make Somnium
//! work on it. One TOML format for the built-in recipes, the user's own,
//! import/export and agents.
//!
//! An ingredient is a typed, declarative fix: Wine patches to compile, or
//! environment variables. Recipes never carry scripts, so importing one
//! can at worst add patches, which are shown and hashed before any build.

use std::collections::BTreeMap;
use std::path::Path;

use aasl_components::ProtonId;
use aasl_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// The recipe format this AASL reads and writes.
pub const FORMAT: u32 = 1;
/// The recipe used when none is selected.
pub const DEFAULT_RECIPE: &str = "recommended";
const LIBRARY_FILE: &str = "ingredients.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Patch,
    Env,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    Upstream,
    Proton,
}

/// Skip an ingredient when the Proton already carries the fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    /// Path in the Wine tree.
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lacks: Option<String>,
}

/// A patch file: `name` is a path in the library, or a file name with its
/// `content` inline (custom and exported ingredients).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PatchRef {
    Library(String),
    Inline { name: String, content: String },
}

impl PatchRef {
    pub fn name(&self) -> &str {
        match self {
            Self::Library(n) | Self::Inline { name: n, .. } => n,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ingredient {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Base>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_prep: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patches: Vec<PatchRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dlls: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unix_libs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded_if: Vec<Condition>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub format: u32,
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// `provider:tag`, `provider:latest`, or none for the Proton picked
    /// in AASL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proton: Option<String>,
    /// Ingredient ids, from the library or from `ingredient` below, in
    /// application order.
    pub ingredients: Vec<String>,
    /// Extra launch environment, applied after the ingredients'.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Ingredients defined by this recipe.
    #[serde(default, rename = "ingredient", skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<Ingredient>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    ingredient: Vec<Ingredient>,
}

/// The known ingredients and the files they point at.
#[derive(Debug, Clone)]
pub struct Library {
    pub ingredients: Vec<Ingredient>,
    files: BTreeMap<String, String>,
}

/// An ingredient with its patch texts loaded and checked.
#[derive(Debug, Clone, Serialize)]
pub struct Resolved {
    pub ingredient: Ingredient,
    /// (file name, text) in application order.
    pub patches: Vec<(String, String)>,
    /// Defined by the recipe rather than the library: code from elsewhere.
    pub custom: bool,
}

/// A recipe checked against the library, ready to build and launch.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub recipe: Recipe,
    pub ingredients: Vec<Resolved>,
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn valid_name(name: &str) -> bool {
    let p = Path::new(name);
    !name.is_empty()
        && name.ends_with(".patch")
        && !p.is_absolute()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn sha256(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Hex SHA-256 of a text, as recipes and patch lists show it.
pub fn sha256_hex(text: &str) -> String {
    sha256(text)
}

impl Ingredient {
    fn check(&self) -> Result<()> {
        let id = &self.id;
        if !valid_id(id) {
            return Err(invalid(format!(
                "ingredient id {id:?}: use lowercase letters, digits, - and _"
            )));
        }
        match self.kind {
            Kind::Env => {
                if self.env.is_empty() {
                    return Err(invalid(format!(
                        "ingredient {id} is kind = \"env\" but sets no env"
                    )));
                }
                if !self.patches.is_empty() || self.base.is_some() {
                    return Err(invalid(format!(
                        "ingredient {id}: env ingredients have no patches or base"
                    )));
                }
            }
            Kind::Patch => {
                let Some(base) = self.base else {
                    return Err(invalid(format!(
                        "ingredient {id}: patch ingredients need base = \"upstream\" or \"proton\""
                    )));
                };
                if self.patches.is_empty() || self.dlls.is_empty() {
                    return Err(invalid(format!(
                        "ingredient {id}: patch ingredients need patches and dlls"
                    )));
                }
                if base == Base::Upstream
                    && (self.upstream_repo.is_none() || self.upstream_commit.is_none())
                {
                    return Err(invalid(format!(
                        "ingredient {id}: base = \"upstream\" needs upstream_repo and upstream_commit"
                    )));
                }
                for d in self.dlls.iter().chain(&self.unix_libs) {
                    if !valid_id(d) {
                        return Err(invalid(format!("ingredient {id}: bad DLL name {d:?}")));
                    }
                }
                for p in &self.patches {
                    if !valid_name(p.name()) {
                        return Err(invalid(format!(
                            "ingredient {id}: bad patch name {:?}",
                            p.name()
                        )));
                    }
                }
                for c in &self.superseded_if {
                    if c.has.is_some() == c.lacks.is_some() || c.file.contains("..") {
                        return Err(invalid(format!(
                            "ingredient {id}: each superseded_if needs file and exactly one of has/lacks"
                        )));
                    }
                }
            }
        }
        for k in self.env.keys() {
            if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(invalid(format!("ingredient {id}: bad variable name {k:?}")));
            }
        }
        Ok(())
    }
}

impl Library {
    /// The library compiled into AASL.
    pub fn embedded() -> Result<Self> {
        let files = EMBEDDED
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Self::from_files(files)
    }

    /// A checkout's `patches/` directory (development).
    pub fn load(dir: &Path) -> Result<Self> {
        let mut files = BTreeMap::new();
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(root, &p, out);
                } else if let (Ok(rel), Ok(text)) =
                    (p.strip_prefix(root), std::fs::read_to_string(&p))
                {
                    out.insert(rel.to_string_lossy().into_owned(), text);
                }
            }
        }
        walk(dir, dir, &mut files);
        Self::from_files(files)
    }

    fn from_files(files: BTreeMap<String, String>) -> Result<Self> {
        let text = files
            .get(LIBRARY_FILE)
            .ok_or_else(|| invalid(format!("{LIBRARY_FILE} is missing")))?;
        let f: LibraryFile =
            toml::from_str(text).map_err(|e| invalid(format!("{LIBRARY_FILE}: {e}")))?;
        let lib = Self {
            ingredients: f.ingredient,
            files,
        };
        let mut seen = std::collections::BTreeSet::new();
        for i in &lib.ingredients {
            i.check()?;
            if !seen.insert(i.id.as_str()) {
                return Err(invalid(format!(
                    "{LIBRARY_FILE}: ingredient {} is defined twice",
                    i.id
                )));
            }
            for p in &i.patches {
                if lib.patch_text(p).is_none() {
                    return Err(invalid(format!(
                        "{LIBRARY_FILE}: {} lists {}, which does not exist",
                        i.id,
                        p.name()
                    )));
                }
            }
        }
        Ok(lib)
    }

    /// (ingredient id, file name, text) for every library patch.
    pub fn patch_files(&self) -> Vec<(String, String, String)> {
        self.ingredients
            .iter()
            .flat_map(|i| {
                i.patches.iter().filter_map(move |p| {
                    let name = p.name().rsplit('/').next().unwrap_or_default().to_string();
                    self.patch_text(p).map(|t| (i.id.clone(), name, t))
                })
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Ingredient> {
        self.ingredients.iter().find(|i| i.id == id)
    }

    fn patch_text(&self, p: &PatchRef) -> Option<String> {
        match p {
            PatchRef::Library(n) => self.files.get(n).cloned(),
            PatchRef::Inline { content, .. } => Some(content.clone()),
        }
    }

    /// Built-in recipes, by id.
    pub fn builtin_recipes(&self) -> Vec<Recipe> {
        self.files
            .iter()
            .filter(|(k, _)| k.starts_with("recipes/") && k.ends_with(".toml"))
            .filter_map(|(_, v)| Recipe::parse(v).ok())
            .collect()
    }

    /// Check a recipe and load everything it needs.
    pub fn plan(&self, recipe: &Recipe) -> Result<Plan> {
        recipe.check()?;
        let mut out = Vec::new();
        for id in &recipe.ingredients {
            let custom = recipe.custom.iter().find(|i| &i.id == id);
            let ing = custom
                .or_else(|| self.get(id))
                .ok_or_else(|| invalid(format!("recipe {}: unknown ingredient {id}", recipe.id)))?;
            let mut patches = Vec::new();
            for p in &ing.patches {
                let text = self.patch_text(p).ok_or_else(|| {
                    invalid(format!(
                        "ingredient {id}: {} is not in the library; inline its content",
                        p.name()
                    ))
                })?;
                if !text.contains("\n--- ") || !text.contains("\n+++ ") {
                    return Err(invalid(format!(
                        "ingredient {id}: {} is not a patch",
                        p.name()
                    )));
                }
                patches.push((
                    p.name().rsplit('/').next().unwrap_or_default().to_string(),
                    text,
                ));
            }
            let custom = custom.is_some_and(|c| self.get(&c.id) != Some(c));
            out.push(Resolved {
                ingredient: ing.clone(),
                patches,
                custom,
            });
        }
        Ok(Plan {
            recipe: recipe.clone(),
            ingredients: out,
        })
    }

    /// A self-contained copy: every ingredient inlined with its patch
    /// texts, so the file works on any AASL, even one whose library
    /// differs.
    pub fn export(&self, recipe: &Recipe) -> Result<String> {
        let plan = self.plan(recipe)?;
        let mut r = recipe.clone();
        r.custom = plan
            .ingredients
            .into_iter()
            .map(|res| {
                let mut i = res.ingredient;
                i.patches = res
                    .patches
                    .into_iter()
                    .map(|(name, content)| PatchRef::Inline { name, content })
                    .collect();
                i
            })
            .collect();
        r.to_toml()
    }

    /// Read a recipe someone shared: inlined copies of library
    /// ingredients collapse back to references, so only real custom code
    /// stays inline (and is flagged by `plan`).
    pub fn import(&self, text: &str) -> Result<Recipe> {
        let mut r = Recipe::parse(text)?;
        let lib = self.clone();
        r.custom.retain(|c| match lib.get(&c.id) {
            Some(l) => !same_content(&lib, l, c),
            None => true,
        });
        self.plan(&r)?;
        Ok(r)
    }
}

fn same_content(lib: &Library, a: &Ingredient, b: &Ingredient) -> bool {
    let texts = |i: &Ingredient| -> Vec<Option<String>> {
        i.patches.iter().map(|p| lib.patch_text(p)).collect()
    };
    let strip = |i: &Ingredient| Ingredient {
        patches: Vec::new(),
        ..i.clone()
    };
    strip(a) == strip(b) && texts(a) == texts(b)
}

impl Recipe {
    pub fn parse(text: &str) -> Result<Self> {
        let r: Recipe = toml::from_str(text).map_err(|e| invalid(format!("recipe: {e}")))?;
        r.check()?;
        Ok(r)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| invalid(format!("writing the recipe: {e}")))
    }

    fn check(&self) -> Result<()> {
        if self.format != FORMAT {
            return Err(invalid(format!(
                "recipe {} is format {}, this AASL reads format {FORMAT}",
                self.id, self.format
            )));
        }
        if !valid_id(&self.id) {
            return Err(invalid(format!(
                "recipe id {:?}: use lowercase letters, digits, - and _",
                self.id
            )));
        }
        if let Some(p) = &self.proton {
            let ok = p
                .split_once(':')
                .is_some_and(|(prov, tag)| !prov.is_empty() && !tag.is_empty());
            if !ok {
                return Err(invalid(format!(
                    "recipe {}: proton must be provider:tag or provider:latest",
                    self.id
                )));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for id in &self.ingredients {
            if !seen.insert(id) {
                return Err(invalid(format!("recipe {}: {id} is listed twice", self.id)));
            }
        }
        for i in &self.custom {
            i.check()?;
        }
        Ok(())
    }

    /// The Proton this recipe pins, if any (`latest` is resolved by the caller).
    pub fn proton(&self) -> Option<(String, String)> {
        self.proton
            .as_ref()
            .and_then(|p| p.split_once(':'))
            .map(|(a, b)| (a.to_string(), b.to_string()))
    }
}

/// One Wine tree of a build: every proton-base ingredient shares the
/// Proton's tree; upstream ingredients get one per (repo, commit).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    pub id: String,
    pub base: Base,
    pub upstream_repo: Option<String>,
    pub upstream_commit: Option<String>,
    pub requires_prep: Option<String>,
    pub dlls: Vec<String>,
    pub unix_libs: Vec<String>,
    /// (file name in the group's directory, text, superseded_if)
    #[serde(skip)]
    pub patches: Vec<(String, String, Vec<Condition>)>,
}

impl Plan {
    pub fn needs_build(&self) -> bool {
        self.ingredients
            .iter()
            .any(|i| i.ingredient.kind == Kind::Patch)
    }

    pub fn has_custom_code(&self) -> bool {
        self.ingredients
            .iter()
            .any(|i| i.custom && !i.patches.is_empty())
    }

    /// Launch environment: ingredients in order, then the recipe's own.
    pub fn env(&self) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        for i in &self.ingredients {
            env.extend(i.ingredient.env.clone());
        }
        env.extend(self.recipe.env.clone());
        env
    }

    /// The Wine trees and patches a build applies, in order.
    pub fn groups(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();
        for r in &self.ingredients {
            let i = &r.ingredient;
            let Some(base) = i.base else { continue };
            let key = (base, i.upstream_repo.clone(), i.upstream_commit.clone());
            let pos = groups
                .iter()
                .position(|g| (g.base, g.upstream_repo.clone(), g.upstream_commit.clone()) == key);
            let g = match pos {
                Some(p) => &mut groups[p],
                None => {
                    let n = groups.iter().filter(|g| g.base == base).count();
                    let id = match (base, n) {
                        (Base::Proton, _) => "proton".to_string(),
                        (Base::Upstream, 0) => "upstream".to_string(),
                        (Base::Upstream, n) => format!("upstream{}", n + 1),
                    };
                    groups.push(Group {
                        id,
                        base,
                        upstream_repo: i.upstream_repo.clone(),
                        upstream_commit: i.upstream_commit.clone(),
                        requires_prep: None,
                        dlls: Vec::new(),
                        unix_libs: Vec::new(),
                        patches: Vec::new(),
                    });
                    groups.last_mut().unwrap()
                }
            };
            if i.requires_prep.is_some() {
                g.requires_prep = i.requires_prep.clone();
            }
            for d in &i.dlls {
                if !g.dlls.contains(d) {
                    g.dlls.push(d.clone());
                }
            }
            for d in &i.unix_libs {
                if !g.unix_libs.contains(d) {
                    g.unix_libs.push(d.clone());
                }
            }
            for (name, text) in &r.patches {
                let n = g.patches.len() + 1;
                g.patches.push((
                    format!("{n:04}-{}-{name}", i.id),
                    text.clone(),
                    i.superseded_if.clone(),
                ));
            }
        }
        groups
    }

    /// Identifies the bundle this recipe builds for `proton`: same
    /// ingredients, same Proton, same builder means the same bundle.
    pub fn bundle_key(&self, proton: &ProtonId) -> String {
        let mut h = Sha256::new();
        h.update(format!("{}:{}\n", proton.provider, proton.tag));
        h.update(sha256(crate::builder::BUILD_SCRIPT));
        h.update(sha256(crate::builder::CONTAINERFILE));
        for g in self.groups() {
            h.update(serde_json::to_string(&g).unwrap_or_default());
            for (name, text, cond) in &g.patches {
                h.update(name);
                h.update(sha256(text));
                h.update(serde_json::to_string(cond).unwrap_or_default());
            }
        }
        hex::encode(h.finalize())[..16].to_string()
    }

    /// Write what build.sh reads: series.toml, one directory per group
    /// with its patches, and a `superseded` list for skipped fixes.
    pub fn materialise(&self, dir: &Path) -> Result<Vec<String>> {
        let _ = std::fs::remove_dir_all(dir);
        let io = |what: String| move |e| Error::io(what, e);
        std::fs::create_dir_all(dir).map_err(io(format!("creating {}", dir.display())))?;
        let mut series = String::new();
        let mut ids = Vec::new();
        for g in self.groups() {
            let gdir = dir.join(&g.id);
            std::fs::create_dir_all(&gdir).map_err(io(format!("creating {}", gdir.display())))?;
            let mut superseded = String::new();
            for (name, text, conds) in &g.patches {
                std::fs::write(gdir.join(name), text).map_err(io(format!("writing {name}")))?;
                for c in conds {
                    let (cond, needle) = match (&c.has, &c.lacks) {
                        (Some(n), _) => ("has", n),
                        (_, Some(n)) => ("lacks", n),
                        _ => continue,
                    };
                    superseded.push_str(&format!("{name} {cond} {} {needle}\n", c.file));
                }
            }
            std::fs::write(gdir.join("superseded"), superseded)
                .map_err(io("writing superseded".into()))?;
            let list = |v: &[String]| {
                v.iter()
                    .map(|s| format!("{s:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            series.push_str(&format!(
                "[[series]]\nid = {:?}\ndir = {:?}\nbase = {:?}\n",
                g.id,
                g.id,
                match g.base {
                    Base::Upstream => "upstream",
                    Base::Proton => "proton",
                }
            ));
            if let (Some(r), Some(c)) = (&g.upstream_repo, &g.upstream_commit) {
                series.push_str(&format!("upstream_repo = {r:?}\nupstream_commit = {c:?}\n"));
            }
            if let Some(p) = &g.requires_prep {
                series.push_str(&format!("requires_prep = {p:?}\n"));
            }
            series.push_str(&format!(
                "dlls = [{}]\nunix_libs = [{}]\n\n",
                list(&g.dlls),
                list(&g.unix_libs)
            ));
            ids.push(g.id);
        }
        std::fs::write(dir.join("series.toml"), series)
            .map_err(io("writing series.toml".into()))?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> Library {
        Library::embedded().unwrap()
    }

    fn recommended() -> Recipe {
        lib()
            .builtin_recipes()
            .into_iter()
            .find(|r| r.id == DEFAULT_RECIPE)
            .unwrap()
    }

    #[test]
    fn embedded_library_and_default_recipe() {
        let l = lib();
        assert!(l.get("tls-client-certs").is_some());
        let p = l.plan(&recommended()).unwrap();
        assert!(p.needs_build());
        assert!(!p.has_custom_code());
        assert_eq!(
            p.env()["PROTON_YTDLP_EXTRACTOR_ARGS"],
            "youtube:player_client=android"
        );
        let g = p.groups();
        assert_eq!(
            g.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
            ["upstream", "proton"]
        );
        assert_eq!(g[1].dlls, ["winedmo", "kernelbase"]);
        assert_eq!(g[1].requires_prep.as_deref(), Some("ge-video-rework"));
        assert!(g[1].patches[0].0.starts_with("0001-media-by-url-0001-"));
        assert_eq!(g[1].patches.len(), 5);
    }

    #[test]
    fn materialise_writes_what_build_sh_reads() {
        let d = tempfile::tempdir().unwrap();
        let ids = lib()
            .plan(&recommended())
            .unwrap()
            .materialise(d.path())
            .unwrap();
        assert_eq!(ids, ["upstream", "proton"]);
        let series = std::fs::read_to_string(d.path().join("series.toml")).unwrap();
        assert!(series.contains("upstream_commit = \"e99fc2f"));
        let sup = std::fs::read_to_string(d.path().join("proton/superseded")).unwrap();
        assert!(sup.contains("0001-media-by-url-0001-winedmo-Open-RTSP-streams-by-URL.patch lacks dlls/winedmo/media_source.c URLOpenBlockingStreamW"));
        assert_eq!(
            std::fs::read_dir(d.path().join("upstream"))
                .unwrap()
                .count(),
            5
        );
    }

    #[test]
    fn export_import_round_trip_and_custom_code() {
        let l = lib();
        let text = l.export(&recommended()).unwrap();
        assert!(text.contains("content = "));
        let back = l.import(&text).unwrap();
        assert!(
            back.custom.is_empty(),
            "library copies collapse to references"
        );
        assert_eq!(back.ingredients, recommended().ingredients);
        let key = |r: &Recipe| {
            l.plan(r)
                .unwrap()
                .bundle_key(&ProtonId::new("ge", "GE-Proton11-12"))
        };
        assert_eq!(key(&back), key(&recommended()));

        let mut mine = recommended();
        mine.id = "mine".into();
        let mut extra = l.get("ytdlp-hook").unwrap().clone();
        extra.id = "my-fix".into();
        extra.patches = vec![PatchRef::Inline {
            name: "0001-x.patch".into(),
            content: "Subject: x\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n".into(),
        }];
        mine.custom.push(extra);
        mine.ingredients.push("my-fix".into());
        let p = l.plan(&mine).unwrap();
        assert!(p.has_custom_code());
        assert_ne!(
            p.bundle_key(&ProtonId::new("ge", "GE-Proton11-12")),
            key(&recommended())
        );
        let again = l.import(&l.export(&mine).unwrap()).unwrap();
        assert_eq!(again.custom.len(), 1);
    }

    #[test]
    fn refusals() {
        let l = lib();
        let bad = |s: &str| {
            Recipe::parse(s)
                .and_then(|r| l.plan(&r).map(|_| ()))
                .unwrap_err()
                .to_string()
        };
        let head = "format = 1\nid = \"x\"\ntitle = \"X\"\n";
        assert!(bad(&format!("{head}ingredients = [\"nope\"]")).contains("unknown ingredient"));
        assert!(
            bad(&format!("{head}ingredients = []\nscript = \"rm -rf\"")).contains("unknown field")
        );
        assert!(bad("format = 9\nid = \"x\"\ntitle = \"X\"\ningredients = []").contains("format"));
        assert!(bad(&format!("{head}proton = \"ge\"\ningredients = []")).contains("provider:tag"));
        let traversal = format!(
            "{head}ingredients = [\"e\"]\n[[ingredient]]\nid = \"e\"\ntitle = \"E\"\nkind = \"patch\"\nbase = \"proton\"\ndlls = [\"x\"]\npatches = [{{ name = \"../../etc/x.patch\", content = \"\" }}]"
        );
        assert!(bad(&traversal).contains("bad patch name"));
    }
}
