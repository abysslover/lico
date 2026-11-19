//! The adapter `browser/` (M17-P06): a browser tab with no window yet. The host starts the engine (Moli, pinned release, hash-checked) only when a tab asks, behind `lico-sandbox`
//! and the fetch guard `lico-proxy`, drives it with `lico-cdp`, keeps one `lico-browser-view` model per tab, and ends the engine with the last tab. It plugs in through the driver
//! port (`driver::register_ops`) and is registered once, from the root; no hub names it.

mod engine;
mod fonts;
mod installer;
mod map;
mod ops;
mod pages_ops;
mod profile;
mod reserved;
mod search;
mod sha256;
mod supervisor;
mod watch;
mod window_ops;
#[cfg(test)]
mod ops_install_tests;
#[cfg(test)]
mod pages_ops_tests;
#[cfg(test)]
mod real_probe;
#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod system_tests;

/// Registers the browser's operations with the host (called once, from the root).
pub fn register() {
    crate::driver::register_ops(ops::Browser::new());
}
