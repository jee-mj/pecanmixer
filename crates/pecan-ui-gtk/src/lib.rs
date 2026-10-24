//! Main-thread GTK presentation and isolated mixer control scaffolding.
//!
//! The application still defaults to observation. The controls module projects
//! the pure mixer domain and forwards intents through an adapter-owned boundary;
//! production ownership and receipt/history wiring remain separate.
mod widgets;

mod presentation;
pub use presentation::GraphPresentation;
pub use widgets::{MixerOverview, PatchbayOverview};

/// Synchronous explicit-path I/O; not wired to the preview or startup.
pub mod settings_storage;

pub mod controls;

pub mod native_backend;

/// Explicit presentation-only preferences; not called by startup.
pub mod display_settings;

/// Explicit serialized I/O worker, not called by startup.
pub mod settings_worker;

/// Latest-frame meter widgets; not connected to preview startup.
pub mod meters;

/// Explicit application owner/coordinator; not adopted by main startup.
pub mod application;
