//! Install, repair and uninstall Somnium Space in a private Proton prefix.

mod desktop;
pub mod pipeline;
pub mod state;

pub use pipeline::{Installer, Mode, Options, Step};
pub use state::InstallState;

use std::path::{Path, PathBuf};

use aasl_components::sources::GAME_ID;
use aasl_components::{ProtonId, ProtonManager};
use aasl_core::exec::CommandSpec;

/// Proton builds live under components/proton/<provider>/<tag>, outside
/// anything Steam scans.
pub fn proton_dir(paths: &aasl_core::Paths, id: &ProtonId) -> PathBuf {
    paths.proton_root().join(&id.provider).join(&id.tag)
}

pub fn proton_manager(
    paths: &aasl_core::Paths,
    config: &aasl_core::Config,
    api_base: &str,
) -> ProtonManager {
    let mut m = ProtonManager::new(
        paths.proton_root(),
        paths.cache_dir.clone(),
        paths.downloads_dir(),
    );
    m.api_base = api_base.to_string();
    if let Some(t) = config.github_token.clone().filter(|t| !t.is_empty()) {
        m.token = Some(t);
    }
    m
}

pub fn umu_run(paths: &aasl_core::Paths) -> PathBuf {
    paths.umu_dir().join("umu-run")
}

/// Base command to run something inside the prefix with umu.
pub fn umu_command(umu: &Path, proton: &Path, prefix: &Path) -> CommandSpec {
    CommandSpec::new(umu)
        .env("PROTONPATH", proton.display().to_string())
        .env("WINEPREFIX", prefix.display().to_string())
        .env("GAMEID", GAME_ID)
}
