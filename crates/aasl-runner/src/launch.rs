//! Starting Somnium, waiting for the headset, stopping a session.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use aasl_components::sources::LAUNCHER_REL_PATH;
use aasl_core::exec::{CommandSpec, spawn};
use aasl_core::{Config, Error, Event, Events, Paths, Result, Severity};
use aasl_system::{Host, procs};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use crate::env::{LaunchEnv, build_env};

#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    /// Start even when no OpenXR runtime is active (desktop mode).
    pub allow_flat: bool,
}

pub struct Launcher {
    pub paths: Paths,
    pub config: Config,
    pub host: Arc<dyn Host>,
    pub events: Events,
    pub cancel: Arc<AtomicBool>,
}

impl Launcher {
    pub fn prefix(&self) -> PathBuf {
        self.config
            .prefix
            .clone()
            .unwrap_or_else(|| self.paths.default_prefix())
    }

    fn notice(&self, severity: Severity, message: impl Into<String>) {
        let _ = self.events.send(Event::Notice {
            severity,
            message: message.into(),
        });
    }

    fn proton(&self) -> Result<PathBuf> {
        let state = aasl_install::InstallState::load(&self.paths.state_file())?;
        let v = state
            .proton
            .or_else(|| self.config.proton.clone())
            .ok_or_else(|| {
                Error::Invalid("Somnium Space is not installed. Run the installer first.".into())
            })?;
        let id = aasl_components::ProtonId::parse(&v)?;
        Ok(aasl_install::proton_dir(&self.paths, &id))
    }

    /// Everything the launch would use, without starting anything.
    pub fn plan(&self) -> Result<(CommandSpec, LaunchEnv)> {
        let prefix = self.prefix();
        let launcher = prefix.join(LAUNCHER_REL_PATH);
        if !launcher.exists() {
            return Err(Error::Invalid(format!(
                "{} is missing. Run a repair or a fresh install.",
                launcher.display()
            )));
        }
        let proton = self.proton()?;
        let umu = aasl_install::umu_run(&self.paths);
        if !umu.exists() || !proton.exists() {
            return Err(Error::Invalid(
                "umu-run or Proton is missing. Run a repair.".into(),
            ));
        }
        let env = build_env(self.host.as_ref(), &self.config, &proton, &prefix);
        let mut spec = CommandSpec::new(umu).arg(launcher.display().to_string());
        spec.env = env.env.clone();
        Ok((spec, env))
    }

    fn vr(&self) -> aasl_system::VrStatus {
        let names: Vec<String> = procs::list_processes()
            .into_iter()
            .map(|p| p.name)
            .collect();
        aasl_system::vr_status(self.host.as_ref(), &names)
    }

    /// Wait until the VR stack is ready: runtime active, its service up,
    /// an OpenVR layer registered where Proton needs one. WiVRn writes its
    /// manifest only once a headset connects, so launching earlier would
    /// silently start in desktop mode.
    async fn wait_for_runtime(&self) -> Result<bool> {
        if self.config.openxr_env.is_some() {
            return Ok(true);
        }
        let mut last: Vec<String> = Vec::new();
        let deadline =
            std::time::Instant::now() + Duration::from_secs(self.config.headset_timeout_secs);
        loop {
            let st = self.vr();
            if st.ready {
                if !last.is_empty() {
                    self.notice(Severity::Info, "VR runtime ready.");
                }
                return Ok(true);
            }
            if st.blockers != last {
                for b in &st.blockers {
                    self.notice(Severity::Warn, format!("Waiting: {b}"));
                }
                last = st.blockers;
            }
            if std::time::Instant::now() >= deadline {
                return Ok(false);
            }
            if self.cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Start Somnium and return the child; the caller decides whether to
    /// wait on it. Fails if a session is already running in the prefix.
    pub async fn launch(&self, opts: &LaunchOptions) -> Result<tokio::process::Child> {
        let running = procs::prefix_processes(&self.prefix());
        if !running.is_empty() {
            return Err(Error::Busy(format!(
                "Somnium Space is already running ({} processes). Stop it first.",
                running.len()
            )));
        }
        let (_, _) = self.plan()?;
        if !opts.allow_flat && self.config.wait_for_headset && !self.wait_for_runtime().await? {
            return Err(Error::Invalid(
                "The VR runtime did not become ready in time. Run `aasl vr` to see what is missing, or launch in desktop mode.".into(),
            ));
        }
        let (spec, env) = self.plan()?;
        self.notice(
            Severity::Info,
            format!("Detected OpenXR runtime: {}", env.runtime_label),
        );
        spawn(&spec, &self.events)
    }
}

/// Stop every process in a prefix: SIGTERM, a grace period, then
/// `wineserver -k`, then SIGKILL for anything left.
pub async fn stop_prefix(prefix: &Path, proton: Option<&Path>, events: &Events) -> Result<usize> {
    let initial = procs::prefix_processes(prefix);
    if initial.is_empty() {
        return Ok(0);
    }
    for p in &initial {
        let _ = kill(Pid::from_raw(p.pid as i32), Signal::SIGTERM);
    }
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if procs::prefix_processes(prefix).is_empty() {
            return Ok(initial.len());
        }
    }
    if let Some(proton) = proton {
        let ws = proton.join("files/bin/wineserver");
        if ws.exists() {
            let spec = CommandSpec::new(ws)
                .arg("-k")
                .env("WINEPREFIX", prefix.display().to_string());
            if let Ok(mut c) = spawn(&spec, events) {
                let _ = tokio::time::timeout(Duration::from_secs(5), c.wait()).await;
            }
        }
    }
    for p in procs::prefix_processes(prefix) {
        let _ = kill(Pid::from_raw(p.pid as i32), Signal::SIGKILL);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let left = procs::prefix_processes(prefix);
    if !left.is_empty() {
        return Err(Error::Busy(format!(
            "{} processes would not stop. Logging out and back in clears them.",
            left.len()
        )));
    }
    Ok(initial.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stop_kills_processes_in_prefix() {
        let d = tempfile::tempdir().unwrap();
        let pfx = d.path().join("pfx");
        std::fs::create_dir_all(&pfx).unwrap();
        let mut child = tokio::process::Command::new("sleep")
            .arg("30")
            .env("WINEPREFIX", &pfx)
            .spawn()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let ev = aasl_core::event_bus();
        let reaper = tokio::spawn(async move { child.wait().await });
        let n = stop_prefix(&pfx, None, &ev).await.unwrap();
        assert_eq!(n, 1);
        assert!(reaper.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn plan_requires_install() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path().display().to_string();
        let l = Launcher {
            paths: Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap(),
            config: Config::default(),
            host: Arc::new(aasl_system::FakeHost::new()),
            events: aasl_core::event_bus(),
            cancel: Arc::default(),
        };
        assert!(l.plan().unwrap_err().to_string().contains("install"));
    }
}
