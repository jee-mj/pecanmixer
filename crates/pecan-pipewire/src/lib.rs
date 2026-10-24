//! Shared backend; no GUI toolkit or webview dependency.
//! Legacy modules retain their API during the migration.
pub mod bus_config;
mod commands;
pub mod graaf;
pub mod meter;
mod meter_captures;
pub mod midimeter;
pub mod opname;
pub mod shutdown;
mod writer_queue;
mod writer_status;
#[cfg(test)]
mod test_support;
