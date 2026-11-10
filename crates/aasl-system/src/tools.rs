use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::Host;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PythonVersion {
    pub major: u32,
    pub minor: u32,
}

impl PythonVersion {
    /// umu-run needs 3.10 or newer.
    pub fn supported(&self) -> bool {
        (self.major, self.minor) >= (3, 10)
    }
}

impl std::fmt::Display for PythonVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

pub fn python_version(host: &dyn Host) -> Option<PythonVersion> {
    let out = host.run("python3", &["--version"])?;
    let v = out
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
    let mut it = v.split('.');
    Some(PythonVersion {
        major: it.next()?.parse().ok()?,
        minor: it.next()?.parse().ok()?,
    })
}

pub fn find_in_path(host: &dyn Host, name: &str) -> Option<PathBuf> {
    let path = host.env("PATH")?;
    path.split(':')
        .filter(|d| !d.is_empty())
        .map(|d| Path::new(d).join(name))
        .find(|p| host.exists(p))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GpuInfo {
    pub card: String,
    pub vendor: String,
    pub driver: Option<String>,
}

fn vendor_name(id: &str) -> String {
    match id.trim() {
        "0x10de" => "NVIDIA".into(),
        "0x1002" => "AMD".into(),
        "0x8086" => "Intel".into(),
        other => other.to_string(),
    }
}

pub fn gpus(host: &dyn Host) -> Vec<GpuInfo> {
    host.list_dir("/sys/class/drm".as_ref())
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("card") && !n.contains('-'))
        })
        .filter_map(|card| {
            let vendor = host.read(&card.join("device/vendor"))?;
            let driver = host.read(&card.join("device/uevent")).and_then(|u| {
                u.lines()
                    .find_map(|l| l.strip_prefix("DRIVER=").map(str::to_string))
            });
            Some(GpuInfo {
                card: card.file_name()?.to_string_lossy().into_owned(),
                vendor: vendor_name(&vendor),
                driver,
            })
        })
        .collect()
}

/// Installed Vulkan ICD manifests. Empty means no Vulkan driver.
pub fn vulkan_icds(host: &dyn Host) -> Vec<String> {
    [
        "/usr/share/vulkan/icd.d",
        "/etc/vulkan/icd.d",
        "/usr/local/share/vulkan/icd.d",
    ]
    .iter()
    .flat_map(|d| host.list_dir(d.as_ref()))
    .filter(|p| p.extension().is_some_and(|e| e == "json"))
    .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeHost;

    #[test]
    fn python_parsing() {
        let h = FakeHost::new().with_command("python3 --version", "Python 3.9.18\n");
        let v = python_version(&h).unwrap();
        assert_eq!(v, PythonVersion { major: 3, minor: 9 });
        assert!(!v.supported());
        let h = FakeHost::new().with_command("python3 --version", "Python 3.13.1");
        assert!(python_version(&h).unwrap().supported());
        assert!(python_version(&FakeHost::new()).is_none());
    }

    #[test]
    fn path_lookup() {
        let h = FakeHost::new()
            .with_env("PATH", "/bin:/usr/bin")
            .with_file("/usr/bin/python3", "");
        assert_eq!(find_in_path(&h, "python3"), Some("/usr/bin/python3".into()));
        assert_eq!(find_in_path(&h, "magick"), None);
    }

    #[test]
    fn gpu_and_vulkan() {
        let h = FakeHost::new()
            .with_file("/sys/class/drm/card1/device/vendor", "0x1002\n")
            .with_file(
                "/sys/class/drm/card1/device/uevent",
                "DRIVER=amdgpu\nPCI_ID=1002:744C",
            )
            .with_file("/sys/class/drm/card1-DP-1/status", "connected")
            .with_file("/usr/share/vulkan/icd.d/radeon_icd.x86_64.json", "{}");
        let g = gpus(&h);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].vendor, "AMD");
        assert_eq!(g[0].driver.as_deref(), Some("amdgpu"));
        assert_eq!(vulkan_icds(&h), vec!["radeon_icd.x86_64.json"]);
    }
}
