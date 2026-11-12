use serde::Serialize;

use crate::Host;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageManager {
    Apt,
    Dnf,
    Pacman,
    Zypper,
    RpmOstree,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Distro {
    pub id: String,
    pub id_like: Vec<String>,
    pub name: String,
    pub version_id: Option<String>,
    /// Bazzite, Silverblue, SteamOS and similar: the root filesystem is
    /// read-only and packages go through layering or containers.
    pub immutable: bool,
    pub package_manager: PackageManager,
}

const IMMUTABLE_IDS: &[&str] = &[
    "bazzite",
    "silverblue",
    "kinoite",
    "aurora",
    "bluefin",
    "steamos",
    "nixos",
    "fedora-asahi-remix-atomic",
];

impl Distro {
    pub fn detect(host: &dyn Host) -> Option<Self> {
        let text = host
            .read("/etc/os-release".as_ref())
            .or_else(|| host.read("/usr/lib/os-release".as_ref()))?;
        let mut d = Self::parse(&text);
        if host.exists("/run/ostree-booted".as_ref()) {
            d.immutable = true;
            d.package_manager = PackageManager::RpmOstree;
        }
        Some(d)
    }

    pub fn parse(text: &str) -> Self {
        let get = |key: &str| {
            text.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == key).then(|| v.trim().trim_matches('"').trim_matches('\'').to_string())
            })
        };
        let id = get("ID").unwrap_or_else(|| "linux".into()).to_lowercase();
        let id_like: Vec<String> = get("ID_LIKE")
            .map(|s| s.split_whitespace().map(|w| w.to_lowercase()).collect())
            .unwrap_or_default();
        let variant = get("VARIANT_ID").unwrap_or_default().to_lowercase();
        let immutable = IMMUTABLE_IDS.contains(&id.as_str())
            || IMMUTABLE_IDS.contains(&variant.as_str())
            || variant.contains("atomic");
        let family = |names: &[&str]| {
            names
                .iter()
                .any(|n| id == *n || id_like.iter().any(|l| l == n))
        };
        let package_manager = if immutable && family(&["fedora"]) {
            PackageManager::RpmOstree
        } else if family(&["debian", "ubuntu"]) {
            PackageManager::Apt
        } else if family(&["fedora", "rhel", "centos"]) {
            PackageManager::Dnf
        } else if family(&["arch"]) {
            PackageManager::Pacman
        } else if family(&["suse", "opensuse", "opensuse-tumbleweed", "opensuse-leap"]) {
            PackageManager::Zypper
        } else {
            PackageManager::Unknown
        };
        Self {
            name: get("PRETTY_NAME")
                .or_else(|| get("NAME"))
                .unwrap_or_else(|| id.clone()),
            version_id: get("VERSION_ID"),
            id,
            id_like,
            immutable,
            package_manager,
        }
    }

    /// The command a user would run to install a generic tool, with the
    /// distribution's own package name.
    pub fn install_hint(&self, tool: &str) -> Option<String> {
        let pkg = match (tool, self.package_manager) {
            ("python3", PackageManager::Pacman) => "python",
            ("magick" | "convert", _) => "imagemagick",
            ("vulkaninfo", PackageManager::Apt) => "vulkan-tools",
            ("vulkaninfo", PackageManager::Pacman) => "vulkan-tools",
            ("vulkaninfo", _) => "vulkan-tools",
            (t, _) => t,
        };
        Some(match self.package_manager {
            PackageManager::Apt => format!("sudo apt install {pkg}"),
            PackageManager::Dnf => format!("sudo dnf install {pkg}"),
            PackageManager::Pacman => format!("sudo pacman -S {pkg}"),
            PackageManager::Zypper => format!("sudo zypper install {pkg}"),
            PackageManager::RpmOstree => {
                format!("rpm-ostree install {pkg} (then reboot), or use a toolbox")
            }
            PackageManager::Unknown => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pm(text: &str) -> (PackageManager, bool) {
        let d = Distro::parse(text);
        (d.package_manager, d.immutable)
    }

    #[test]
    fn families() {
        use PackageManager::*;
        let cases = [
            ("ID=ubuntu\nID_LIKE=debian", (Apt, false)),
            ("ID=linuxmint\nID_LIKE=\"ubuntu debian\"", (Apt, false)),
            ("ID=pop\nID_LIKE=\"ubuntu debian\"", (Apt, false)),
            ("ID=debian", (Apt, false)),
            ("ID=fedora\nVARIANT_ID=workstation", (Dnf, false)),
            ("ID=nobara\nID_LIKE=\"rhel centos fedora\"", (Dnf, false)),
            ("ID=fedora\nVARIANT_ID=silverblue", (RpmOstree, true)),
            ("ID=bazzite\nID_LIKE=\"fedora\"", (RpmOstree, true)),
            ("ID=arch", (Pacman, false)),
            ("ID=cachyos\nID_LIKE=arch", (Pacman, false)),
            ("ID=manjaro\nID_LIKE=arch", (Pacman, false)),
            ("ID=steamos\nID_LIKE=arch", (Pacman, true)),
            (
                "ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"",
                (Zypper, false),
            ),
            ("ID=nixos", (Unknown, true)),
        ];
        for (text, expected) in cases {
            assert_eq!(pm(text), expected, "{text}");
        }
    }

    #[test]
    fn hints_use_distro_names() {
        let arch = Distro::parse("ID=arch");
        assert_eq!(
            arch.install_hint("python3").unwrap(),
            "sudo pacman -S python"
        );
        let mint = Distro::parse("ID=linuxmint\nID_LIKE=ubuntu");
        assert_eq!(
            mint.install_hint("python3").unwrap(),
            "sudo apt install python3"
        );
        assert!(Distro::parse("ID=nixos").install_hint("python3").is_none());
    }

    #[test]
    fn pretty_name() {
        let d = Distro::parse(
            "NAME=\"Linux Mint\"\nPRETTY_NAME=\"Linux Mint 22\"\nID=linuxmint\nVERSION_ID=\"22\"",
        );
        assert_eq!(d.name, "Linux Mint 22");
        assert_eq!(d.version_id.as_deref(), Some("22"));
    }
}
