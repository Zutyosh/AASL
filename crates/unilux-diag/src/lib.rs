//! Diagnostics: checks that explain what is wrong and name the fix.

pub mod checks;
pub mod report;

pub use checks::{Check, DiagContext, Finding, FixId, run_all};
