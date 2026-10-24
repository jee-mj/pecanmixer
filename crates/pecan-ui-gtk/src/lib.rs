//! Main-thread GTK presentation for the read-only migration preview.
//!
//! This crate consumes the core overview as-is: node ordering, labels and port
//! ordering belong to the pure projection. No routing or audio mutations live here.
mod widgets;

pub use widgets::MixerOverview;
