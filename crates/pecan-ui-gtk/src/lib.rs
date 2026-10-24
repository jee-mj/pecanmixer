//! Main-thread GTK presentation for the read-only migration preview.
//!
//! This crate consumes the core overview as-is: node ordering, labels and port
//! ordering belong to the pure projection. No routing or audio mutations live here.
mod widgets;

mod presentation;
pub use presentation::GraphPresentation;
pub use widgets::{MixerOverview, PatchbayOverview};

/// Synchronous explicit-path I/O; not wired to the preview or startup.
pub mod settings_storage;

/// Latest-frame meter widgets; not connected to preview startup.
pub mod meters;
