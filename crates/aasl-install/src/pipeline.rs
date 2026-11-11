//! The install state machine. Each step checks the filesystem first and
//! skips when its work is already done, so any run can be repeated or
//! resumed after a crash.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use aasl_components::extract::extract_to_staging;
use aasl_components::sources::LAUNCHER_REL_PATH;
use aasl_components::{Checksum, CpuLevel, Downloader, ProtonId, Sources};
use aasl_core::exec::Exec;
use aasl_core::{Config, Error, Event, Events, IoContext, Paths, Result, Severity};
use aasl_patches::catalog::{Catalog, TargetStatus};
use aasl_patches::{ARCHIVE_FORMAT, bundle};
use aasl_system::Host;
use serde::Serialize;

use crate::state::{InstallState, now};
use crate::{desktop, proton_dir, proton_manager, umu_command, umu_run};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    CheckDeps,
    StopRunning,
    Catalog,
    Umu,
    Proton,
    Patches,
    Prefix,
    SomniumInstaller,
    DesktopEntry,
}

impl Step {
    pub fn id(self) -> &'static str {
        match self {
            Self::CheckDeps => "check_deps",
            Self::StopRunning => "stop_running",
            Self::Catalog => "catalog",
            Self::Umu => "umu",
            Self::Proton => "proton",
            Self::Patches => "patches",
            Self::Prefix => "prefix",
            Self::SomniumInstaller => "somnium_installer",
            Self::DesktopEntry => "desktop_entry",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Install,
    /// Re-check and re-apply everything except the prefix and Somnium.
    Repair,
}

impl Mode {
    pub fn steps(self) -> &'static [Step] {
        use Step::*;
        match self {
            Mode::Install => &[
                CheckDeps,
                StopRunning,
                Catalog,
                Umu,
                Proton,
                Patches,
                Prefix,
                SomniumInstaller,
                DesktopEntry,
            ],
            Mode::Repair => &[
                CheckDeps,
                StopRunning,
                Catalog,
                Umu,
                Proton,
                Patches,
                DesktopEntry,
            ],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Somnium's official Windows installer when the user picked one;
    /// None downloads it from somniumspace.com.
    pub somnium_installer: Option<PathBuf>,
    /// Use this patch bundle instead of the catalog's.
    pub local_bundle: Option<PathBuf>,
    /// The compatibility recipe, checked. Decides the bundle with
    /// `aasl_patches::choose`; None behaves like the default recipe
    /// without a bundle built here.
    pub recipe: Option<aasl_patches::Plan>,
    /// Path written into the desktop entry's Exec line.
    pub aasl_exe: Option<PathBuf>,
}

pub struct Installer {
    pub paths: Paths,
    pub config: Config,
    pub sources: Sources,
    pub host: Arc<dyn Host>,
    pub exec: Arc<dyn Exec>,
    pub events: Events,
    pub downloader: Downloader,
    pub cancel: Arc<AtomicBool>,
    pub cpu: CpuLevel,
}

struct Run<'a> {
    inst: &'a Installer,
    opts: &'a Options,
    state: InstallState,
    catalog: Option<Catalog>,
}

impl Installer {
    pub fn prefix(&self) -> PathBuf {
        self.config
            .prefix
            .clone()
            .unwrap_or_else(|| self.paths.default_prefix())
    }

    fn emit(&self, e: Event) {
        let _ = self.events.send(e);
    }

    fn notice(&self, severity: Severity, message: impl Into<String>) {
        self.emit(Event::Notice {
            severity,
            message: message.into(),
        });
    }

    fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    pub async fn run(&self, mode: Mode, opts: &Options) -> Result<InstallState> {
        let mut run = Run {
            inst: self,
            opts,
            state: InstallState::load(&self.paths.state_file())?,
            catalog: None,
        };
        if mode == Mode::Repair && !self.prefix().join(LAUNCHER_REL_PATH).exists() {
            return Err(Error::Invalid(format!(
                "no Somnium install found in {}. Pick the folder that contains drive_c, or run a fresh install.",
                self.prefix().display()
            )));
        }
        for step in mode.steps() {
            self.check_cancel()?;
            self.emit(Event::StepStarted {
                step: step.id().into(),
            });
            let skipped = run.step(*step).await?;
            run.state.steps.insert(step.id().into(), now());
            run.state.save(&self.paths.state_file())?;
            self.emit(Event::StepFinished {
                step: step.id().into(),
                skipped,
            });
        }
        if mode == Mode::Install {
            run.state.installed_at = Some(now());
        }
        run.state.save(&self.paths.state_file())?;
        Ok(run.state)
    }

