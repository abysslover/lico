//! The cues that make a page with the keys look alive (M17-P11): the engine draws no caret and no focus ring, so the window draws a ring at the person's last click, makes the
//! pointer a crosshair over the page, and says in the header where typing goes. Nothing here is sent to the engine.

use super::App;

/// The colour of the click ring: an orange that shows on a white page and on a dark one.
pub(crate) const MARK_COLOUR: u32 = 0xFFE8_590C;

impl App {
    /// The click ring to show: the last click on the page while the page is shown with the keys (the keys went to the address row: no ring), and no help over it.
    pub(crate) fn browser_mark(&self) -> Option<(i32, i32)> {
        let b = &self.ui.browser;
        if self.browser_active() && b.page_focus && !b.help { b.mark } else { None }
    }

    /// Paints the click ring over the frame that was just painted, clipped to the page area and the window, and records the pixel a test reads.
    pub(crate) fn browser_paint_mark(&mut self, buf: &mut [u32], width: usize, height: usize) {
        let [x0, y0, w, h] = self.ui.browser.rect;
        self.ui.browser.mark_pixel = 0;
        let Some((mx, my)) = self.browser_mark() else { return };
        let len = buf.len();
        let at = move |dx: i32, dy: i32| {
            let (px, py) = (x0 + mx + dx, y0 + my + dy);
            (px >= x0 && py >= y0 && px < x0 + w && py < y0 + h && px < width as i32 && py < height as i32 && (py as usize) * width + (px as usize) < len).then(|| py as usize * width + px as usize)
        };
        for (dx, dy) in lico_browser_view::click_mark(lico_browser_view::MARK_RADIUS, lico_browser_view::MARK_THICKNESS) {
            if let Some(i) = at(dx, dy) {
                buf[i] = MARK_COLOUR;
            }
        }
        self.ui.browser.mark_pixel = at(lico_browser_view::MARK_RADIUS, 0).map_or(0, |i| buf[i]);
    }

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

    /// Whether the person's typing goes to the page now.
    pub(crate) fn browser_page_has_the_keys(&self) -> bool {
        self.browser_to_page().is_some()
    }
}
