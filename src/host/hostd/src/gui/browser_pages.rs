//! The remembered pages in the window (M22-P05c): a click on a page's card in Chats opens it in a new tab, the menu's `Reopen closed tab` asks the host for the newest remembered page
//! that no tab shows, `Clear remembered pages` empties the list. The host holds the list (`pages.rs`), tells it in the Chats list (`session.chats` with `pages`) and gives the address
//! only from that list; the window sends an address only for a card it shows, as `browser.open` takes it (exact, never a search).

use serde_json::{json, Value};

use super::super::App;
use super::BrowserJob;

impl App {
    /// A click on the card of a remembered page: a new tab on that address.
    pub(crate) fn browser_open_page(&mut self, address: &str) {
        if self.socket.is_empty() {
            return;
        }
        self.browser_call(json!({"op": "browser.open", "url": address}), 30, BrowserJob::Open { local: false });
    }

    /// `Reopen closed tab`: the host opens the newest remembered page that no tab shows (`no_page` is told in the flash when there is none).
    pub(crate) fn browser_reopen(&mut self) {
        if self.socket.is_empty() {
            return;
        }
        self.browser_call(json!({"op": "browser.reopen"}), 30, BrowserJob::Open { local: false });
    }

    /// `Clear remembered pages`.
    pub(crate) fn browser_forget_pages(&mut self) {
        if self.socket.is_empty() {
            return;
        }
        self.browser_call(json!({"op": "browser.forget"}), 8, BrowserJob::Forget);
    }

    /// The host's answer to `browser.forget`: told in the flash, and the Chats list (when it is shown) loses the pages at once.
    pub(super) fn browser_forgot(&mut self, reply: &Value) {
        self.ui.flash = Some(match reply["forgotten"].as_u64() {
            _ if reply["ok"] != json!(1) => "the remembered pages could not be cleared".to_string(),
            Some(0) | None => "No remembered pages.".to_string(),
            Some(n) => format!("Remembered pages cleared ({n})."),
        });
        self.chats_sync_if_open(true);
        self.dirty = true;
    }
}
