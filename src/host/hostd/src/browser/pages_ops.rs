//! The remembered pages in the browser adapter (M22-P05c): what is remembered when (`note`, called by the supervisor when a page has loaded), and the two operations
//! `browser.reopen` (the newest remembered page that no tab shows) and `browser.forget` (the list emptied). The store is `crate::pages`; an address comes into a tab only from
//! that list, never from the caller.

use super::ops::Browser;
use crate::driver::Opaque;
use crate::pages::{self, Page};
use serde_json::{json, Map, Value};
use std::path::Path;

pub const PAGES_NAMES: [&str; 2] = ["browser.reopen", "browser.forget"];

type Reply = Result<Map<String, Value>, &'static str>;

/// The request fields of the two operations: none of their own (`op`, `v` and `id` only), as for the others unknown fields are refused.
pub fn valid(op: &str, request: &Map<String, Value>) -> bool {
    PAGES_NAMES.contains(&op) && request.iter().all(|(k, v)| matches!(k.as_str(), "op" | "v" | "id") && super::ops::valid_common(k, v))
}

/// Remembers the page a tab shows once it has loaded: a tab for this machine's own services never (`local`), a page that has not finished loading (`ready` false) never. The
/// answer is why a page was not kept; the supervisor says only `not_saved`.
pub(super) fn note(dir: &Path, local: bool, ready: bool, url: &str, title: &str, now: u64) -> Result<(), &'static str> {
    if local {
        return Err("local");
    }
    if !ready {
        return Err("not_ready");
    }
    pages::remember(dir, url, title, now)
}

/// The newest remembered page (the list is newest first) that none of `shown` addresses is; a shown address is taken without its fragment, as the list keeps it.
pub(super) fn to_reopen<'a>(pages: &'a [Page], shown: &[String]) -> Option<&'a Page> {
    pages.iter().find(|p| !shown.iter().any(|s| pages::strip_fragment(s) == p.address))
}

impl Browser {
    /// `browser.reopen`: opens the newest remembered page that no tab shows, as `browser.open` does (the same reply, and `url`), else `no_page`.
    pub(super) fn reopen(&self, shared: &Opaque) -> Reply {
        let dir = self.state_dir_of(shared).ok_or("engine_failed")?;
        let shown: Vec<String> = self.hub.tabs.lock().unwrap().iter().filter(|e| !e.local).map(|e| e.tab.url().to_string()).collect();
        let list = pages::load(&dir);
        let page = to_reopen(&list, &shown).ok_or("no_page")?;
        let mut reply = self.open_url(shared, &page.address, false)?;
        reply.insert("url".into(), json!(page.address));
        Ok(reply)
    }

    /// `browser.forget`: empties the list and says how many pages it held.
    pub(super) fn forget(&self, shared: &Opaque) -> Reply {
        let dir = self.state_dir_of(shared).ok_or("engine_failed")?;
        let mut reply = Map::new();
        reply.insert("forgotten".into(), json!(pages::forget(&dir)));
        Ok(reply)
    }
}
