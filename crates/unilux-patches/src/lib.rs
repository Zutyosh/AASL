//! Somnium needs Wine fixes that no Proton ships yet. This crate owns them
//! end to end: compatibility recipes and their ingredient library, the
//! catalog of published bundles, building a bundle for any Proton, and
//! installing it and checking it is still intact.

pub mod builder;
pub mod bundle;
pub mod catalog;
pub mod community;
pub mod recipe;
pub mod timeline;

pub use bundle::{ARCHIVE_FORMAT, PatchStatus};
pub use catalog::{Bundle, Catalog, Match, TargetStatus};
pub use recipe::{Library, Plan, Recipe};

use serde::Serialize;

/// Where a Proton's patches come from. One rule, shared by the installer,
/// the service and the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Source {
    /// The recipe has nothing to compile.
    NotNeeded,
    /// A bundle from the catalog (only for the default recipe, which is
    /// what published bundles are built from).
    Published(TargetStatus),
    /// Built here for this exact recipe and Proton.
    Built,
    /// Nothing fits yet: the recipe has to be built (podman or docker).
    MustBuild,
}

/// Verified published bundles win (tested by people), then a bundle built
/// here for this exact recipe and Proton, then an untested published one.
pub fn choose(
    needs_build: bool,
    default_recipe: bool,
    published: Option<TargetStatus>,
    built: bool,
) -> Source {
    let published = published.filter(|_| default_recipe);
    match (needs_build, published, built) {
        (false, _, _) => Source::NotNeeded,
        (true, Some(TargetStatus::Verified), _) => Source::Published(TargetStatus::Verified),
        (true, _, true) => Source::Built,
        (true, Some(s), false) => Source::Published(s),
        (true, None, false) => Source::MustBuild,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TargetStatus::*;

    #[test]
    fn choice_order() {
        assert_eq!(choose(false, true, Some(Verified), true), Source::NotNeeded);
        assert_eq!(
            choose(true, true, Some(Verified), true),
            Source::Published(Verified)
        );
        assert_eq!(choose(true, true, Some(Untested), true), Source::Built);
        assert_eq!(
            choose(true, true, Some(Untested), false),
            Source::Published(Untested)
        );
        assert_eq!(
            choose(true, false, Some(Verified), false),
            Source::MustBuild
        );
        assert_eq!(choose(true, false, Some(Verified), true), Source::Built);
    }
}
