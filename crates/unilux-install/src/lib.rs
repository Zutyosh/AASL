//! Install, repair and uninstall Somnium Space in a private Proton prefix.

mod desktop;
pub mod pipeline;
pub mod state;

pub use pipeline::{Installer, Mode, Options, Step};
pub use state::InstallState;

use std::path::{Path, PathBuf};

use unilux_components::sources::GAME_ID;
use unilux_components::{ProtonId, ProtonManager};
use unilux_core::exec::CommandSpec;

/// Proton builds live under components/proton/<provider>/<tag>, outside
/// anything Steam scans.
pub fn proton_dir(paths: &unilux_core::Paths, id: &ProtonId) -> PathBuf {
    paths.proton_root().join(&id.provider).join(&id.tag)
}

pub fn proton_manager(
    paths: &unilux_core::Paths,
    config: &unilux_core::Config,
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

/// The user's language from the usual locale variables.
pub fn system_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
}

/// Inno Setup language name for a locale (`fr_FR.UTF-8` is `french`).
/// Somnium's installer ships English, German, Spanish, French, Russian
/// and Japanese; anything else gets English.
pub fn inno_language(locale: &str) -> &'static str {
    match locale.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("fr") => "french",
        Some("de") => "german",
        Some("es") => "spanish",
        Some("ru") => "russian",
        Some("ja") => "japanese",
        _ => "english",
    }
}

pub fn umu_run(paths: &unilux_core::Paths) -> PathBuf {
    paths.umu_dir().join("umu-run")
}

/// Base command to run something inside the prefix with umu.
pub fn umu_command(umu: &Path, proton: &Path, prefix: &Path) -> CommandSpec {
    CommandSpec::new(umu)
        .env("PROTONPATH", proton.display().to_string())
        .env("WINEPREFIX", prefix.display().to_string())
        .env("GAMEID", GAME_ID)
}

#[cfg(test)]
mod tests {
    #[test]
    fn inno_languages() {
        assert_eq!(super::inno_language("fr_FR.UTF-8"), "french");
        assert_eq!(super::inno_language("ja_JP"), "japanese");
        assert_eq!(super::inno_language("pt_BR.UTF-8"), "english");
        assert_eq!(super::inno_language(""), "english");
    }
}
