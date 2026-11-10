use std::path::PathBuf;

/// Every location AASL reads or writes. Nothing outside these directories is
/// ever written, apart from the desktop entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub home: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl Paths {
    /// Resolve from the process environment.
    pub fn from_env() -> Option<Self> {
        Self::resolve(|k| std::env::var(k).ok())
    }

    /// Resolve with an injectable environment lookup (used by tests).
    pub fn resolve(env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let home = PathBuf::from(env("HOME").filter(|h| !h.is_empty())?);
        let xdg = |key: &str, fallback: &str| {
            env(key)
                .filter(|v| v.starts_with('/'))
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(fallback))
                .join("aasl")
        };
        let runtime_dir = env("XDG_RUNTIME_DIR")
            .filter(|v| v.starts_with('/'))
            .map(|v| PathBuf::from(v).join("aasl"))
            .unwrap_or_else(|| {
                std::env::temp_dir().join(format!("aasl-{}", env("USER").unwrap_or_default()))
            });
        Some(Self {
            config_dir: xdg("XDG_CONFIG_HOME", ".config"),
            data_dir: xdg("XDG_DATA_HOME", ".local/share"),
            state_dir: xdg("XDG_STATE_HOME", ".local/state"),
            cache_dir: xdg("XDG_CACHE_HOME", ".cache"),
            runtime_dir,
            home,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }
    pub fn state_file(&self) -> PathBuf {
        self.state_dir.join("install-state.json")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.state_dir.join("logs")
    }
    pub fn launch_log(&self) -> PathBuf {
        self.logs_dir().join("launch.log")
    }
    pub fn audit_log(&self) -> PathBuf {
        self.logs_dir().join("agent-audit.jsonl")
    }
    pub fn components_dir(&self) -> PathBuf {
        self.data_dir.join("components")
    }
    pub fn umu_dir(&self) -> PathBuf {
        self.components_dir().join("umu")
    }
    pub fn proton_root(&self) -> PathBuf {
        self.components_dir().join("proton")
    }
    pub fn default_prefix(&self) -> PathBuf {
        self.data_dir.join("prefix")
    }
    pub fn downloads_dir(&self) -> PathBuf {
        self.cache_dir.join("downloads")
    }
    pub fn socket(&self) -> PathBuf {
        self.runtime_dir.join("aasl.sock")
    }
    pub fn desktop_entry(&self) -> PathBuf {
        self.home
            .join(".local/share/applications/aasl-somnium-space.desktop")
    }
    /// The user OpenXR active runtime manifest.
    pub fn openxr_active_runtime(&self, config_home: Option<PathBuf>) -> PathBuf {
        config_home
            .unwrap_or_else(|| self.home.join(".config"))
            .join("openxr/1/active_runtime.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(map: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn defaults_follow_home() {
        let p = Paths::resolve(env(&[
            ("HOME", "/home/u"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ]))
        .unwrap();
        assert_eq!(p.data_dir, PathBuf::from("/home/u/.local/share/aasl"));
        assert_eq!(
            p.config_file(),
            PathBuf::from("/home/u/.config/aasl/config.toml")
        );
        assert_eq!(p.socket(), PathBuf::from("/run/user/1000/aasl/aasl.sock"));
    }

    #[test]
    fn xdg_overrides_and_relative_values_are_ignored() {
        let p = Paths::resolve(env(&[
            ("HOME", "/home/u"),
            ("XDG_DATA_HOME", "/data"),
            ("XDG_CONFIG_HOME", "relative/path"),
        ]))
        .unwrap();
        assert_eq!(p.data_dir, PathBuf::from("/data/aasl"));
        assert_eq!(p.config_dir, PathBuf::from("/home/u/.config/aasl"));
    }

    #[test]
    fn no_home_is_none() {
        assert!(Paths::resolve(env(&[])).is_none());
    }
}
