//! Somnium needs Wine fixes that no Proton ships yet. This crate owns them
//! end to end: the catalog of published bundles, installing a bundle into
//! a Proton build and checking it is still intact, and building a new
//! bundle for any Proton from the patch sources in `patches/`.

pub mod builder;
pub mod bundle;
pub mod catalog;
pub mod series;

pub use bundle::{ARCHIVE_FORMAT, PatchStatus};
pub use catalog::{Bundle, Catalog, Match, TargetStatus};
