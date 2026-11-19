//! The adapter `terminal/` (M23-P02): a terminal with no window yet. The host opens a shell on a pty, keeps the emulator (`lico-vt`) that reads what the shell writes, and answers
//! seven socket-only operations (`terminal.open`, `.list`, `.close`, `.input`, `.resize`, `.watch`, `.text`). It plugs in through the driver port (`driver::register_ops`) and is
//! registered once, from the root; no hub names it. The only file that calls the system is `pty.rs`.

mod agent;
mod encode;
mod ops;
mod pty;
mod tab;
#[cfg(test)]
mod tests;

/// Whether the host accepts `request` as the operation `op` (the command line's tests use it to check what they build).
#[cfg(test)]
pub fn ops_accept(op: &str, request: &serde_json::Map<String, serde_json::Value>) -> bool {
    ops::valid(op, request)
}

/// Registers the terminal's operations with the host (called once, from the root).
pub fn register() {
    crate::driver::register_ops(ops::Terminals::new());
}
