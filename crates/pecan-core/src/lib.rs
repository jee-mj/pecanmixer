//! Shared, owned data and the first read-only native controller.
//!
//! This crate deliberately has no GTK, Tauri or PipeWire dependency. Routing,
//! recording policy and settings migration are later slices.

pub mod backend;
pub mod graph;
pub mod inbox;
pub mod overview;
