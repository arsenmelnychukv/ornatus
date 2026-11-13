//! ornatus application: egui UI, desktop dev harness and wasm entry point.

mod app;
mod theme;
mod viewport;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use app::OrnatusApp;