    /// Remove components and the desktop entry. The prefix (your Somnium
    /// install and its settings) is removed only when asked.
    pub fn uninstall(&self, remove_prefix: bool) -> Result<Vec<PathBuf>> {
        let mut removed = Vec::new();
        let mut rm = |p: PathBuf| -> Result<()> {
            if p.is_dir() {
                std::fs::remove_dir_all(&p).ctx(|| format!("removing {}", p.display()))?;
                removed.push(p);
            } else if p.exists() {
                std::fs::remove_file(&p).ctx(|| format!("removing {}", p.display()))?;
                removed.push(p);
            }
            Ok(())
        };
        rm(self.paths.components_dir())?;
        rm(self.paths.desktop_entry())?;
        rm(self.paths.state_file())?;
        rm(self.paths.downloads_dir())?;
        if remove_prefix {
            rm(self.prefix())?;
        }
        Ok(removed)
    }
}

impl Run<'_> {
    /// Returns true when the step had nothing to do.
    async fn step(&mut self, step: Step) -> Result<bool> {
        match step {
            Step::CheckDeps => self.check_deps(),
            Step::StopRunning => self.stop_running(),
            Step::Catalog => self.catalog().await,
            Step::Umu => self.umu().await,
            Step::Proton => self.proton().await,
            Step::Patches => self.patches().await,
            Step::Prefix => self.prefix().await,
            Step::SomniumInstaller => self.somnium_installer().await,
            Step::DesktopEntry => self.desktop_entry(),
        }
    }

    fn check_deps(&mut self) -> Result<bool> {
        let host = self.inst.host.as_ref();
        let hint = |tool: &str| {
            aasl_system::Distro::detect(host)
                .and_then(|d| d.install_hint(tool))
                .map(|h| format!(" Install it with: {h}"))
                .unwrap_or_default()
        };
        match aasl_system::python_version(host) {
            None => {
                return Err(Error::Invalid(format!(
                    "python3 was not found. umu-run needs Python 3.10 or newer.{}",
                    hint("python3")
                )));
            }
            Some(v) if !v.supported() => {
                return Err(Error::Invalid(format!(
                    "python3 is {v}, and umu-run needs 3.10 or newer. Installing a second Python does not help unless it becomes python3 on your PATH."
                )));
            }
            Some(_) => {}
        }
        if aasl_system::vulkan_icds(host).is_empty() {
            self.inst.notice(
                Severity::Warn,
                "No Vulkan driver manifest was found. Proton needs a working Vulkan driver.",
            );
        }
        Ok(false)
    }

    fn stop_running(&mut self) -> Result<bool> {
        let procs = aasl_system::procs::prefix_processes(&self.inst.prefix());
        if procs.is_empty() {
            return Ok(true);
        }
        let list = procs
            .iter()
            .map(|p| format!("{} ({})", p.name, p.pid))
            .collect::<Vec<_>>()
            .join(", ");
        Err(Error::Busy(format!(
            "Somnium Space is running ({list}). Close it, or run `aasl stop`, then try again."
        )))
    }

    async fn catalog(&mut self) -> Result<bool> {
        let (cat, warning) =
            Catalog::fetch(&self.inst.downloader, &self.inst.config.catalog_url).await;
        if let Some(w) = warning {
            self.inst.notice(Severity::Warn, w);
        }
        let id = match &self.inst.config.proton {
            Some(p) => ProtonId::parse(p)?,
            None => cat.recommended()?,
        };
        if id.major().is_some_and(|m| m < 9) {
            self.inst.notice(Severity::Warn, format!("{id} is older than Proton 9, which cannot start VR. Somnium will only run in desktop mode."));
        }
        self.state.proton = Some(id.to_string());
        self.catalog = Some(cat);
        Ok(false)
    }

