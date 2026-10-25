//! Main-thread GTK presentation and isolated mixer control scaffolding.
//!
//! The explicit native entrypoint uses the single owner/shell. GraphPresentation
//! remains observational; functional controls use that owner's exact graph and
//! shared receipt/history boundary. A separate owner adapter gates bus edits and
//! recording behind exact local readback and an explicit recording directory.
mod widgets;

mod presentation;
pub use presentation::GraphPresentation;
pub use widgets::{MixerOverview, PatchbayOverview};

/// Synchronous explicit-path I/O, called only by the off-UI settings worker.
pub mod settings_storage;

pub mod controls;

pub mod native_backend;

/// Presentation-only preferences; no audio configuration effects.
pub mod display_settings;

/// Serialized I/O worker, started only for an explicit native settings path.
pub mod settings_worker;

/// Latest-frame meter widgets and the accepted-batch presentation adapter.
pub mod meters;

/// One application owner/coordinator shared by native startup and fixture tests.
pub mod application;
/// Native shell policy and finalization effects.
pub mod lifetime;
/// GTK-free tray adapter, started only by explicit native opt-in.
pub mod tray;

/// Pure recording-bus presentation/intents; no runtime adoption.
pub mod recording_buses;

/// Explicit owner/shell executable adoption; no implicit settings discovery.
pub mod startup;

/// Isolated routing selection/reset intents and widgets; no startup adoption.
pub mod routing;

/// Pure native desktop commands and explicit-path I/O; no runtime adoption.
pub mod desktop_launch;
