//! Unilux desktop app.

mod agents;
mod app;
mod approvals;
mod build_screen;
mod diagnostics;
mod events;
mod i18n;
mod launcher;
mod loader;
mod recipes;
mod sessions;
mod settings;
mod setup;
mod state;
mod theme;
mod vr;
mod widgets;
mod wizard;

use std::sync::OnceLock;

use freya::prelude::*;
use unilux_service::Service;

static SERVICE: OnceLock<Service> = OnceLock::new();
/// How we were started, so a restart runs the updated file.
static LAUNCH_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Start a fresh Unilux (the updated one) and exit this process.
pub fn restart() {
    let target = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| LAUNCH_PATH.get().cloned())
        .unwrap_or_else(|| "unilux-gui".into());
    match std::process::Command::new(&target).spawn() {
        Ok(_) => std::process::exit(0),
        Err(e) => eprintln!("unilux-gui: could not restart {}: {e}", target.display()),
    }
}

pub fn try_svc() -> Option<&'static Service> {
    SERVICE.get()
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
            eprintln!("unilux-gui: {e}");
            std::process::exit(1);
        }
    };
    let server = unilux_agent::Server::new(service.clone()).with_confirm(approvals::hook());
    let socket = service.paths().socket();
    rt.spawn(async move {
        if let Err(e) = unilux_agent::serve_socket(server, &socket).await {
            eprintln!("unilux-gui: agent socket unavailable: {e}");
        }
    });
    let _ = LAUNCH_PATH.set(std::env::current_exe().unwrap_or_default());
    let warm = service.clone();
    rt.spawn(async move {
        warm.catalog(false).await;
        if warm.config().check_updates
            && let Err(e) = warm.update_check().await
        {
            eprintln!("unilux-gui: update check failed: {e}");
        }
    });
    let _ = SERVICE.set(service);
    launch(
        LaunchConfig::new().with_window(
            WindowConfig::new(app::app)
                .with_title("Unilux")
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
    /// UNILUX_SNAPSHOT_DIR to keep the PNGs.
    #[test]
    fn screens_render() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let _g = rt.enter();
        let home = tempfile::tempdir().unwrap();
        let h = home.path().display().to_string();
        let paths = unilux_core::Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
        let host = unilux_system::FakeHost::new()
            .with_env("HOME", &h)
            .with_command("python3 --version", "Python 3.13.1")
            .with_file(
                &format!("{h}/.config/openxr/1/active_runtime.json"),
                r#"{"runtime":{"name":"WiVRn","library_path":"/var/lib/flatpak/app/io.github.wivrn.wivrn/current/active/files/lib/wivrn/libopenxr_wivrn.so"}}"#,
            );
        let svc = Service::builder(paths.clone())
            .host(Arc::new(host))
            .log_file(false)
            .build()
            .unwrap();
        if let Ok(bg) = std::env::var("UNILUX_SNAPSHOT_BG") {
            std::fs::create_dir_all(&paths.cache_dir).unwrap();
            std::fs::copy(bg, paths.cache_dir.join("background.jpg")).unwrap();
        }
        let _ = SERVICE.set(svc);
        let out = std::env::var("UNILUX_SNAPSHOT_DIR")
            .ok()
            .map(std::path::PathBuf::from);
        let size = Size2D::new(1180., 720.);

        for (i, name) in [
            "welcome",
            "proton",
            "account",
            "source",
            "installing",
            "done",
        ]
        .iter()
        .enumerate()
        {
            wizard::TEST_PAGE.with(|p| p.set(i as u8));
            let (mut wizard, _) = TestingRunner::new(app::app, size, |_| {}, 1.0);
            wizard.sync_and_update();
            wizard.poll(
                std::time::Duration::from_millis(50),
                std::time::Duration::from_millis(1200),
            );
            assert!(!wizard.render().is_empty());
            if let Some(d) = &out {
                wizard.render_to_file(d.join(format!("wizard-{name}.png")));
            }
        }
        wizard::TEST_PAGE.with(|p| p.set(2));
        events::TEST_CHALLENGE.with(|c| {
            *c.borrow_mut() = Some(unilux_service::TwoFactor {
                sms: true,
                authenticator: true,
                phone: Some("+33 *** ** 12".into()),
            })
        });
        let (mut code, _) = TestingRunner::new(app::app, size, |_| {}, 1.0);
        code.sync_and_update();
        code.poll(
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(800),
        );
        assert!(!code.render().is_empty());
        if let Some(d) = &out {
            code.render_to_file(d.join("wizard-account-code.png"));
        }
        events::TEST_CHALLENGE.with(|c| *c.borrow_mut() = None);
        wizard::TEST_PAGE.with(|p| p.set(0));

        let launcher_exe = paths
            .default_prefix()
            .join(unilux_components::sources::LAUNCHER_REL_PATH);
        std::fs::create_dir_all(launcher_exe.parent().unwrap()).unwrap();
        std::fs::write(&launcher_exe, "MZ").unwrap();
        std::fs::create_dir_all(paths.state_file().parent().unwrap()).unwrap();
        std::fs::write(
            paths.state_file(),
            r#"{"schema":2,"proton":"ge:GE-Proton11-5","installed_at":1}"#,
        )
        .unwrap();
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

        // Signed in, with events coming up.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ev = |id: &str, name: &str, start: u64| unilux_service::UpcomingEvent {
            id: id.into(),
            name: name.into(),
            description: Some(
                "Live music, open mic after the show. Doors open thirty minutes before.".into(),
            ),
            location: Some("Amphitheater".into()),
            creator: Some("Shadola".into()),
            attendees: Some(12),
            start,
            end: Some(start + 9000),
            image_url: None,
        };
        let banner = std::env::var("UNILUX_SNAPSHOT_BG")
            .ok()
            .map(std::path::PathBuf::from);
        state::TEST_EVENTS.with(|e| {
            *e.borrow_mut() = Some((
                unilux_service::Account {
                    name: "Zutyo.sh".into(),
                    user_id: None,
                    expires: None,
                },
                vec![
                    (
                        ev("1", "Velvet & Dragon Concert", now + 5 * 3600 + 33 * 60),
                        banner,
                    ),
                    (ev("2", "World Exploring", now + 26 * 3600), None),
                    (ev("3", "Tarot Reading", now + 50 * 3600), None),
                ],
            ))
        });
        let (mut signed, _) = TestingRunner::new(app::app, size, |_| {}, 1.0);
        signed.sync_and_update();
        signed.poll(
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(1500),
        );
        if let Some(d) = &out {
            signed.render_to_file(d.join("launcher-events.png"));
        }
        assert!(!signed.render().is_empty());
        state::TEST_EVENTS.with(|e| *e.borrow_mut() = None);

        // A saved session with its frame curve, for the Sessions tab.
        let sessions = paths.state_file().parent().unwrap().join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let report = r#"{"format":1,"unilux_version":"0.1.1","recipe":{"id":"recommended","key":"750ff3f4fd938d1e","proton":"ge:GE-Proton11-12"},"hardware":{"gpu":"AMD Radeon RX 7900 XT (RADV NAVI31)","gpu_driver":"amdgpu","cpu":"AMD Ryzen 5 5600X 6-Core Processor","ram_gb":31,"distro":"CachyOS","kernel":"7.3.3-1-cachyos","vr_runtime":"WiVRn"},"started_at":1794395606,"duration_s":702,"flat":false,"exit_code":0,"crash":null,"worlds":1,"joins":2,"video":{"opened":2,"failed":0},"perf":{"frames":58017,"avg_fps":85.4,"low1_fps":14.7,"p99_frametime_ms":17.2,"stutters":213,"dips":{"episodes":3,"seconds":42.0,"fps":41.0,"gpu_load":[62.0,31.0],"gpu_clock_mhz":null,"cpu_load":null,"cause":"gpu_waiting"},"hitches":{"count":12,"longest_ms":420.0,"period_s":30.0}},"exceptions":1}"#;
        std::fs::write(sessions.join("1794395606-recommended.json"), report).unwrap();
        let mut csv = String::from("fps,frametime,cpu_load,gpu_load\n");
        for i in 0..6000 {
            let slow = (2500..2800).contains(&i) || (4800..4850).contains(&i);
            let ft = if slow {
                24.0
            } else if i % 1500 == 700 {
                220.0
            } else {
                11.5
            };
            let gpu = if slow { 30 } else { 62 };
            csv.push_str(&format!("{:.1},{ft},35,{gpu}\n", 1000.0 / ft));
        }
        let tl = unilux_patches::timeline::from_mangohud(&csv).unwrap();
        std::fs::write(
            sessions.join("1794395606-recommended.timeline"),
            serde_json::to_string(&tl).unwrap(),
        )
        .unwrap();

        // A few agent calls, for the Agents page.
        std::fs::create_dir_all(paths.logs_dir()).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let calls = [
            (now - 5400, "status", "{}", true),
            (now - 5300, "run_doctor", "{}", true),
            (now - 900, "recipe_check", r#"{"toml":"x"}"#, false),
            (now - 600, "recipe_save", r#"{"id":"my-fix"}"#, true),
            (now - 120, "recipe_use", r#"{"id":"my-fix"}"#, false),
        ];
        let audit: String = calls
            .iter()
            .map(|(at, tool, args, ok)| {
                format!("{{\"args\":{args},\"at\":{at},\"ok\":{ok},\"tool\":\"{tool}\"}}\n")
            })
            .collect();
        std::fs::write(paths.audit_log(), audit).unwrap();

        // Compatibility recipes: every tab, then the compile screen.
        state::TEST_SECTION.with(|s| s.set(Some(state::Section::Recipes)));
        for (i, name) in [
            "recipes",
            "mine",
            "sessions",
            "community",
            "setup-proton",
            "setup-fixes",
            "setup-build",
            "build",
        ]
        .iter()
        .enumerate()
        {
            recipes::TEST_TAB.with(|t| t.set(i as u8 % 4));
            setup::TEST_STEP.with(|s| s.set((4..7).contains(&i).then(|| (i - 4) as u8)));
            state::TEST_BUILD.with(|b| b.set(*name == "build"));
            let (mut r, _) = TestingRunner::new(app::app, size, |_| {}, 1.0);
            r.sync_and_update();
            r.poll(
                std::time::Duration::from_millis(50),
                std::time::Duration::from_millis(800),
            );
            assert!(!r.render().is_empty());
            if let Some(d) = &out {
                r.render_to_file(d.join(format!("recipes-{name}.png")));
            }
        }
        setup::TEST_STEP.with(|s| s.set(None));
        state::TEST_BUILD.with(|b| b.set(false));
        recipes::TEST_TAB.with(|t| t.set(2));
        sessions::TEST_OPEN.with(|o| o.set(true));
        let (mut r, _) = TestingRunner::new(app::app, size, |_| {}, 1.0);
        r.sync_and_update();
        r.poll(
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(800),
        );
        assert!(!r.render().is_empty());
        if let Some(d) = &out {
            r.render_to_file(d.join("recipes-session-detail.png"));
        }
        sessions::TEST_OPEN.with(|o| o.set(false));

        // VR and Diagnostics.
        for (section, name, wait) in [
            (state::Section::Vr, "vr", 800),
            (state::Section::Diagnostics, "diagnostics", 4000),
            (state::Section::Agents, "agents", 800),
        ] {
            state::TEST_SECTION.with(|s| s.set(Some(section)));
            let (mut r, _) = TestingRunner::new(app::app, size, |_| {}, 1.0);
            r.sync_and_update();
            r.poll(
                std::time::Duration::from_millis(50),
                std::time::Duration::from_millis(wait),
            );
            assert!(!r.render().is_empty());
            if let Some(d) = &out {
                r.render_to_file(d.join(format!("{name}.png")));
            }
        }
        state::TEST_SECTION.with(|s| s.set(None));
        state::TEST_BUILD.with(|b| b.set(false));
    }
}
