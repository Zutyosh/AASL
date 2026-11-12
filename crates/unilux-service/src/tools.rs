//! Proton, patch and VR operations on the service.

use std::path::PathBuf;

use serde::Serialize;
use unilux_components::proton::{InstalledProton, PROVIDERS, Provider};
use unilux_components::{ProtonId, Release};
use unilux_core::{Error, Result, Severity};
use unilux_install::{Mode, Options};
use unilux_patches::builder::{self, BuildPlan, Engine};
use unilux_patches::{Catalog, PatchStatus, Plan, Source as PatchSource, TargetStatus};
use unilux_system::VrStatus;

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
            unilux_install::InstallState::load(&self.inner.paths.state_file()).unwrap_or_default();
        state
            .proton_id()
            .or_else(|| self.config().proton.and_then(|p| ProtonId::parse(&p).ok()))
    }

    pub fn vr_status(&self) -> VrStatus {
        let names: Vec<String> = unilux_system::procs::list_processes()
            .into_iter()
            .map(|p| p.name)
            .collect();
        unilux_system::vr_status(self.inner.host.as_ref(), &names)
    }

    /// CachyOS's game-performance, when launches can run inside it.
    pub fn performance_wrapper(&self) -> Option<PathBuf> {
        unilux_runner::performance_wrapper(self.inner.host.as_ref())
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
            &unilux_components::Downloader::new(),
            &self.config().catalog_url,
        )
        .await;
        if let Some(w) = warning {
            self.notice(Severity::Warn, w);
        }
        *self.inner.catalog.lock().unwrap() = Some(c.clone());
        c
    }

    pub(crate) fn manager(&self) -> unilux_components::ProtonManager {
        unilux_install::proton_manager(
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
                patches: unilux_patches::bundle::status(&p.path),
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
                let _ = events.send(unilux_core::Event::Progress {
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
        unilux_install::InstallState::load(&self.inner.paths.state_file())
            .unwrap_or_default()
            .proton
    }

    fn record_proton(&self, config: Option<String>, state: Option<String>) -> Result<()> {
        let mut cfg = self.config();
        cfg.proton = config;
        self.set_config(cfg)?;
        let file = self.inner.paths.state_file();
        let mut st = unilux_install::InstallState::load(&file).unwrap_or_default();
        st.proton = state;
        st.save(&file)
    }

    /// What can build patch bundles here: podman, docker, or the host when
    /// it has the Wine cross toolchain. None means building is impossible.
    pub fn patch_builder(&self) -> Option<Engine> {
        let host = self.inner.host.as_ref();
        Engine::detect(|b| unilux_system::find_in_path(host, b).is_some()).or_else(|| {
            unilux_system::find_in_path(host, "x86_64-w64-mingw32-gcc").map(|_| Engine::Host)
        })
    }

    /// The command that installs podman on this distribution, if known.
    pub fn podman_install_hint(&self) -> Option<String> {
        unilux_system::Distro::detect(self.inner.host.as_ref())?.install_hint("podman")
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
        unilux_components::Downloader::new()
            .fetch(
                unilux_components::sources::BACKGROUND_URL,
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
                sha256: unilux_patches::recipe::sha256_hex(&text),
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
        let dl = unilux_components::Downloader::new();
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
        unilux_patches::bundle::restore(&unilux_install::proton_dir(&self.inner.paths, &id))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use unilux_components::ProtonId;
    use unilux_components::proton::InstalledProton;
    use unilux_core::Paths;
    use unilux_patches::Catalog;
    use unilux_patches::builder::Engine;
    use unilux_system::FakeHost;

    use crate::Service;

    fn svc_with(home: &std::path::Path, host: FakeHost) -> Service {
        let h = home.display().to_string();
        let paths = Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
        let s = Service::builder(paths)
            .host(Arc::new(host))
            .log_file(false)
            .build()
            .unwrap();
        // Never reach the network for the catalog.
        *s.inner.catalog.lock().unwrap() = Some(Catalog::embedded());
        s
    }

    fn svc(home: &std::path::Path) -> Service {
        svc_with(home, FakeHost::new())
    }

    /// What a finished download leaves on disk.
    fn fake_install(s: &Service, id: &ProtonId) {
        let dir = s.manager().dir_for(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("proton"), "#!/bin/sh\n").unwrap();
        let rec = InstalledProton {
            id: id.clone(),
            variant: "x86_64".into(),
            asset: format!("{}.tar.gz", id.tag),
            installed_at: 1,
            path: dir.clone(),
        };
        std::fs::write(
            dir.join(".unilux-proton.json"),
            serde_json::to_vec(&rec).unwrap(),
        )
        .unwrap();
    }

    fn record_state(s: &Service, proton: &str) {
        let file = s.paths().state_file();
        let mut st = unilux_install::InstallState::load(&file).unwrap_or_default();
        st.proton = Some(proton.into());
        st.save(&file).unwrap();
    }

    #[tokio::test]
    async fn current_proton_prefers_the_install_record() {
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path());
        assert_eq!(s.current_proton(), None);
        let mut cfg = s.config();
        cfg.proton = Some("ge:GE-Proton10-1".into());
        s.set_config(cfg).unwrap();
        assert_eq!(
            s.current_proton(),
            Some(ProtonId::new("ge", "GE-Proton10-1"))
        );
        // Schema 1 stored a bare GE tag.
        record_state(&s, "GE-Proton9-27");
        assert_eq!(
            s.current_proton(),
            Some(ProtonId::new("ge", "GE-Proton9-27"))
        );
    }

    #[tokio::test]
    async fn installed_list_and_remove_guards() {
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path());
        let used = ProtonId::new("ge", "GE-Proton10-1");
        let spare = ProtonId::new("ge", "GE-Proton9-27");
        fake_install(&s, &used);
        fake_install(&s, &spare);
        // A folder Unilux did not install is not listed and not removable.
        let foreign = ProtonId::new("ge", "Custom");
        std::fs::create_dir_all(s.manager().dir_for(&foreign)).unwrap();
        record_state(&s, &used.to_string());

        let list = s.proton_installed();
        assert_eq!(list.len(), 2, "{list:?}");
        assert!(list.iter().any(|p| p.proton.id == used && p.in_use));
        assert!(list.iter().any(|p| p.proton.id == spare && !p.in_use));

        let err = s.proton_remove(&used).unwrap_err().to_string();
        assert!(err.contains("Switch to another one"), "{err}");
        assert!(s.manager().is_installed(&used));
        assert!(s.proton_remove(&foreign).is_err());
        assert!(s.manager().dir_for(&foreign).is_dir(), "left alone");

        s.proton_remove(&spare).unwrap();
        assert!(!s.manager().dir_for(&spare).exists());
        assert_eq!(s.proton_installed().len(), 1);
    }

    #[tokio::test]
    async fn proton_use_records_the_choice_before_install() {
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path());
        let id = ProtonId::new("ge", "GE-Proton10-1");
        s.proton_use(id.clone(), true).await.unwrap();
        assert_eq!(s.config().proton.as_deref(), Some("ge:GE-Proton10-1"));
        assert_eq!(s.current_proton(), Some(id));
        assert!(
            !s.status().installed,
            "nothing to repair before the first install"
        );
    }

    #[tokio::test]
    async fn proton_use_respects_a_pinned_recipe() {
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path());
        let pinned = "format = 1\nid = \"pinned\"\ntitle = \"Pinned\"\nproton = \"ge:GE-Proton10-1\"\ningredients = [\"ytdlp-android-client\"]\n";
        s.recipe_import(pinned).unwrap();
        let mut cfg = s.config();
        cfg.recipe = Some("pinned".into());
        s.set_config(cfg).unwrap();

        let other = ProtonId::new("ge", "GE-Proton9-27");
        let err = s.proton_use(other, true).await.unwrap_err().to_string();
        assert!(err.contains("pins ge:GE-Proton10-1"), "{err}");
        assert_eq!(s.config().proton, None, "nothing recorded");

        s.proton_use(ProtonId::new("ge", "GE-Proton10-1"), true)
            .await
            .unwrap();
        assert_eq!(s.config().proton.as_deref(), Some("ge:GE-Proton10-1"));
    }

    #[tokio::test]
    async fn patch_builder_order() {
        let d = tempfile::tempdir().unwrap();
        let none = svc(d.path());
        assert!(none.patch_builder().is_none());
        let host = |files: &[&str]| {
            files
                .iter()
                .fold(FakeHost::new(), |h, f| h.with_file(f, ""))
        };
        let both = svc_with(d.path(), host(&["/usr/bin/docker", "/usr/bin/podman"]));
        assert!(matches!(both.patch_builder(), Some(Engine::Podman)));
        let docker = svc_with(d.path(), host(&["/usr/bin/docker"]));
        assert!(matches!(docker.patch_builder(), Some(Engine::Docker)));
        let mingw = svc_with(d.path(), host(&["/usr/bin/x86_64-w64-mingw32-gcc"]));
        assert!(matches!(mingw.patch_builder(), Some(Engine::Host)));
    }
}
