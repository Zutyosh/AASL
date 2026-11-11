//! Proton, patch and VR operations on the service.

use std::path::PathBuf;

use aasl_components::proton::{InstalledProton, PROVIDERS, Provider};
use aasl_components::{ProtonId, Release};
use aasl_core::{Error, Result, Severity};
use aasl_install::{Mode, Options};
use aasl_patches::builder::{self, BuildPlan, Engine};
use aasl_patches::{Catalog, PatchStatus, Plan, Source as PatchSource, TargetStatus};
use aasl_system::VrStatus;
use serde::Serialize;

use crate::Service;

fn no_builder(id: &ProtonId, hint: Option<String>) -> Error {
    let install = hint.map(|c| format!(" ({c})")).unwrap_or_default();
    Error::Invalid(format!(
        "{id} has no ready-made patches for this recipe, and building them needs podman or docker. Install podman{install}, or pick a Proton marked \"patches verified\"."
    ))
}

/// One patch file of the ingredient library.
#[derive(Debug, Clone, Serialize)]
pub struct PatchFile {
    pub ingredient: String,
    pub name: String,
    pub subject: String,
    pub sha256: String,
}

/// One upstream release as the UI shows it.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseView {
    pub id: ProtonId,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub installed: bool,
    pub in_use: bool,
    /// Best patch bundle for it, if any.
    pub patches: Option<TargetStatus>,
    /// Published, built here, or neither.
    pub source: PatchSource,
    pub size: u64,
    /// The build this CPU would get, if there is one.
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledView {
    pub proton: InstalledProton,
    pub path: PathBuf,
    pub in_use: bool,
    pub patches: PatchStatus,
}

impl Service {
    /// The Proton this install uses: the recorded one, else the config's.
    pub fn current_proton(&self) -> Option<ProtonId> {
        let state =
            aasl_install::InstallState::load(&self.inner.paths.state_file()).unwrap_or_default();
        state
            .proton_id()
            .or_else(|| self.config().proton.and_then(|p| ProtonId::parse(&p).ok()))
    }

    pub fn vr_status(&self) -> VrStatus {
        let names: Vec<String> = aasl_system::procs::list_processes()
            .into_iter()
            .map(|p| p.name)
            .collect();
        aasl_system::vr_status(self.inner.host.as_ref(), &names)
    }

