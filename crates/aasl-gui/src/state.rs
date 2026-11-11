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
    Recipes,
    Vr,
    Diagnostics,
    Logs,
    Agents,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Ui {
    pub background: State<Option<std::path::PathBuf>>,
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
    pub update: State<Option<aasl_service::UpdateInfo>>,
    pub restart_ready: State<bool>,
    /// Interface language; changing it remounts the screen.
    pub lang: State<crate::i18n::Lang>,
    /// The compile screen was dismissed for the current build.
    pub build_hidden: State<bool>,
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
        // Window-level: the button that started it may unmount (Play turns
        // into Stop), which would cancel a component task.
        spawn_in_window(async move {
            match tokio::spawn(fut).await {
                Ok(Ok(v)) => on_ok(v),
                Ok(Err(e)) => self.error.set(Some(e.to_string())),
                Err(e) => self.error.set(Some(e.to_string())),
            }
            self.status.set(svc().status());
            self.installed.set(svc().proton_installed());
        });
    }

    pub fn update_now(mut self) {
        self.run(async { svc().update_apply().await }, move |_| {
            self.restart_ready.set(true)
        });
    }

    pub fn load_releases(mut self, refresh: bool) {
        let provider = *self.provider.read();
        self.run(
            async move { svc().proton_releases(provider, refresh).await },
            move |v| {
                if *self.provider.peek() == provider {
                    self.releases.set(v)
                }
            },
        );
    }
}

#[cfg(test)]
thread_local! {
    /// Snapshot test: open Settings on this section, and show the compile screen.
    pub static TEST_SECTION: std::cell::Cell<Option<Section>> = const { std::cell::Cell::new(None) };
    pub static TEST_BUILD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn test_section() -> Option<Section> {
    #[cfg(test)]
    {
        return TEST_SECTION.with(|s| s.get());
    }
    #[allow(unreachable_code)]
    None
}

/// The compile screen is up: a build runs (or the snapshot test asks for it).
pub fn building(ui: Ui) -> bool {
    #[cfg(test)]
    if TEST_BUILD.with(|b| b.get()) {
        return true;
    }
    ui.status.read().operation.as_deref() == Some("patch_build") && !*ui.build_hidden.read()
}

pub fn use_ui() -> Ui {
    let first = !svc().status().installed;
    let test = test_section();
    let mut ui = Ui {
        background: use_state(|| svc().background()),
        screen: use_state(move || {
            if test.is_some() {
                Screen::Settings
            } else if first {
                Screen::Wizard
            } else {
                Screen::Launcher
            }
        }),
        section: use_state(move || test.unwrap_or(Section::General)),
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
        update: use_state(|| svc().update_cached()),
        restart_ready: use_state(|| false),
        lang: use_state(crate::i18n::current),
        build_hidden: use_state(|| false),
    };

    use_future(move || async move {
        let mut rx = svc().subscribe();
        loop {
            let ev = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Wine's output can outrun us; resync what was missed.
                    let st = svc().status();
                    if st.operation.is_none() {
                        ui.step.set(None);
                        ui.progress.set(None);
                    }
                    ui.status.set(st);
                    continue;
                }
                Err(_) => break,
            };
            match ev {
                Event::OperationStarted { operation } => {
                    ui.done_steps.set(Vec::new());
                    if operation == "patch_build" {
                        ui.build_hidden.set(false);
                    }
                }
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
        if let Ok(Ok(p)) = tokio::spawn(async { svc().fetch_background().await }).await
            && ui.background.peek().as_ref() != Some(&p)
        {
            ui.background.set(Some(p));
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            ui.status.set(svc().status());
            ui.approvals.set(approvals::pending());
            let cached = svc().update_cached();
            if ui.update.peek().as_ref().map(|u| &u.available)
                != cached.as_ref().map(|u| &u.available)
            {
                ui.update.set(cached);
            }
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
