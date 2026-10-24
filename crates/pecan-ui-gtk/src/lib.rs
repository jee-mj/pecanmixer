//! Main-thread GTK presentation and isolated mixer control scaffolding.
//!
//! The application still defaults to observation. The controls module projects
//! the pure mixer domain and forwards intents through an adapter-owned boundary;
//! production dispatch awaits the reviewed backend intent/completion contract.
mod widgets;

mod presentation;
pub use presentation::GraphPresentation;
pub use widgets::{MixerOverview, PatchbayOverview};

/// Synchronous explicit-path I/O; not wired to the preview or startup.
pub mod settings_storage;

pub mod controls;

pub mod native_backend;
