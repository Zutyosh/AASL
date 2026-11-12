//! Compatibility recipes on the service: the same operations for the GUI,
//! the CLI and agents.

use std::path::PathBuf;

use serde::Serialize;
use unilux_components::ProtonId;
use unilux_core::{Error, Result, Severity};
use unilux_patches::recipe::DEFAULT_RECIPE;
use unilux_patches::{Library, Plan, Recipe, Source};

use crate::Service;

/// A recipe as lists show it.
#[derive(Debug, Clone, Serialize)]
pub struct RecipeInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Shipped with Unilux (read-only; export it to make a copy).
    pub builtin: bool,
    pub active: bool,
    pub proton: Option<String>,
    pub ingredients: Vec<String>,
    /// Carries patches that are not in Unilux's library: someone else's code.
    pub custom_code: bool,
    /// Compiles something (else only environment variables).
    pub needs_build: bool,
}

impl Service {
    /// The ingredient library: the checkout's `patches/` when
    /// UNILUX_PATCHES_DIR points at one, else the copy compiled in.
    pub fn library(&self) -> Result<Library> {
        match std::env::var("UNILUX_PATCHES_DIR").ok().map(PathBuf::from) {
            Some(d) if d.join("ingredients.toml").is_file() => Library::load(&d),
            _ => Library::embedded(),
        }
    }

    fn recipes_dir(&self) -> PathBuf {
        self.inner.paths.config_dir.join("recipes")
    }

