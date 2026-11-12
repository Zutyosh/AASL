//! Unilux's own updates, from GitHub releases.

use unilux_core::{Error, Event, Result, Severity};
use unilux_update::{Applied, Channel, InstallKind, Release, Updater, Version};
use serde::Serialize;

use crate::Service;

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: Version,
    pub channel: Channel,
    /// The release to update to; None when up to date.
    pub available: Option<Release>,
    pub install: InstallKind,
    /// Whether `update_apply` can do it in place (false for packages).
    pub can_apply: bool,
}

impl Service {
    fn updater(&self) -> Updater {
        let mut u = Updater::new();
        u.api_base = self.inner.sources.github_api.clone();
        if let Some(t) = self.config().github_token.filter(|t| !t.is_empty()) {
            u.token = Some(t);
        }
        u
    }

    fn channel(&self) -> Channel {
        if self.config().update_channel.eq_ignore_ascii_case("beta") {
            Channel::Beta
        } else {
            Channel::Stable
        }
    }

    pub fn install_kind(&self) -> InstallKind {
        let appimage = std::env::var("APPIMAGE").ok();
        let exe = std::env::current_exe().unwrap_or_default();
        unilux_update::detect_install(appimage.as_deref(), &exe, |d| {
            let probe = d.join(format!(".unilux-write-test-{}", std::process::id()));
            let ok = std::fs::write(&probe, b"").is_ok();
            let _ = std::fs::remove_file(&probe);
            ok
        })
    }

    /// Ask GitHub whether a newer Unilux exists. The answer is kept for
    /// `update_cached`.
    pub async fn update_check(&self) -> Result<UpdateInfo> {
        let current = Version::current();
        let channel = self.channel();
        let available = self.updater().check(current, channel).await?;
        let install = self.install_kind();
        let info = UpdateInfo {
            current,
            channel,
            can_apply: !matches!(install, InstallKind::Managed { .. }),
            available,
            install,
        };
        if let Some(r) = &info.available {
            self.notice(
                Severity::Info,
                format!("Unilux {} is available (you have {current}).", r.version),
            );
        }
        *self.inner.update.lock().unwrap() = Some(info.clone());
        Ok(info)
    }

    pub fn update_cached(&self) -> Option<UpdateInfo> {
        self.inner.update.lock().unwrap().clone()
    }

    /// Download and install the available update over this copy of Unilux.
    /// The new version runs after a restart.
    pub async fn update_apply(&self) -> Result<Applied> {
        let info = match self.update_cached() {
            Some(i) => i,
            None => self.update_check().await?,
        };
        let Some(release) = info.available else {
            return Err(Error::Invalid(format!(
                "Unilux {} is the newest version.",
                info.current
            )));
        };
        let updater = self.updater();
        let events = self.inner.events.clone();
        let kind = info.install.clone();
        let mut last = None;
        let applied = self
            .exclusive("self_update", async move {
                updater
                    .apply(&release, &kind, move |done, total| {
                        // One event per percent, not per network chunk.
                        let pct = total.filter(|t| *t > 0).map(|t| done * 100 / t);
                        if pct != last {
                            last = pct;
                            let _ = events.send(Event::Progress {
                                task: "Unilux update".into(),
                                done,
                                total,
                            });
                        }
                    })
                    .await
            })
            .await?;
        *self.inner.update.lock().unwrap() = None;
        self.notice(
            Severity::Info,
            format!(
                "Unilux {} installed. Restart Unilux to use it.",
                applied.version
            ),
        );
        Ok(applied)
    }
}
