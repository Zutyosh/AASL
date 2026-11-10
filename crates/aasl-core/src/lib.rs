//! Shared types for every AASL crate: paths, configuration, events and errors.

mod config;
mod error;
mod event;
pub mod exec;
mod paths;

pub use config::{CONFIG_VERSION, Config, DEFAULT_CATALOG_URL, write_atomic};
pub use error::{Error, IoContext, Result};
pub use event::{Event, LogSource, Severity};
pub use paths::Paths;

/// The event bus shared by every operation.
pub type Events = tokio::sync::broadcast::Sender<Event>;

/// Create an event bus with room for a burst of log lines.
pub fn event_bus() -> Events {
    tokio::sync::broadcast::channel(4096).0
}
