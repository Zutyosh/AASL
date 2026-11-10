//! AASL desktop app.

mod app;
mod approvals;
mod i18n;
mod launcher;
mod settings;
mod state;
mod theme;
mod widgets;
mod wizard;

use std::sync::OnceLock;

use aasl_service::Service;
use freya::prelude::*;

static SERVICE: OnceLock<Service> = OnceLock::new();
/// How we were started, so a restart runs the updated file.
static LAUNCH_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Start a fresh AASL (the updated one) and exit this process.
pub fn restart() {
    let target = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| LAUNCH_PATH.get().cloned())
        .unwrap_or_else(|| "aasl-gui".into());
    match std::process::Command::new(&target).spawn() {
        Ok(_) => std::process::exit(0),
        Err(e) => eprintln!("aasl-gui: could not restart {}: {e}", target.display()),
    }
}

pub fn svc() -> &'static Service {
    SERVICE.get().expect("service initialised in main")
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let _guard = rt.enter();
    let service = match Service::from_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aasl-gui: {e}");
            std::process::exit(1);
        }
    };
    let server = aasl_agent::Server::new(service.clone()).with_confirm(approvals::hook());
    let socket = service.paths().socket();
    rt.spawn(async move {
        if let Err(e) = aasl_agent::serve_socket(server, &socket).await {
            eprintln!("aasl-gui: agent socket unavailable: {e}");
        }
    });
    let _ = LAUNCH_PATH.set(std::env::current_exe().unwrap_or_default());
    let warm = service.clone();
    rt.spawn(async move {
        warm.catalog(false).await;
        if warm.config().check_updates
            && let Err(e) = warm.update_check().await
        {
            eprintln!("aasl-gui: update check failed: {e}");
        }
    });
    let _ = SERVICE.set(service);
    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app::app)
                .with_title("AASL")
                .with_size(1180., 720.)
                .with_min_size(900., 600.)
                .with_background((0x12, 0x14, 0x17)),
        ),
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use freya_testing::prelude::*;

    use super::*;

    /// Renders the first-run wizard and the launcher against a fake home. Set
    /// AASL_SNAPSHOT_DIR to keep the PNGs.
    #[test]
    fn screens_render() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let _g = rt.enter();
        let home = tempfile::tempdir().unwrap();
        let h = home.path().display().to_string();
        let paths = aasl_core::Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
        let host = aasl_system::FakeHost::new()
            .with_env("HOME", &h)
            .with_command("python3 --version", "Python 3.13.1");
        let svc = Service::builder(paths.clone())
            .host(Arc::new(host))
            .log_file(false)
            .build()
            .unwrap();
        let _ = SERVICE.set(svc);
        let out = std::env::var("AASL_SNAPSHOT_DIR")
            .ok()
            .map(std::path::PathBuf::from);
        let size = Size2D::new(1180., 720.);

        let (mut wizard, _) = TestingRunner::new(app::app, size, |_| {}, 1.0);
        wizard.sync_and_update();
        wizard.poll(
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(1500),
        );
        assert!(!wizard.render().is_empty());
        if let Some(d) = &out {
            wizard.render_to_file(d.join("wizard.png"));
        }
        drop(wizard);

        let launcher_exe = paths
            .default_prefix()
            .join(aasl_components::sources::LAUNCHER_REL_PATH);
        std::fs::create_dir_all(launcher_exe.parent().unwrap()).unwrap();
        std::fs::write(&launcher_exe, "MZ").unwrap();
        let (mut launcher, _) = TestingRunner::new(app::app, size, |_| {}, 1.0);
        launcher.sync_and_update();
        launcher.poll(
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(1500),
        );
        if let Some(d) = &out {
            launcher.render_to_file(d.join("launcher.png"));
        }
        assert!(!launcher.render().is_empty());
    }
}
