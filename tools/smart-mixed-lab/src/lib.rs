//! Pure Recognition Lab: no filesystem, database, Tauri, network or production scanner dependency.
pub mod model;
mod recognize;
pub mod signals;
pub use recognize::{recognize, recognize_indexed};