    pub fn providers(&self) -> &'static [Provider] {
        PROVIDERS
    }

    /// The catalog already loaded, or the built-in one.
    pub fn catalog_now(&self) -> Catalog {
        self.inner
            .catalog
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(Catalog::embedded)
    }

    /// Fetch the live catalog (and keep it for later calls).
    pub async fn catalog(&self, refresh: bool) -> Catalog {
        if !refresh && let Some(c) = self.inner.catalog.lock().unwrap().clone() {
            return c;
        }
        let (c, warning) = Catalog::fetch(
            &aasl_components::Downloader::new(),
            &self.config().catalog_url,
        )
        .await;
        if let Some(w) = warning {
            self.notice(Severity::Warn, w);
        }
        *self.inner.catalog.lock().unwrap() = Some(c.clone());
        c
    }

    pub(crate) fn manager(&self) -> aasl_components::ProtonManager {
        aasl_install::proton_manager(
            &self.inner.paths,
            &self.config(),
            &self.inner.sources.github_api,
        )
    }

    pub async fn proton_releases(&self, provider: &str, refresh: bool) -> Result<Vec<ReleaseView>> {
        let mgr = self.manager();
        let releases: Vec<Release> = mgr.releases(provider, refresh).await?;
        let cat = self.catalog(false).await;
        let current = self.current_proton();
        Ok(releases
            .iter()
            .map(|r| {
                let id = r.id();
                ReleaseView {
                    installed: mgr.is_installed(&id),
                    in_use: current.as_ref() == Some(&id),
                    patches: cat.resolve(&id).first().map(|m| m.status),
                    source: self.patch_source(&id),
                    size: r.pick(self.inner.cpu).map(|(_, a, _)| a.size).unwrap_or(0),
                    variant: r.pick(self.inner.cpu).map(|(v, _, _)| v.id.to_string()),
                    published_at: r.published_at.clone(),
                    prerelease: r.prerelease,
                    id,
                }
            })
            .collect())
    }

    pub fn proton_installed(&self) -> Vec<InstalledView> {
        let current = self.current_proton();
        self.manager()
            .installed()
            .into_iter()
            .map(|p| InstalledView {
                in_use: current.as_ref() == Some(&p.id),
                patches: aasl_patches::bundle::status(&p.path),
                path: p.path.clone(),
                proton: p,
            })
            .collect()
    }

    /// Download and install a Proton build without switching to it.
    pub async fn proton_install(&self, id: ProtonId) -> Result<PathBuf> {
        let mgr = self.manager();
        let cpu = self.inner.cpu;
        let events = self.inner.events.clone();
        let task = id.tag.clone();
        self.exclusive("proton_install", async move {
            let release = mgr.find(&id).await?;
            mgr.install(&release, cpu, move |done, total| {
                let _ = events.send(aasl_core::Event::Progress {
                    task: task.clone(),
                    done,
                    total,
                });
            })
            .await
        })
        .await
    }

    pub fn proton_remove(&self, id: &ProtonId) -> Result<()> {
        if self.current_proton().as_ref() == Some(id) {
            return Err(Error::Invalid(format!(
                "{id} is the Proton Somnium uses. Switch to another one first."
            )));
        }
        self.manager().remove(id)
    }

    /// Make Somnium use this Proton: record it, then repair so it is
    /// downloaded and patched. When no published bundle fits, the patches
    /// are built for it first (podman or docker), unless `force` skips
    /// patching altogether.
    pub async fn proton_use(&self, id: ProtonId, force: bool) -> Result<()> {
        let plan = self.active_plan()?;
        if let Some((provider, tag)) = plan.recipe.proton()
            && tag != "latest"
            && ProtonId::new(&provider, &tag) != id
        {
            return Err(Error::Invalid(format!(
                "recipe {} pins {provider}:{tag}; edit the recipe or switch to another one to use {id}",
                plan.recipe.id
            )));
        }
        self.catalog(false).await;
        if !force && self.bundle_source(&plan, &id) == PatchSource::MustBuild {
            if self.patch_builder().is_none() {
                return Err(no_builder(&id, self.podman_install_hint()));
            }
            self.notice(
                Severity::Info,
                format!("No ready-made patches for {id} with recipe {}; building them now. Wine compiles, so this takes a while.", plan.recipe.id),
            );
            self.patch_build_plan(&plan, id.clone()).await?;
        }
        let was_installed = self.status().installed;
        let previous = (self.config().proton, self.recorded_proton());
        self.record_proton(Some(id.to_string()), Some(id.to_string()))?;
        if was_installed && let Err(e) = self.repair().await {
            // The switch failed: keep Somnium on the Proton that worked.
            self.record_proton(previous.0, previous.1)?;
            return Err(e);
        }
        Ok(())
    }

    fn recorded_proton(&self) -> Option<String> {
        aasl_install::InstallState::load(&self.inner.paths.state_file())
            .unwrap_or_default()
            .proton
    }

    fn record_proton(&self, config: Option<String>, state: Option<String>) -> Result<()> {
        let mut cfg = self.config();
        cfg.proton = config;
        self.set_config(cfg)?;
        let file = self.inner.paths.state_file();
        let mut st = aasl_install::InstallState::load(&file).unwrap_or_default();
        st.proton = state;
        st.save(&file)
    }

    /// What can build patch bundles here: podman, docker, or the host when
    /// it has the Wine cross toolchain. None means building is impossible.
    pub fn patch_builder(&self) -> Option<Engine> {
        let host = self.inner.host.as_ref();
        Engine::detect(|b| aasl_system::find_in_path(host, b).is_some()).or_else(|| {
            aasl_system::find_in_path(host, "x86_64-w64-mingw32-gcc").map(|_| Engine::Host)
        })
    }

    /// The command that installs podman on this distribution, if known.
    pub fn podman_install_hint(&self) -> Option<String> {
        aasl_system::Distro::detect(self.inner.host.as_ref())?.install_hint("podman")
    }

    /// Where a Proton's patches would come from with the active recipe.
    pub fn patch_source(&self, id: &ProtonId) -> PatchSource {
        match self.active_plan() {
            Ok(plan) => self.bundle_source(&plan, id),
            Err(_) => PatchSource::MustBuild,
        }
    }

    /// The launcher background (official key art), cached for a week.
    pub fn background(&self) -> Option<PathBuf> {
        let p = self.inner.paths.cache_dir.join("background.jpg");
        p.is_file().then_some(p)
    }

    pub async fn fetch_background(&self) -> Result<PathBuf> {
        let p = self.inner.paths.cache_dir.join("background.jpg");
        let fresh = std::fs::metadata(&p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() < 7 * 86400);
        if fresh {
            return Ok(p);
        }
        let tmp = self.inner.paths.cache_dir.join("background.download.jpg");
        aasl_components::Downloader::new()
            .fetch(
                aasl_components::sources::BACKGROUND_URL,
                &tmp,
                None,
                |_, _| {},
            )
            .await?;
        std::fs::rename(&tmp, &p).map_err(|e| Error::io("saving the background", e))?;
        Ok(p)
    }

    /// Every patch of the ingredient library, with its subject and hash.
    pub fn patch_list(&self) -> Result<Vec<PatchFile>> {
        Ok(self
            .library()?
            .patch_files()
            .into_iter()
            .map(|(ingredient, name, text)| PatchFile {
                subject: text
                    .lines()
                    .find_map(|l| l.strip_prefix("Subject: "))
                    .map(|s| {
                        s.trim_start_matches(|c| c != ']')
                            .trim_start_matches(']')
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_default(),
                sha256: aasl_patches::recipe::sha256_hex(&text),
                ingredient,
                name,
            })
            .collect())
    }

    /// Full text of one library patch, by its file name.
    pub fn patch_read(&self, name: &str) -> Result<String> {
        self.library()?
            .patch_files()
            .into_iter()
            .find(|(_, n, _)| n == name)
            .map(|(_, _, t)| t)
            .ok_or_else(|| Error::Invalid(format!("no patch named {name}; see patch_list")))
    }

    /// Everything building the active recipe for `id` would do, without doing it.
    pub async fn patch_plan(&self, id: &ProtonId) -> Result<BuildPlan> {
        let plan = self.active_plan()?;
        self.build_plan(&plan, id).await
    }

    async fn build_plan(&self, plan: &Plan, id: &ProtonId) -> Result<BuildPlan> {
        let dl = aasl_components::Downloader::new();
        let (proton_repo, wine) =
            builder::resolve_wine(&dl, &self.inner.sources.github_api, id).await?;
        let engine = self.patch_builder().unwrap_or(Engine::Host);
        let root = self.inner.paths.cache_dir.join("patch-builder");
        let key = plan.bundle_key(id);
        let patches_dir = root.join("recipe").join(&key);
        let series = plan.materialise(&patches_dir)?;
        Ok(BuildPlan {
            proton: id.clone(),
            proton_repo,
            wine,
            series,
            patches_dir,
            work_dir: root.join("work"),
            out_dir: root.join("out"),
            bundle_name: builder::bundle_name(id, &key),
            with_ffmpeg: false,
            engine,
        })
    }

    /// Build the active recipe's bundle for `id`. See `recipe_build`.
    pub async fn patch_build(&self, id: ProtonId) -> Result<PathBuf> {
        self.recipe_build(None, Some(id), true).await
    }

    /// Build `plan` for `id` in a container. Long: Wine compiles. The
    /// output streams on the event bus. Returns the bundle path.
    pub(crate) async fn patch_build_plan(&self, plan: &Plan, id: ProtonId) -> Result<PathBuf> {
        if self.patch_builder().is_none() {
            return Err(no_builder(&id, self.podman_install_hint()));
        }
        let plan = self.build_plan(plan, &id).await?;
        let exec = self.inner.exec.clone();
        let events = self.inner.events.clone();
        let tools = self.inner.paths.cache_dir.join("patch-builder/context");
        let logs = self.inner.logs.clone();
        self.exclusive("patch_build", async move {
            builder::materialise_tools(&tools)?;
            std::fs::create_dir_all(&plan.work_dir)
                .map_err(|e| Error::io("creating work dir", e))?;
            std::fs::create_dir_all(&plan.out_dir).map_err(|e| Error::io("creating out dir", e))?;
            if let Some(img) = plan.image_command(&tools) {
                let code = exec.run(&img, &events).await?;
                if code != Some(0) {
                    return Err(Error::Invalid(
                        "building the patch-builder image failed; see the log".into(),
                    ));
                }
            }
            let code = exec.run(&plan.command(&tools), &events).await?;
            let out = plan.out_dir.join(&plan.bundle_name);
            if code != Some(0) || !out.exists() {
                // build.sh ends with "FATAL <reason>"; show that, not just the exit code.
                let reason = logs
                    .tail(None, 200)
                    .into_iter()
                    .rev()
                    .find_map(|l| l.line.strip_prefix("FATAL ").map(str::to_string));
                return Err(Error::Invalid(match reason {
                    Some(r) => format!("the patch build failed: {r}"),
                    None => format!("the patch build failed (exit {code:?}); see Logs"),
                }));
            }
            Ok(out)
        })
        .await
    }

    /// Install a bundle file into the current Proton (through a repair).
    pub async fn patch_apply(&self, bundle: PathBuf) -> Result<()> {
        let inst = self.installer();
        let opts = Options {
            local_bundle: Some(bundle),
            recipe: self.active_plan().ok(),
            ..Default::default()
        };
        self.exclusive("patch_apply", async move {
            inst.run(Mode::Repair, &opts).await.map(|_| ())
        })
        .await
    }

    /// Put stock Proton files back in the current Proton.
    pub fn patch_restore(&self) -> Result<usize> {
        let id = self
            .current_proton()
            .ok_or_else(|| Error::Invalid("no Proton selected".into()))?;
        aasl_patches::bundle::restore(&aasl_install::proton_dir(&self.inner.paths, &id))
    }
}
