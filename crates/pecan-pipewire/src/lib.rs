//! Shared backend; no GUI toolkit or webview dependency.
//! Legacy modules retain their API during the migration.
mod commands;
pub mod graaf;
pub mod meter;
pub mod midimeter;
pub mod opname;
pub mod shutdown;
#[cfg(test)]
mod test_support;
