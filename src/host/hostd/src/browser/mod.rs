//! The adapter `browser/` (M17-P06): a browser tab with no window yet. The host starts the engine (Obscura, pinned release, hash-checked) only when a tab asks, behind `lico-sandbox`
//! and the fetch guard `lico-proxy`, drives it with `lico-cdp`, keeps one `lico-browser-view` model per tab, and ends the engine with the last tab. It plugs in through the driver
//! port (`driver::register_ops`) and is registered once, from the root; no hub names it.

mod engine;
mod fonts;
mod map;
mod ops;
mod profile;
mod sha256;
mod supervisor;
#[cfg(test)]
mod system_tests;

/// Registers the browser's operations with the host (called once, from the root).
pub fn register() {
    crate::driver::register_ops(std::sync::Arc::new(ops::Browser::new()));
}
