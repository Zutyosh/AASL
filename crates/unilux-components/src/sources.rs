//! Pinned upstream locations. umu-run's version and digest match
//! Somnilux's install.sh so both tools run the same launcher.

pub const UMU_VERSION: &str = "1.4.4";
pub const UMU_SHA256: &str = "eb590691841f7fad3fc3ad8fd5db4ccb87849fe7948e62b28ece7a4ee48cc851";

/// Path of Somnium's launcher inside a prefix.
pub const LAUNCHER_REL_PATH: &str =
    "drive_c/Program Files/Somnium Space/Somnium Space Launcher.exe";
pub const GAME_ID: &str = "umu-somnium";
/// Somnium Space's official Windows installer (Inno Setup), as linked from
/// somniumspace.com.
pub const SOMNIUM_INSTALLER_URL: &str =
    "https://ai-space.fra1.cdn.digitaloceanspaces.com/launcher/installer/Somnium%20Space.exe";
/// Official key art from somniumspace.com, fetched at run time for the
/// launcher background (never shipped in Unilux).
pub const BACKGROUND_URL: &str =
    "https://ai-space.fra1.cdn.digitaloceanspaces.com/website/somniumPoster.jpeg";
/// Somnium Space's web API (players online, events, sign-in).
pub const SOMNIUM_API: &str = "https://identity.somniumspace.org/v3/identity/api";
/// Unilux's GitHub repository: community recipes and their submission form.
pub const REPO: &str = "Zutyosh/Unilux";
pub const COMMUNITY_RAW: &str = "https://raw.githubusercontent.com/Zutyosh/Unilux/main/community";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sources {
    pub umu_base: String,
    pub umu_version: String,
    pub umu_sha256: String,
    /// GitHub API root, overridable for tests and mirrors.
    pub github_api: String,
    pub somnium_installer_url: String,
    /// Somnium Space's web API, as used by its launcher.
    pub somnium_api: String,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            umu_base: "https://github.com/Open-Wine-Components/umu-launcher/releases/download"
                .into(),
            umu_version: UMU_VERSION.into(),
            umu_sha256: UMU_SHA256.into(),
            github_api: "https://api.github.com".into(),
            somnium_installer_url: SOMNIUM_INSTALLER_URL.into(),
            somnium_api: SOMNIUM_API.into(),
        }
    }
}

impl Sources {
    pub fn umu_url(&self) -> String {
        let v = &self.umu_version;
        format!("{}/{v}/umu-launcher-{v}-zipapp.tar", self.umu_base)
    }
}
