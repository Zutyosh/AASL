//! Reactive state shared by every screen, refreshed from the service.

use std::future::Future;
use std::time::Duration;

use aasl_core::{Event, Severity};
use aasl_diag::Finding;
use aasl_service::{InstalledView, ReleaseView, Status};
use freya::prelude::*;

use crate::{approvals, svc};

#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Launcher,
    Wizard,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Section {
    General,
    Proton,
    Patches,
    Vr,
    Diagnostics,
    Logs,
    Agents,
}

#[derive(Clone, Copy)]
pub struct Ui {
    pub screen: State<Screen>,
    pub section: State<Section>,
    pub status: State<Status>,
    pub step: State<Option<String>>,
    pub done_steps: State<Vec<String>>,
    pub progress: State<Option<(String, u64, Option<u64>)>>,
    pub notices: State<Vec<(Severity, String)>>,
    pub error: State<Option<String>>,
    pub findings: State<Option<Vec<Finding>>>,
    pub logs: State<Vec<String>>,
    pub approvals: State<Vec<(u64, String)>>,
    pub installed: State<Vec<InstalledView>>,
    pub releases: State<Vec<ReleaseView>>,
    pub provider: State<&'static str>,
}

impl Ui {
    pub fn notice(mut self, severity: Severity, msg: impl Into<String>) {
        let mut n = self.notices.write();
        n.push((severity, msg.into()));
        let extra = n.len().saturating_sub(5);
        n.drain(..extra);
    }

    /// Run a service call on tokio and report its error in the banner.
    pub fn run<T: Send + 'static>(
        mut self,
        fut: impl Future<Output = aasl_core::Result<T>> + Send + 'static,
        on_ok: impl FnOnce(T) + 'static,
    ) {
        self.error.set(None);
        spawn(async move {
            match tokio::spawn(fut).await {
                Ok(Ok(v)) => on_ok(v),
                Ok(Err(e)) => self.error.set(Some(e.to_string())),
                Err(e) => self.error.set(Some(e.to_string())),
            }
            self.status.set(svc().status());
            self.installed.set(svc().proton_installed());
        });
    }

    pub fn load_releases(mut self, refresh: bool) {
        let provider = *self.provider.read();
        self.run(
            async move { svc().proton_releases(provider, refresh).await },
            move |v| self.releases.set(v),
        );
    }
}

pub fn use_ui() -> Ui {
    let first = !svc().status().installed;
    let mut ui = Ui {
        screen: use_state(move || {
            if first {
                Screen::Wizard
            } else {
                Screen::Launcher
            }
        }),
        section: use_state(|| Section::General),
        status: use_state(|| svc().status()),
        step: use_state(|| None),
        done_steps: use_state(Vec::new),
        progress: use_state(|| None),
        notices: use_state(Vec::new),
        error: use_state(|| None),
        findings: use_state(|| None),
        logs: use_state(Vec::new),
        approvals: use_state(Vec::new),
        installed: use_state(|| svc().proton_installed()),
        releases: use_state(Vec::new),
        provider: use_state(|| "ge"),
    };

    use_future(move || async move {
        let mut rx = svc().subscribe();
        loop {
            let ev = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };
            match ev {
                Event::OperationStarted { .. } => ui.done_steps.set(Vec::new()),
                Event::StepStarted { step } => ui.step.set(Some(step)),
                Event::StepFinished { step, .. } => ui.done_steps.write().push(step),
                Event::Progress { task, done, total } => ui.progress.set(Some((task, done, total))),
                Event::Notice { severity, message } => ui.notice(severity, message),
                Event::OperationFinished { .. } => {
                    ui.step.set(None);
                    ui.progress.set(None);
                    ui.status.set(svc().status());
                }
                _ => {}
            }
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            ui.status.set(svc().status());
            ui.approvals.set(approvals::pending());
            if *ui.section.read() == Section::Logs && *ui.screen.read() == Screen::Settings {
                ui.logs.set(
                    svc()
                        .tail_logs(None, 400)
                        .into_iter()
                        .map(|l| l.line)
                        .collect(),
                );
            }
        }
    });

    ui
}
