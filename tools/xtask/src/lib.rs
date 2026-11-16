//! The tools of the component rules (M15-P02), std and `serde_json` only: the module graph of a crate read from its source text, the manifest and graph check of the
//! components, receipts (a hash over a component's inputs), the isolated test run with time and tree memory against a budget, and the release acceptance (two clean
//! clones build the same bytes).

pub mod accept;
pub mod components;
pub mod graph;
pub mod runner;
pub mod sha256;
pub mod system;
pub mod toml;