    fn user_recipes(&self) -> Vec<Recipe> {
        let mut out: Vec<Recipe> = std::fs::read_dir(self.recipes_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .filter_map(
                |p| match std::fs::read_to_string(&p).map(|t| Recipe::parse(&t)) {
                    Ok(Ok(r)) => Some(r),
                    _ => {
                        self.notice(
                            Severity::Warn,
                            format!("Skipping {}: not a valid recipe", p.display()),
                        );
                        None
                    }
                },
            )
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    fn info(&self, lib: &Library, r: &Recipe, builtin: bool, active: &str) -> RecipeInfo {
        let plan = lib.plan(r).ok();
        RecipeInfo {
            id: r.id.clone(),
            title: r.title.clone(),
            description: r.description.clone(),
            builtin,
            active: r.id == active,
            proton: r.proton.clone(),
            ingredients: r.ingredients.clone(),
            custom_code: plan.as_ref().is_some_and(Plan::has_custom_code),
            needs_build: plan.as_ref().is_some_and(Plan::needs_build),
        }
    }

    /// Built-in recipes first, then the user's.
    pub fn recipes(&self) -> Result<Vec<RecipeInfo>> {
        let lib = self.library()?;
        let active = self.active_recipe_id();
        let mut out: Vec<RecipeInfo> = lib
            .builtin_recipes()
            .iter()
            .map(|r| self.info(&lib, r, true, &active))
            .collect();
        out.extend(
            self.user_recipes()
                .iter()
                .map(|r| self.info(&lib, r, false, &active)),
        );
        Ok(out)
    }

    pub fn recipe(&self, id: &str) -> Result<Recipe> {
        let lib = self.library()?;
        lib.builtin_recipes()
            .into_iter()
            .chain(self.user_recipes())
            .find(|r| r.id == id)
            .ok_or_else(|| {
                Error::Invalid(format!("no recipe named {id}; see `unilux recipe list`"))
            })
    }

    fn active_recipe_id(&self) -> String {
        self.config()
            .recipe
            .unwrap_or_else(|| DEFAULT_RECIPE.into())
    }

    /// The recipe in use, checked. Falls back to the default one (with a
    /// warning) when the selected recipe vanished or no longer checks.
    pub fn active_plan(&self) -> Result<Plan> {
        let lib = self.library()?;
        let id = self.active_recipe_id();
        match self.recipe(&id).and_then(|r| lib.plan(&r)) {
            Ok(p) => Ok(p),
            Err(e) if id != DEFAULT_RECIPE => {
                self.notice(
                    Severity::Warn,
                    format!("Recipe {id} is unusable ({e}); using {DEFAULT_RECIPE}."),
                );
                lib.plan(&self.recipe(DEFAULT_RECIPE)?)
            }
            Err(e) => Err(e),
        }
    }

    /// Save a user recipe (create or replace).
    pub fn recipe_save(&self, recipe: &Recipe) -> Result<RecipeInfo> {
        let lib = self.library()?;
        if lib.builtin_recipes().iter().any(|r| r.id == recipe.id) {
            return Err(Error::Invalid(format!(
                "{} is a built-in recipe; save yours under another id",
                recipe.id
            )));
        }
        lib.plan(recipe)?;
        let dir = self.recipes_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| Error::io(format!("creating {}", dir.display()), e))?;
        let path = dir.join(format!("{}.toml", recipe.id));
        std::fs::write(&path, recipe.to_toml()?)
            .map_err(|e| Error::io(format!("writing {}", path.display()), e))?;
        Ok(self.info(&lib, recipe, false, &self.active_recipe_id()))
    }

    pub fn recipe_delete(&self, id: &str) -> Result<()> {
        if self.active_recipe_id() == id {
            return Err(Error::Invalid(format!(
                "{id} is the recipe in use; switch to another one first"
            )));
        }
        let path = self.recipes_dir().join(format!("{id}.toml"));
        if !path.is_file() {
            return Err(Error::Invalid(format!("{id} is not one of your recipes")));
        }
        std::fs::remove_file(&path)
            .map_err(|e| Error::io(format!("removing {}", path.display()), e))
    }

    /// Check a recipe text without saving it.
    pub fn recipe_check(&self, text: &str) -> Result<RecipeInfo> {
        let lib = self.library()?;
        let r = lib.import(text)?;
        Ok(self.info(&lib, &r, false, &self.active_recipe_id()))
    }

    /// Import a shared recipe and save it. Patches that are not in Unilux's
    /// library are flagged in `custom_code`: show them before building.
    pub fn recipe_import(&self, text: &str) -> Result<RecipeInfo> {
        let r = self.library()?.import(text)?;
        let info = self.recipe_save(&r)?;
        if info.custom_code {
            self.notice(Severity::Warn, format!("Recipe {} brings its own Wine patches. They run inside Somnium: read them before building.", r.id));
        }
        Ok(info)
    }

    /// A self-contained copy to share: every patch inlined.
    pub fn recipe_export(&self, id: &str) -> Result<String> {
        self.library()?.export(&self.recipe(id)?)
    }

    /// The Proton a recipe runs on: its pin (`provider:latest` resolved
    /// to the newest stable release), else the one picked in Unilux.
    pub async fn recipe_proton(&self, recipe: &Recipe) -> Result<ProtonId> {
        match recipe.proton() {
            Some((provider, tag)) if tag == "latest" => {
                let releases = self.manager().releases(&provider, false).await?;
                releases
                    .iter()
                    .find(|r| !r.prerelease)
                    .map(|r| r.id())
                    .ok_or_else(|| Error::Invalid(format!("no stable {provider} release found")))
            }
            Some((provider, tag)) => Ok(ProtonId::new(&provider, &tag)),
            None => match self.current_proton() {
                Some(p) => Ok(p),
                None => self.catalog(false).await.recommended(),
            },
        }
    }

    /// Where `id`'s patches would come from with the active recipe.
    pub fn bundle_source(&self, plan: &Plan, id: &ProtonId) -> Source {
        unilux_patches::choose(
            plan.needs_build(),
            plan.recipe.id == DEFAULT_RECIPE,
            self.catalog_now().resolve(id).first().map(|m| m.status),
            self.recipe_bundle(plan, id).is_file(),
        )
    }

    /// The bundle `plan` builds for `id`, built or not.
    pub fn recipe_bundle(&self, plan: &Plan, id: &ProtonId) -> PathBuf {
        unilux_patches::builder::built_bundle(&self.inner.paths.cache_dir, id, &plan.bundle_key(id))
    }

    /// Build a recipe's bundle for a Proton (default: the active recipe,
    /// on the Proton it runs on). Reuses the cached bundle when the
    /// recipe, the Proton and the builder are unchanged.
    pub async fn recipe_build(
        &self,
        id: Option<&str>,
        proton: Option<ProtonId>,
        force: bool,
    ) -> Result<PathBuf> {
        let lib = self.library()?;
        let recipe = match id {
            Some(id) => self.recipe(id)?,
            None => self.active_plan()?.recipe,
        };
        let plan = lib.plan(&recipe)?;
        let proton = match proton {
            Some(p) => p,
            None => self.recipe_proton(&recipe).await?,
        };
        if !plan.needs_build() {
            return Err(Error::Invalid(format!(
                "recipe {} compiles nothing",
                recipe.id
            )));
        }
        let out = self.recipe_bundle(&plan, &proton);
        if out.is_file() && !force {
            self.notice(
                Severity::Info,
                format!("Bundle for {} on {proton} is already built", recipe.id),
            );
            return Ok(out);
        }
        self.patch_build_plan(&plan, proton).await
    }

    /// Make a recipe the one in use: record it, move to its Proton if it
    /// pins one, build its bundle when nothing fits, then repair. Rolls
    /// back the selection when that fails.
    pub async fn recipe_use(&self, id: &str) -> Result<()> {
        let recipe = self.recipe(id)?;
        self.library()?.plan(&recipe)?;
        let previous = self.config().recipe;
        let mut cfg = self.config();
        cfg.recipe = (id != DEFAULT_RECIPE).then(|| id.to_string());
        self.set_config(cfg)?;
        let proton = match self.recipe_proton(&recipe).await {
            Ok(p) => p,
            Err(e) => return self.restore_recipe(previous, e),
        };
        if let Err(e) = self.proton_use(proton, false).await {
            return self.restore_recipe(previous, e);
        }
        Ok(())
    }

    fn restore_recipe(&self, previous: Option<String>, e: Error) -> Result<()> {
        let mut cfg = self.config();
        cfg.recipe = previous;
        self.set_config(cfg)?;
        Err(e)
    }
}
