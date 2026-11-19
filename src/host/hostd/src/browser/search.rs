//! Words typed in the window's bar (M22-P05b): the address `browser.go` and `browser.open` load for what the bar held. The component decides what the text is
//! (`lico_browser_view::classify`) and builds the search address (`search_url`); this is where the owner's setting `browser_search` is read and the strict ways are kept: a request
//! that does not say `search` (the command line) and a tab for this machine's own services parse the text as an address, as they always did.

use super::engine;
use super::ops::Browser;
use super::window_ops::address_code;
use crate::driver::Opaque;
use lico_browser_view::{check_template, classify, parse_address, search_url, SearchError, Typed, DEFAULT_SEARCH};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The key of the owner's setting file: an address with one `%s` where the words go.
const KEY: &str = "browser_search";

/// The search template: the owner's setting, else the default. A setting that is there and is not a valid template is `search_setting`, never skipped for the default (the owner asked
/// for something else); a missing, damaged or group-readable file says nothing.
pub(super) fn template(state_dir: &Path) -> Result<String, &'static str> {
    match engine::setting_value(state_dir, KEY) {
        None => Ok(DEFAULT_SEARCH.to_string()),
        Some(Value::String(t)) if check_template(&t).is_ok() => Ok(t),
        Some(_) => Err("search_setting"),
    }
}

/// What to load for `typed`. `search` is the request's flag: only a window says it. A local tab never searches (a search site is out of its reach). The folder of the setting is
/// asked for only when a search needs it.
pub(super) fn address(typed: &str, search: bool, local: bool, state_dir: impl FnOnce() -> Option<PathBuf>) -> Result<String, &'static str> {
    if !search || local {
        return parse_address(typed).map_err(address_code);
    }
    match classify(typed).map_err(address_code)? {
        Typed::Address(url) => Ok(url),
        Typed::Search(words) => {
            let dir = state_dir().ok_or("engine_failed")?;
            search_url(&template(&dir)?, &words).map_err(|e| match e {
                SearchError::TooLong => "address_too_long",
                SearchError::Empty => "address_empty",
                SearchError::Template => "search_setting",
            })
        }
    }
}

impl Browser {
    pub(super) fn address_for(&self, shared: &Opaque, typed: &str, search: bool, local: bool) -> Result<String, &'static str> {
        address(typed, search, local, || self.state_dir_of(shared))
    }
}
