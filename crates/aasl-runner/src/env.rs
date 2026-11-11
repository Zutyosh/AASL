//! The environment handed to umu-run for a launch.

use std::collections::BTreeMap;
use std::path::Path;

use aasl_components::sources::GAME_ID;
use aasl_core::Config;
use aasl_system::{Host, OpenXrRuntime};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaunchEnv {
    pub env: BTreeMap<String, String>,
    pub runtime: Option<OpenXrRuntime>,
    /// Human description of where the OpenXR settings came from.
    pub runtime_label: String,
}

/// Build the launch environment. Order: base umu variables, OpenXR (from
/// detection, or replaced wholesale by `config.openxr_env`), then the
/// user's own `config.env` (the service puts the recipe's environment
/// in front of it), which wins over everything.
pub fn build_env(host: &dyn Host, config: &Config, proton: &Path, prefix: &Path) -> LaunchEnv {
    let mut env = BTreeMap::new();
    env.insert("PROTONPATH".into(), proton.display().to_string());
    env.insert("WINEPREFIX".into(), prefix.display().to_string());
    env.insert("GAMEID".into(), GAME_ID.into());

    let (runtime, runtime_label) = if let Some(over) = &config.openxr_env {
        env.extend(over.clone());
        (None, "set in config (detection off)".to_string())
    } else {
        env.insert(
            "PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES".into(),
            "1".into(),
        );
        match aasl_system::active_runtime(host) {
            Some(rt) => {
                // The runtime library and the OpenVR layer must both be
                // visible inside pressure-vessel's container.
                let home = host.home();
                let mut shares: Vec<String> = Vec::new();
                let openvr = aasl_system::openvr_runtime(host).map(|o| o.path);
                for p in [Some(rt.library.clone()), openvr].into_iter().flatten() {
                    if let Some(s) = aasl_system::share_root_for(&p, &home) {
                        let s = s.display().to_string();
                        if !shares.contains(&s) {
                            shares.push(s);
                        }
                    }
                }
                if !shares.is_empty() {
                    env.insert("PRESSURE_VESSEL_FILESYSTEMS_RW".into(), shares.join(":"));
                }
                let label = rt.name.clone().unwrap_or_else(|| rt.kind.to_string());
                (Some(rt), label)
            }
            None => (None, "none detected".to_string()),
        }
    };
    env.extend(config.env.clone());
    LaunchEnv {
        env,
        runtime,
        runtime_label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aasl_system::FakeHost;

    const WIVRN: &str = r#"{"runtime":{"name":"WiVRn","library_path":"/var/lib/flatpak/app/io.github.wivrn.wivrn/x/lib/libopenxr_wivrn.so"}}"#;

    #[test]
    fn detected_flatpak_runtime_is_shared() {
        let h = FakeHost::new().with_file("/home/u/.config/openxr/1/active_runtime.json", WIVRN);
        let e = build_env(&h, &Config::default(), "/p".as_ref(), "/pfx".as_ref());
        assert_eq!(e.env["PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES"], "1");
        assert_eq!(
            e.env["PRESSURE_VESSEL_FILESYSTEMS_RW"],
            "/var/lib/flatpak/app/io.github.wivrn.wivrn"
        );
        assert_eq!(e.env["WINEPREFIX"], "/pfx");
        assert_eq!(e.runtime_label, "WiVRn");
    }

    #[test]
    fn override_replaces_detection_and_user_env_wins() {
        let h = FakeHost::new().with_file("/home/u/.config/openxr/1/active_runtime.json", WIVRN);
        let mut cfg = Config {
            openxr_env: Some([("XR_RUNTIME_JSON".to_string(), "/x.json".to_string())].into()),
            ..Default::default()
        };
        cfg.env.insert("GAMEID".into(), "custom".into());
        let e = build_env(&h, &cfg, "/p".as_ref(), "/pfx".as_ref());
        assert!(
            !e.env
                .contains_key("PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES")
        );
        assert_eq!(e.env["XR_RUNTIME_JSON"], "/x.json");
        assert_eq!(e.env["GAMEID"], "custom");
        assert!(e.runtime.is_none());
    }

    #[test]
    fn nothing_detected() {
        let e = build_env(
            &FakeHost::new(),
            &Config::default(),
            "/p".as_ref(),
            "/pfx".as_ref(),
        );
        assert_eq!(e.runtime_label, "none detected");
        assert!(!e.env.contains_key("PRESSURE_VESSEL_FILESYSTEMS_RW"));
    }
}
