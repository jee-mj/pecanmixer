//! Synthetic-testable meter presentation; no backend ownership or audio effects.
//! See README.md in this directory for the owning shell's adapter contract.
mod frames;
mod widget;

pub use frames::MeterFeed;
pub use widget::{MeterMode, MeterReading, MeterWidget};
