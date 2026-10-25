//! Owned data, read-only overview and pure supported-audio policy.
//!
//! No GTK, Tauri, PipeWire, filesystem I/O or backend dispatch. Native widget
//! integration, recording I/O and settings migration remain later work.

pub mod backend;
pub mod graph;
pub mod inbox;
pub mod overview;

pub mod domain;

pub mod settings;

pub mod history_backend;
pub mod history_identity;
pub mod native_command;
