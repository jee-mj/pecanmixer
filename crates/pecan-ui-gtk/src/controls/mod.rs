//! Functional volume/mute state behind an isolated adapter boundary.
//!
//! Snapshot and request types belong to the supplied adapter. This module never
//! constructs backend-origin tokens or treats admission as completion. The
//! application keeps its read-only default until a production adapter is wired
//! and covered by synthetic integration tests.
mod gesture;
mod keyboard;
mod model;
mod native;
/// Pure partial-strip gain memory; no dispatch or history adoption.
pub mod partial_mute;
mod view;
pub use model::{Adapter, Binding, ControlError, Controller, Phase, ReadOnly};
pub use view::MixerControls;

#[cfg(test)]
mod tests;
