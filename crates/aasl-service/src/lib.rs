//! The single entry point for the GUI, the CLI and the agent server.
//! Each front end sees the same state, the same events and the same
//! actions, so an agent can never do something a user could not see.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

mod tools;
mod update;

pub use tools::{InstalledView, ReleaseView};
pub use update::UpdateInfo;

use aasl_components::{CpuLevel, Downloader, Sources};
use aasl_core::exec::{Exec, TokioExec};
use aasl_core::{Config, Error, Event, Events, Paths, Result, Severity};
use aasl_diag::{DiagContext, Finding, FixId};
use aasl_install::{InstallState, Installer, Mode, Options};
use aasl_patches::{Catalog, PatchStatus};
use aasl_runner::logs::LogLine;
use aasl_runner::{LaunchOptions, Launcher, LogBuffer};
use aasl_system::procs::ProcInfo;
use aasl_system::{Host, OpenXrRuntime, RealHost, SystemReport, VrStatus};
use serde::Serialize;

pub use aasl_diag::FixId as Fix;
pub use aasl_install::Options as InstallOptions;
pub use aasl_runner::LaunchOptions as LaunchOpts;

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub aasl_version: &'static str,
    pub installed: bool,
    pub prefix: PathBuf,
    pub proton: Option<String>,
    pub operation: Option<String>,
    pub session_pid: Option<u32>,
    pub runtime: Option<OpenXrRuntime>,
    pub vr: VrStatus,
    pub patches: Option<PatchStatus>,
    pub cpu: CpuLevel,
    pub state: InstallState,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixOutcome {
    pub fix: FixId,
    pub description: &'static str,
    pub destructive: bool,
    pub applied: bool,
    pub message: String,
}

struct Inner {
    paths: Paths,
    config: RwLock<Config>,
    host: Arc<dyn Host>,
    exec: Arc<dyn Exec>,
    sources: Sources,
    events: Events,
    logs: LogBuffer,
    op_lock: tokio::sync::Mutex<()>,
    op_name: Mutex<Option<String>>,
    cancel: Arc<AtomicBool>,
    session: Mutex<Option<u32>>,
    cpu: CpuLevel,
    catalog: Mutex<Option<Catalog>>,
    update: Mutex<Option<UpdateInfo>>,
}

#[derive(Clone)]
pub struct Service {
    inner: Arc<Inner>,
}

pub struct Builder {
    paths: Paths,
    host: Arc<dyn Host>,
    exec: Arc<dyn Exec>,
    sources: Sources,
    log_file: bool,
}

impl Builder {
    pub fn host(mut self, h: Arc<dyn Host>) -> Self {
        self.host = h;
        self
    }
    pub fn exec(mut self, e: Arc<dyn Exec>) -> Self {
        self.exec = e;
        self
    }
    pub fn sources(mut self, s: Sources) -> Self {
        self.sources = s;
        self
    }
    pub fn log_file(mut self, on: bool) -> Self {
        self.log_file = on;
        self
    }
    /// Needs a running tokio runtime (the log writer is a task).
    pub fn build(self) -> Result<Service> {
        let config = Config::load(&self.paths.config_file())?;
        let events = aasl_core::event_bus();
        let logs = LogBuffer::new(5000);
        let file = self.log_file.then(|| self.paths.launch_log());
        logs.attach(&events, file.as_deref());
        Ok(Service {
            inner: Arc::new(Inner {
                paths: self.paths,
                config: RwLock::new(config),
                host: self.host,
                exec: self.exec,
                sources: self.sources,
                events,
                logs,
                op_lock: tokio::sync::Mutex::new(()),
                op_name: Mutex::new(None),
                cancel: Arc::new(AtomicBool::new(false)),
                session: Mutex::new(None),
                cpu: CpuLevel::detect(),
                catalog: Mutex::new(None),
                update: Mutex::new(None),
            }),
        })
    }
}