    fn proton_id(&self) -> Result<ProtonId> {
        self.state
            .proton_id()
            .ok_or_else(|| Error::Invalid("Proton version unknown".into()))
    }

    fn progress(&self, task: &str) -> impl FnMut(u64, Option<u64>) + Send + use<> {
        let events = self.inst.events.clone();
        let task = task.to_string();
        let mut last = 0u64;
        move |done, total| {
            if done == 0 || done.saturating_sub(last) >= 1 << 20 || Some(done) == total {
                last = done;
                let _ = events.send(Event::Progress {
                    task: task.clone(),
                    done,
                    total,
                });
            }
        }
    }

    async fn umu(&mut self) -> Result<bool> {
        let s = &self.inst.sources;
        let bin = umu_run(&self.inst.paths);
        let stamp = self.inst.paths.umu_dir().join("version");
        if bin.exists() && std::fs::read_to_string(&stamp).is_ok_and(|v| v.trim() == s.umu_version)
        {
            self.state.umu = Some(s.umu_version.clone());
            return Ok(true);
        }
        let dl = self
            .inst
            .paths
            .downloads_dir()
            .join(format!("umu-{}.tar", s.umu_version));
        let sum = Checksum::Sha256(s.umu_sha256.clone());
        self.inst
            .downloader
            .fetch(&s.umu_url(), &dl, Some(&sum), self.progress("umu-run"))
            .await?;
        let dir = self.inst.paths.umu_dir();
        let parent = self.inst.paths.components_dir();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let st = extract_to_staging(&dl, &parent)?;
            let src = st.path().join("umu/umu-run");
            if !src.is_file() {
                return Err(Error::Invalid(
                    "umu-run is missing from the umu archive".into(),
                ));
            }
            std::fs::create_dir_all(&dir).ctx(|| format!("creating {}", dir.display()))?;
            let dest = dir.join("umu-run");
            std::fs::copy(&src, &dest).ctx(|| "installing umu-run".into())?;
            set_executable(&dest)
        })
        .await
        .map_err(|e| Error::Invalid(e.to_string()))??;
        std::fs::write(&stamp, &s.umu_version).ctx(|| "writing umu version".into())?;
        self.state.umu = Some(s.umu_version.clone());
        Ok(false)
    }

    async fn proton(&mut self) -> Result<bool> {
        let id = self.proton_id()?;
        let mgr = proton_manager(
            &self.inst.paths,
            &self.inst.config,
            &self.inst.sources.github_api,
        );
        if mgr.is_installed(&id) {
            return Ok(true);
        }
        let release = mgr.find(&id).await?;
        self.inst
            .notice(Severity::Info, format!("Downloading {id}"));
        mgr.install(&release, self.inst.cpu, self.progress(&id.tag))
            .await?;
        Ok(false)
    }

    async fn patches(&mut self) -> Result<bool> {
        let id = self.proton_id()?;
        let proton = proton_dir(&self.inst.paths, &id);
        let cat = self
            .catalog
            .as_ref()
            .ok_or_else(|| Error::Invalid("catalog not loaded".into()))?;
        let published = cat.resolve(&id).into_iter().next();
        let recipe = self.opts.recipe.as_ref();
        let built = recipe.map(|p| {
            aasl_patches::builder::built_bundle(&self.inst.paths.cache_dir, &id, &p.bundle_key(&id))
        });
        let source = aasl_patches::choose(
            recipe.is_none_or(|p| p.needs_build()),
            recipe.is_none_or(|p| p.recipe.id == aasl_patches::recipe::DEFAULT_RECIPE),
            published.as_ref().map(|m| m.status),
            built.as_ref().is_some_and(|b| b.is_file()),
        );
        let local = match (&self.opts.local_bundle, source) {
            (Some(l), _) => Some(l.clone()),
            (None, aasl_patches::Source::NotNeeded) => {
                if bundle::record(&proton).is_some() {
                    let n = bundle::restore(&proton)?;
                    self.inst.notice(
                        Severity::Info,
                        format!("The recipe compiles nothing: restored {n} stock Proton files"),
                    );
                }
                self.state.patch_bundle = None;
                return Ok(true);
            }
            (None, aasl_patches::Source::Built) => built,
            (None, aasl_patches::Source::MustBuild) if recipe.is_some() => {
                return Err(Error::Invalid(format!(
                    "Recipe {} has no bundle for {id} yet. Build it first (Settings, or `aasl recipe build`).",
                    recipe.map(|p| p.recipe.id.as_str()).unwrap_or_default()
                )));
            }
            (None, _) => None,
        };
        let (bundle_id, archive) = match local {
            Some(local) => {
                let id = format!("local:{}", local.display());
                let st = bundle::status(&proton);
                if self.opts.local_bundle.is_none()
                    && st.bundle.as_deref() == Some(id.as_str())
                    && st.intact
                {
                    self.state.patch_bundle = Some(id);
                    return Ok(true);
                }
                (id, local)
            }
            None => {
                let Some(m) = published else {
                    return Err(Error::Invalid(format!(
                        "No patch bundle exists for {id} yet, and Somnium cannot sign in without one. Choose a verified Proton ({}) or let AASL build the patches from Settings > Proton.",
                        cat.verified_protons().join(", ")
                    )));
                };
                if m.bundle.archive_format != ARCHIVE_FORMAT {
                    return Err(Error::Invalid(format!(
                        "bundle {} is format {}, this AASL installs format {ARCHIVE_FORMAT}. Update AASL.",
                        m.bundle.id, m.bundle.archive_format
                    )));
                }
                if m.status == TargetStatus::Untested {
                    self.inst.notice(Severity::Warn, format!("Bundle {} has not been tested with {id}. If Somnium misbehaves, switch to a verified Proton.", m.bundle.id));
                }
                let st = bundle::status(&proton);
                if st.bundle.as_deref() == Some(m.bundle.id.as_str()) && st.intact {
                    self.state.patch_bundle = Some(m.bundle.id.clone());
                    return Ok(true);
                }
                let dl = self
                    .inst
                    .paths
                    .downloads_dir()
                    .join(format!("{}.tar.gz", m.bundle.id));
                let sum = m.bundle.sha256.clone().map(Checksum::Sha256);
                self.inst
                    .downloader
                    .fetch(&m.bundle.url, &dl, sum.as_ref(), self.progress("patches"))
                    .await?;
                (m.bundle.id.clone(), dl)
            }
        };
        let parent = self.inst.paths.components_dir();
        let bid = bundle_id.clone();
        let n = tokio::task::spawn_blocking(move || -> Result<usize> {
            let st = extract_to_staging(&archive, &parent)?;
            let plan = bundle::plan(st.path(), &proton, ARCHIVE_FORMAT)?;
            bundle::apply_recorded(&plan, &proton, &bid)
        })
        .await
        .map_err(|e| Error::Invalid(e.to_string()))??;
        self.inst.notice(
            Severity::Info,
            format!("Installed {n} patched files from {bundle_id}"),
        );
        self.state.patch_bundle = Some(bundle_id);
        Ok(false)
    }

    fn umu_spec(&self) -> Result<aasl_core::exec::CommandSpec> {
        let proton = proton_dir(&self.inst.paths, &self.proton_id()?);
        Ok(umu_command(
            &umu_run(&self.inst.paths),
            &proton,
            &self.inst.prefix(),
        ))
    }

    async fn prefix(&mut self) -> Result<bool> {
        let prefix = self.inst.prefix();
        self.state.prefix = Some(prefix.clone());
        if prefix.join("system.reg").exists() {
            return Ok(true);
        }
        std::fs::create_dir_all(&prefix).ctx(|| format!("creating {}", prefix.display()))?;
        let spec = self.umu_spec()?.arg("wineboot").arg("-u");
        let code = self.inst.exec.run(&spec, &self.inst.events).await?;
        if code != Some(0) || !prefix.join("system.reg").exists() {
            return Err(Error::Invalid(format!(
                "creating the Wine prefix failed (exit code {code:?}). See the log for Proton's output."
            )));
        }
        Ok(false)
    }

    async fn somnium_installer(&mut self) -> Result<bool> {
        let prefix = self.inst.prefix();
        let launcher = prefix.join(LAUNCHER_REL_PATH);
        if launcher.exists() {
            return Ok(true);
        }
        let exe = match self.opts.somnium_installer.clone() {
            Some(exe) => exe,
            None => self.download_installer().await?,
        };
        if !exe.is_file() || !is_windows_exe(&exe) {
            return Err(Error::Invalid(format!(
                "{} is not a Windows installer (.exe)",
                exe.display()
            )));
        }
        let lang = crate::inno_language(
            self.inst
                .config
                .locale
                .clone()
                .or_else(crate::system_locale)
                .as_deref()
                .unwrap_or("en"),
        );
        self.inst.notice(
            Severity::Info,
            format!("Installing Somnium Space silently ({lang})."),
        );
        let spec = self
            .umu_spec()?
            .arg(exe.display().to_string())
            .arg("/VERYSILENT")
            .arg("/SUPPRESSMSGBOXES")
            .arg("/NORESTART")
            .arg("/SP-")
            .arg(format!("/LANG={lang}"));
        let exe_name = exe
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let finished_early = tokio::select! {
            r = self.inst.exec.run(&spec, &self.inst.events) => { r?; false }
            _ = wait_for_install(&launcher, &prefix, &exe_name) => true,
        };
        if finished_early {
            // The installer started Somnium's launcher (or left Wine
            // running): the files are in place, so close the prefix.
            self.close_prefix().await;
        }
        if !launcher.exists() {
            return Err(Error::Invalid(format!(
                "Somnium's installer finished, but {LAUNCHER_REL_PATH} was not created. Run the install again."
            )));
        }
        Ok(false)
    }

    async fn download_installer(&mut self) -> Result<PathBuf> {
        let dest = self
            .inst
            .paths
            .downloads_dir()
            .join("SomniumSpaceSetup.exe");
        self.inst
            .notice(Severity::Info, "Downloading Somnium Space's installer");
        let _ = std::fs::remove_file(&dest);
        self.inst
            .downloader
            .fetch(
                &self.inst.sources.somnium_installer_url,
                &dest,
                None,
                self.progress("somnium"),
            )
            .await?;
        Ok(dest)
    }

    /// Stop everything left in the prefix: wineserver -k, then SIGKILL.
    async fn close_prefix(&self) {
        if let Ok(id) = self.proton_id() {
            let ws = proton_dir(&self.inst.paths, &id).join("files/bin/wineserver");
            if ws.exists() {
                let spec = aasl_core::exec::CommandSpec::new(ws)
                    .arg("-k")
                    .env("WINEPREFIX", self.inst.prefix().display().to_string());
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    self.inst.exec.run(&spec, &self.inst.events),
                )
                .await;
            }
        }
        for p in aasl_system::procs::prefix_processes(&self.inst.prefix()) {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(p.pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }

    fn desktop_entry(&mut self) -> Result<bool> {
        let exe = self
            .opts
            .aasl_exe
            .clone()
            .or_else(|| std::env::current_exe().ok())
            .unwrap_or_else(|| PathBuf::from("aasl"));
        desktop::write_entry(&self.inst.paths.desktop_entry(), &exe, None)?;
        Ok(false)
    }
}

/// Resolves once Somnium's launcher exists and no installer process is
/// left in the prefix (two checks in a row), even if Wine keeps running
/// because the installer started Somnium at the end.
async fn wait_for_install(launcher: &Path, prefix: &Path, exe_name: &str) {
    let mut calm = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let installing = aasl_system::procs::prefix_processes(prefix)
            .iter()
            .any(|p| {
                let c = p.cmdline.to_ascii_lowercase();
                c.contains(".tmp")
                    || (!exe_name.is_empty() && c.contains(&exe_name.to_ascii_lowercase()))
            });
        if launcher.exists() && !installing {
            calm += 1;
            if calm >= 2 {
                return;
            }
        } else {
            calm = 0;
        }
    }
}

fn is_windows_exe(p: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 2];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && &magic == b"MZ"
}

fn set_executable(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))
        .ctx(|| format!("chmod {}", p.display()))
}
