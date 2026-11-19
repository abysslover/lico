//! The cues that make a page with the keys look alive (M17-P11): the engine draws no caret and no focus ring, so the window makes the pointer a crosshair over the page and says in
//! the bottom row where typing goes. (The ring at the last click went in M22-P05a, at the owner's word.) Nothing here is sent to the engine.

use super::App;

impl App {
    /// The pointer is a crosshair over the page (the engine draws none, and a page that is a picture says nothing about being live) and the window's own elsewhere.
    pub(crate) fn browser_pointer(&mut self) {
        let on = self.browser_point().is_some();
        if on != self.ui.browser.pointer_on_page {
            self.ui.browser.pointer_on_page = on;
            if let Some(window) = &self.window {
                window.set_cursor(if on { winit::window::CursorIcon::Crosshair } else { winit::window::CursorIcon::Default });
            }
        }
    }
}
