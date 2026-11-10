//! Proton, patch and VR operations on the service.

use std::path::PathBuf;

use aasl_components::proton::{InstalledProton, PROVIDERS, Provider};
use aasl_components::{ProtonId, Release};
use aasl_core::{Error, Result, Severity};
use aasl_install::{Mode, Options};
use aasl_patches::builder::{self, BuildPlan, Engine};
use aasl_patches::series::{self, PatchFile};
use aasl_patches::{Catalog, PatchStatus, TargetStatus};
use aasl_system::VrStatus;
use serde::Serialize;

use crate::Service;

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

    fn manager(&self) -> aasl_components::ProtonManager {
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
    /// downloaded and patched. Refuses a Proton without any bundle unless
    /// `force` is set.
    pub async fn proton_use(&self, id: ProtonId, force: bool) -> Result<()> {
        let cat = self.catalog(false).await;
        if !force && cat.resolve(&id).is_empty() {
            return Err(Error::Invalid(format!(
                "No patch bundle exists for {id}; Somnium would not be able to sign in. Build one first (aasl patch build --proton {id}) or pass force."
            )));
        }
        let mut cfg = self.config();
        cfg.proton = Some(id.to_string());
        self.set_config(cfg)?;
        let mut state =
            aasl_install::InstallState::load(&self.inner.paths.state_file()).unwrap_or_default();
        state.proton = Some(id.to_string());
        state.save(&self.inner.paths.state_file())?;
        if self.status().installed {
            self.repair().await?;
        }
        Ok(())
    }

    /// Patch files: the checkout's `patches/` when running from source,
    /// else the copies compiled into AASL.
    pub fn patches_dir(&self) -> Result<PathBuf> {
        let dev = std::env::var("AASL_PATCHES_DIR").ok().map(PathBuf::from);
        if let Some(d) = dev.filter(|d| d.join("series.toml").exists()) {
            return Ok(d);
        }
        let d = self.inner.paths.cache_dir.join("patches");
        series::materialise(&d)?;
        Ok(d)
    }

    pub fn patch_list(&self) -> Result<Vec<PatchFile>> {
        series::list(&self.patches_dir()?)
    }

    /// Everything a build would do, without doing it.
    pub async fn patch_plan(
        &self,
        id: &ProtonId,
        with_ffmpeg: bool,
        only: &[String],
    ) -> Result<BuildPlan> {
        let dl = aasl_components::Downloader::new();
        let (proton_repo, wine) =
            builder::resolve_wine(&dl, &self.inner.sources.github_api, id).await?;
        let host = self.inner.host.as_ref();
        let engine = Engine::detect(|b| aasl_system::find_in_path(host, b).is_some())
            .unwrap_or(Engine::Host);
        let all = series::parse(
            &std::fs::read_to_string(self.patches_dir()?.join("series.toml"))
                .map_err(|e| Error::io("series.toml", e))?,
        )?;
        let root = self.inner.paths.cache_dir.join("patch-builder");
        Ok(BuildPlan {
            proton: id.clone(),
            proton_repo,
            wine,
            series: all
                .into_iter()
                .map(|s| s.id)
                .filter(|s| only.is_empty() || only.contains(s))
                .collect(),
            patches_dir: self.patches_dir()?,
            work_dir: root.join("work"),
            out_dir: root.join("out"),
            with_ffmpeg,
            engine,
        })
    }

    /// Build a bundle for `id` in a container. Long: Wine compiles. The
    /// output streams on the event bus. Returns the bundle path.
    pub async fn patch_build(
        &self,
        id: ProtonId,
        with_ffmpeg: bool,
        only: &[String],
    ) -> Result<PathBuf> {
        let plan = self.patch_plan(&id, with_ffmpeg, only).await?;
        if plan.engine == Engine::Host
            && aasl_system::find_in_path(self.inner.host.as_ref(), "x86_64-w64-mingw32-gcc")
                .is_none()
        {
            return Err(Error::Invalid(
                "Building patches needs podman or docker (recommended), or a host with the Wine build toolchain (mingw-w64-gcc). Install podman and try again.".into(),
            ));
        }
        let exec = self.inner.exec.clone();
        let events = self.inner.events.clone();
        let tools = self.inner.paths.cache_dir.join("patch-builder/context");
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
            let out = plan.out_dir.join(plan.bundle_name());
            if code != Some(0) || !out.exists() {
                return Err(Error::Invalid(format!(
                    "the patch build failed (exit {code:?}); the log names the step and the patch"
                )));
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
