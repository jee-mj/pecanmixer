//! Pure policy migrated from the supported audio frontend. No toolkit, I/O,
//! timers or backend dispatch: callers apply the returned command plans.
//! MIDI UI, persisted settings and patch geometry are outside this batch.
pub mod audio;
pub mod buses;
pub mod history;
pub mod levels;
pub mod mixer;