impl Service {
    pub fn builder(paths: Paths) -> Builder {
        Builder {
            paths,
            host: Arc::new(RealHost),
            exec: Arc::new(TokioExec),
            sources: Sources::default(),
            log_file: true,
        }
    }

    /// Service for the current user with real host access.
    pub fn from_env() -> Result<Self> {
        let paths = Paths::from_env().ok_or_else(|| Error::Invalid("HOME is not set".into()))?;
        Self::builder(paths).build()
    }

    pub fn paths(&self) -> &Paths {
        &self.inner.paths
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    pub fn events(&self) -> &Events {
        &self.inner.events
    }

    pub fn config(&self) -> Config {
        self.inner.config.read().unwrap().clone()
    }

    pub fn set_config(&self, cfg: Config) -> Result<()> {
        cfg.save(&self.inner.paths.config_file())?;
        *self.inner.config.write().unwrap() = cfg;
        Ok(())
    }

    fn prefix(&self) -> PathBuf {
        self.config()
            .prefix
            .unwrap_or_else(|| self.inner.paths.default_prefix())
    }

    fn notice(&self, severity: Severity, message: impl Into<String>) {
        let _ = self.inner.events.send(Event::Notice {
            severity,
            message: message.into(),
        });
    }

    pub fn status(&self) -> Status {
        let state = InstallState::load(&self.inner.paths.state_file()).unwrap_or_default();
        let proton = self.current_proton();
        Status {
            vr: self.vr_status(),
            patches: proton.map(|p| {
                aasl_patches::bundle::status(&aasl_install::proton_dir(&self.inner.paths, &p))
            }),
            cpu: self.inner.cpu,
            aasl_version: env!("CARGO_PKG_VERSION"),
            installed: self
                .prefix()
                .join(aasl_components::sources::LAUNCHER_REL_PATH)
                .exists(),
            prefix: self.prefix(),
            proton: state.proton.clone(),
            operation: self.inner.op_name.lock().unwrap().clone(),
            session_pid: *self.inner.session.lock().unwrap(),
            runtime: aasl_system::active_runtime(self.inner.host.as_ref()),
            state,
        }
    }

    pub fn system(&self) -> SystemReport {
        aasl_system::probe(self.inner.host.as_ref())
    }

    pub fn processes(&self) -> Vec<ProcInfo> {
        aasl_system::procs::prefix_processes(&self.prefix())
    }

    pub fn tail_logs(&self, since: Option<u64>, limit: usize) -> Vec<LogLine> {
        self.inner.logs.tail(since, limit)
    }

    /// Ask the running operation to stop at its next step.
    pub fn cancel(&self) {
        self.inner.cancel.store(true, Ordering::Relaxed);
    }

    /// Run `f` as the only mutating operation, with start/finish events.
    async fn exclusive<T>(
        &self,
        name: &str,
        f: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        let _guard = self.inner.op_lock.try_lock().map_err(|_| {
            Error::Busy(
                self.inner
                    .op_name
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
            )
        })?;
        self.inner.cancel.store(false, Ordering::Relaxed);
        *self.inner.op_name.lock().unwrap() = Some(name.into());
        let _ = self.inner.events.send(Event::OperationStarted {
            operation: name.into(),
        });
        let r = f.await;
        let _ = self.inner.events.send(Event::OperationFinished {
            operation: name.into(),
            ok: r.is_ok(),
            message: r.as_ref().err().map(|e| e.to_string()),
        });
        *self.inner.op_name.lock().unwrap() = None;
        r
    }

    fn installer(&self) -> Installer {
        Installer {
            paths: self.inner.paths.clone(),
            config: self.config(),
            sources: self.inner.sources.clone(),
            host: self.inner.host.clone(),
            exec: self.inner.exec.clone(),
            events: self.inner.events.clone(),
            downloader: Downloader::new(),
            cancel: self.inner.cancel.clone(),
            cpu: self.inner.cpu,
        }
    }

    pub async fn install(&self, opts: Options) -> Result<InstallState> {
        self.exclusive("install", self.installer().run(Mode::Install, &opts))
            .await
    }

    pub async fn repair(&self) -> Result<InstallState> {
        self.exclusive(
            "repair",
            self.installer().run(Mode::Repair, &Options::default()),
        )
        .await
    }

    pub async fn uninstall(&self, remove_prefix: bool) -> Result<Vec<PathBuf>> {
        self.exclusive("uninstall", async {
            self.installer().uninstall(remove_prefix)
        })
        .await
    }

    /// Start Somnium. Returns its pid; a background task records the exit.
    pub async fn launch(&self, opts: LaunchOptions) -> Result<u32> {
        let launcher = Launcher {
            paths: self.inner.paths.clone(),
            config: self.config(),
            host: self.inner.host.clone(),
            events: self.inner.events.clone(),
            cancel: self.inner.cancel.clone(),
        };
        let mut child = self.exclusive("launch", launcher.launch(&opts)).await?;
        let pid = child.id().unwrap_or_default();
        *self.inner.session.lock().unwrap() = Some(pid);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let code = child.wait().await.ok().and_then(|s| s.code());
            let _ = inner.events.send(Event::ProcessExited { pid, code });
            *inner.session.lock().unwrap() = None;
        });
        Ok(pid)
    }

    /// Wait until the session started by `launch` ends.
    pub async fn wait_session(&self) {
        while self.inner.session.lock().unwrap().is_some() {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    pub async fn stop(&self) -> Result<usize> {
        let proton = self
            .current_proton()
            .map(|p| aasl_install::proton_dir(&self.inner.paths, &p));
        let prefix = self.prefix();
        let events = self.inner.events.clone();
        self.cancel();
        let n = aasl_runner::stop_prefix(&prefix, proton.as_deref(), &events).await?;
        Ok(n)
    }

    async fn github_reachable(&self) -> bool {
        let url = self.config().catalog_url;
        tokio::time::timeout(
            std::time::Duration::from_secs(6),
            Downloader::new().text(&url),
        )
        .await
        .is_ok_and(|r| r.is_ok())
    }

    fn diag_context(&self, network: Option<bool>) -> DiagContext {
        let state = InstallState::load(&self.inner.paths.state_file()).unwrap_or_default();
        let config = self.config();
        let proton = self.current_proton();
        let proton_dir = proton
            .as_ref()
            .map(|p| aasl_install::proton_dir(&self.inner.paths, p));
        let patches = proton_dir.as_ref().map(|d| aasl_patches::bundle::status(d));
        let patch_match = proton
            .as_ref()
            .and_then(|p| self.catalog_now().resolve(p).first().map(|m| m.status));
        let launch_log = std::fs::read_to_string(self.inner.paths.launch_log())
            .map(|t| {
                let lines: Vec<String> = t.lines().map(str::to_string).collect();
                let skip = lines.len().saturating_sub(2000);
                lines.into_iter().skip(skip).collect()
            })
            .unwrap_or_default();
        DiagContext {
            paths: self.inner.paths.clone(),
            prefix: self.prefix(),
            proton_dir,
            proton,
            patches,
            patch_match,
            vr: Some(self.vr_status()),
            config,
            state,
            system: self.system(),
            processes: self.processes(),
            github_reachable: network,
            launch_log,
            exists: Box::new(|p| p.exists()),
        }
    }

    /// Run every diagnostic. `network` also tests that GitHub answers.
    pub async fn doctor(&self, network: bool) -> Vec<Finding> {
        let net = if network {
            Some(self.github_reachable().await)
        } else {
            None
        };
        aasl_diag::run_all(&self.diag_context(net))
    }

    /// A JSON bug report with the home directory and user name removed.
    pub async fn report(&self) -> String {
        let ctx = self.diag_context(Some(self.github_reachable().await));
        let findings = aasl_diag::run_all(&ctx);
        aasl_diag::report::render(&ctx, &findings)
    }

    /// Apply one whitelisted fix. With `dry_run` nothing changes and the
    /// outcome says what would happen.
    pub async fn apply_fix(&self, fix: FixId, dry_run: bool) -> Result<FixOutcome> {
        let out = |applied: bool, message: String| FixOutcome {
            fix,
            description: fix.describe(),
            destructive: fix.destructive(),
            applied,
            message,
        };
        if dry_run {
            return Ok(out(false, format!("dry run: {}", fix.describe())));
        }
        self.notice(Severity::Info, format!("Applying fix {fix:?}"));
        match fix {
            FixId::Repair => {
                self.repair().await?;
                Ok(out(true, "Repair finished.".into()))
            }
            FixId::StopSession => {
                let n = self.stop().await?;
                Ok(out(true, format!("Stopped {n} processes.")))
            }
            FixId::ClearOpenxrOverride => {
                let mut cfg = self.config();
                cfg.openxr_env = None;
                self.set_config(cfg)?;
                Ok(out(true, "OpenXR detection is automatic again.".into()))
            }
            FixId::ClearDownloads => {
                let d = self.inner.paths.downloads_dir();
                if d.exists() {
                    std::fs::remove_dir_all(&d)
                        .map_err(|e| Error::io(format!("removing {}", d.display()), e))?;
                }
                Ok(out(true, "Downloads cleared.".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(home: &std::path::Path) -> Service {
        let h = home.display().to_string();
        let paths = Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
        Service::builder(paths)
            .host(Arc::new(aasl_system::FakeHost::new()))
            .log_file(false)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn update_check_reports_newer_release_and_install_kind() {
        use wiremock::matchers::path;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/repos/Zutyosh/AASL/releases"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"[{"tag_name":"v99.0.0","prerelease":false,"assets":[]}]"#),
            )
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let h = d.path().display().to_string();
        let paths = Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
        let s = Service::builder(paths)
            .host(Arc::new(aasl_system::FakeHost::new()))
            .sources(Sources {
                github_api: server.uri(),
                ..Sources::default()
            })
            .log_file(false)
            .build()
            .unwrap();
        let info = s.update_check().await.unwrap();
        assert_eq!(info.available.unwrap().tag, "v99.0.0");
        assert_eq!(info.current, aasl_update::Version::current());
        assert_eq!(
            s.update_cached().unwrap().available.unwrap().tag,
            "v99.0.0",
            "result kept for the UI"
        );
    }

    #[tokio::test]
    async fn status_and_doctor_on_empty_home() {
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path());
        let st = s.status();
        assert!(!st.installed);
        assert!(st.operation.is_none());
        let f = s.doctor(false).await;
        assert!(
            f.iter()
                .any(|f| f.check == "install" && f.severity == Severity::Error)
        );
    }

    #[tokio::test]
    async fn one_operation_at_a_time() {
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path());
        let s2 = s.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let held = tokio::spawn(async move {
            s2.exclusive("slow", async {
                let _ = rx.await;
                Ok(())
            })
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(s.status().operation.as_deref(), Some("slow"));
        let err = s.repair().await.unwrap_err();
        assert!(matches!(err, Error::Busy(ref n) if n == "slow"));
        tx.send(()).unwrap();
        held.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn fixes_dry_run_and_config_fix() {
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path());
        let o = s.apply_fix(FixId::StopSession, true).await.unwrap();
        assert!(!o.applied && o.destructive);
        let mut cfg = s.config();
        cfg.openxr_env = Some(Default::default());
        s.set_config(cfg).unwrap();
        s.apply_fix(FixId::ClearOpenxrOverride, false)
            .await
            .unwrap();
        assert!(
            Config::load(&s.paths().config_file())
                .unwrap()
                .openxr_env
                .is_none()
        );
    }
}
