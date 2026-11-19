//! The browser tab's bar (M22-P05a): `‹ › ⟳` and the address, painted over the first line of the browser view. The editor of the window (`self.ed`) stays where the address is edited, with its
//! text, cursor, selection, undo, parked drafts and the input method's pre-edit; this module only decides WHERE the address is drawn and gives the editor the geometry of that place
//! (the caret, the selection, the boxes of the glyphs for the mouse), so every key and every mouse path of the editor works on the bar as it did on the bottom row.
//!
//! While the bar does not have the keys (the page has them) the field shows the address cut in the middle (`lico_browser_view::display_address`); while it has them the whole address with its
//! caret and selection, scrolled sideways so that the caret is in view. The field's text is drawn into a scratch picture of the field's size and copied, so that scrolled text can
//! never reach the buttons or the sidebar.

use cosmic_text::SwashCache;
use serde_json::{json, Value};

use super::super::controls::{self, Rect};
use super::super::ui::icons::{self, Icon};
use super::super::App;
use super::BrowserJob;

const BAR_BG: u32 = 0xFF18_1E26;
const BTN_BG: u32 = 0xFF23_2E3E;
const FIELD_BG: u32 = 0xFF10_151C;
const FIELD_EDGE: u32 = 0xFF2F_3A4A;
const FOCUS_EDGE: u32 = 0xFF3A_82F7;
const INK: u32 = 0xFFD8_DEE8;
const DIM: u32 = 0xFF5A_6575;
const CARET: u32 = 0xFFE0_E0E0;
const SELECT: u32 = 0xFF2A_4A7A;
/// What an empty field that is not in use says.
pub(crate) const PLACEHOLDER: &str = "Search or type an address";
/// What an empty bar of a tab for this machine's own services says: it takes addresses only (a search site is out of its reach).
pub(crate) const PLACEHOLDER_LOCAL: &str = "Type an address";
/// How many columns of the text to the right of the caret the field keeps in view.
const AHEAD_COLS: f32 = 6.0;

/// Where the parts of the bar are, in window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BarLayout {
    pub bar: Rect,
    pub back: Rect,
    pub forward: Rect,
    pub reload: Rect,
    pub field: Rect,
}

/// What a point on the bar is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarHit {
    Back,
    Forward,
    Reload,
    Field,
}

/// The parts of a bar of the rectangle `bar` at scale `s`: three square buttons as high as the bar, a small gap between them, and the field from there to the bar's right edge.
pub(crate) fn layout(bar: Rect, s: f32) -> BarLayout {
    let h = (bar[3] - bar[1]).max(0.0);
    let gap = (2.0 * s).round();
    let mut x = bar[0];
    let mut button = || {
        let r = [x.min(bar[2]), bar[1], (x + h).min(bar[2]), bar[3]];
        x += h + gap;
        r
    };
    let (back, forward, reload) = (button(), button(), button());
    let left = x.min(bar[2]); // `x` is already one gap past the last button
    BarLayout { bar, back, forward, reload, field: [left, bar[1], bar[2], bar[3]] }
}

fn inside(r: Rect, x: f32, y: f32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

/// What the point (x, y) is on, if it is on the bar.
pub(crate) fn hit(l: &BarLayout, x: f32, y: f32) -> Option<BarHit> {
    [(l.back, BarHit::Back), (l.forward, BarHit::Forward), (l.reload, BarHit::Reload), (l.field, BarHit::Field)].into_iter().find(|(r, _)| inside(*r, x, y)).map(|(_, h)| h)
}

/// How far the text is scrolled to the left (pixels) so that the caret, with `ahead` of the text after it, is inside a field `inner_w` wide; 0 while all of it fits.
pub(crate) fn scroll(caret_x: f32, caret_w: f32, ahead: f32, text_w: f32, inner_w: f32) -> f32 {
    ((caret_x + caret_w + ahead).min(text_w + caret_w) - inner_w).max(0.0)
}

impl App {
    /// What a press or a release on the bar means; true when it was the bar's. A press on a button asks the host to go back, forward or reload (nothing while there is no tab); a press on the
    /// field gives it the keys with the whole address selected when the page had them (when it has them already, the editor's own mouse code places the caret).
    pub(super) fn browser_bar_button(&mut self, pressed: bool) -> bool {
        let Some(l) = self.ui.browser.bar.filter(|_| self.browser_active()) else { return false };
        let Some(h) = hit(&l, self.ui.mouse.0 as f32, self.ui.mouse.1 as f32) else { return false };
        match h {
            BarHit::Field if pressed && self.ui.browser.page_focus => self.browser_focus_address(),
            BarHit::Field => return false,
            BarHit::Back | BarHit::Forward | BarHit::Reload if pressed => {
                if let Some(tab) = self.ui.browser.active.clone() {
                    self.browser_nav(&tab, if h == BarHit::Back { "back" } else if h == BarHit::Forward { "forward" } else { "reload" });
                }
            }
            _ => {}
        }
        true
    }

    /// The address bar takes the keys, with the address selected (a click on it, Ctrl+L).
    pub(super) fn browser_focus_address(&mut self) {
        self.ui.browser.page_focus = false;
        self.ed.select_all();
        self.dirty = true;
    }

    pub(super) fn browser_nav(&mut self, tab: &str, action: &str) {
        self.browser_call(json!({"op": "browser.nav", "tab": tab, "action": action}), 8, BrowserJob::Done);
    }

    /// Paints the bar over the first line of the view (`top` to `bottom`, from `x0` to the window's right edge, less a margin).
    pub(super) fn browser_paint_bar(&mut self, buf: &mut [u32], width: usize, height: usize, x0: f32, top: f32, bottom: f32) {
        let s = self.ui.scale.max(1.0);
        let l = layout([x0, top, (width as f32 - 4.0 * s).max(x0), bottom], s);
        self.ui.browser.bar = Some(l);
        let focused = !self.ui.browser.page_focus;
        let enabled = self.ui.browser.active.is_some();
        controls::fill(buf, width, height, l.bar, BAR_BG);
        controls::fill(buf, width, height, l.field, if focused { FOCUS_EDGE } else { FIELD_EDGE });
        let b = s.round().max(1.0);
        let inner = [l.field[0] + b, l.field[1] + b, l.field[2] - b, l.field[3] - b];
        controls::fill(buf, width, height, inner, FIELD_BG);
        self.browser_paint_field(buf, width, height, inner, focused);
        for (r, icon) in [(l.back, Icon::Back), (l.forward, Icon::Forward), (l.reload, Icon::Reload)] {
            controls::fill(buf, width, height, r, BTN_BG);
            icons::draw(buf, width, height, icon, r, if enabled { INK } else { DIM }, BTN_BG);
        }
    }

    /// What an empty field says: words may be searched, except in a tab for this machine's own services (the prompt for one, or an open one).
    fn browser_placeholder(&self) -> &'static str {
        let b = &self.ui.browser;
        let local = match &b.active {
            Some(id) => b.tabs.iter().any(|t| &t.id == id && t.local),
            None => b.prompt == Some(true),
        };
        if local { PLACEHOLDER_LOCAL } else { PLACEHOLDER }
    }

    /// The text of the field, drawn into a scratch picture of the field's inside and copied; with the keys, the editor is given the geometry of what is drawn.
    fn browser_paint_field(&mut self, buf: &mut [u32], width: usize, height: usize, inner: Rect, focused: bool) {
        let s = self.ui.scale.max(1.0);
        let pad = (5.0 * s).round();
        let (tx0, tx1) = (inner[0] + pad, inner[2] - pad);
        let (iw, ih) = ((tx1 - tx0).max(0.0) as usize, (inner[3] - inner[1]).max(0.0) as usize);
        if iw == 0 || ih == 0 || tx0 < 0.0 {
            return;
        }
        let char_w = self.char_w() * s;
        let preedit = self.preedit.clone();
        let placeholder = self.browser_placeholder();
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let ed = &mut self.ed;
        let mut scratch = vec![FIELD_BG; iw * ih];
        let shown = if focused {
            let prompt = ed.prompt(8192, 1, &preedit);
            let row = prompt.rows.first().cloned().unwrap_or_default();
            let drawn = format!("  {}", row.get(2..).unwrap_or(""));
            let spans = controls.glyph_spans(fs, &drawn);
            let shift = spans.iter().find(|g| g.0 >= 2).map(|g| g.2).or_else(|| spans.last().map(|g| g.3)).unwrap_or(0.0);
            let text_w = spans.last().map_or(0.0, |g| g.3) - shift;
            let caret_x = prompt.caret.map_or(text_w, |(_, off)| spans.iter().find(|g| g.0 >= off).map(|g| g.2).or_else(|| spans.last().map(|g| g.3)).unwrap_or(shift) - shift);
            let dx = scroll(caret_x, 1.5 * s, AHEAD_COLS * char_w, text_w, iw as f32);
            let origin = tx0 - shift - dx;
            controls.place_editor(fs, ed, &drawn, prompt.caret, [origin, inner[1], text_w + shift, s]);
            for r in &ed.sel_rects {
                controls::fill(&mut scratch, iw, ih, [r[0] - tx0, r[1] - inner[1], r[2] - tx0, r[3] - inner[1]], SELECT);
            }
            let empty = ed.text().is_empty() && preedit.is_empty();
            if empty {
                controls.text(fs, swash, &mut scratch, iw, ih, 0.0, 0.0, iw as i32, placeholder, cosmic_text::Color(DIM)); // an empty bar says what it is for, the caret before it
            } else {
                controls.text(fs, swash, &mut scratch, iw, ih, origin - tx0, 0.0, iw as i32, &drawn, cosmic_text::Color(INK));
            }
            if let Some([cx, _, cw, ch]) = ed.caret {
                controls::fill(&mut scratch, iw, ih, [cx - tx0, 0.0, cx - tx0 + cw, ch.min(ih as f32)], CARET);
            }
            if empty { placeholder.to_string() } else { drawn[2..].to_string() }
        } else {
            let empty = ed.text().is_empty();
            let text = if empty { placeholder.to_string() } else { lico_browser_view::display_address(ed.text(), (iw as f32 / char_w) as usize) };
            controls.text(fs, swash, &mut scratch, iw, ih, 0.0, 0.0, iw as i32, &text, cosmic_text::Color(if empty { DIM } else { INK }));
            text
        };
        for row in 0..ih {
            let y = inner[1] as usize + row;
            let x = tx0 as usize;
            if y >= height || x + iw > width {
                break;
            }
            buf[y * width + x..y * width + x + iw].copy_from_slice(&scratch[row * iw..(row + 1) * iw]);
        }
        self.ui.browser.bar_text = shown;
    }

    /// For the `ui` command: the parts of the bar and what the field shows.
    pub(super) fn browser_bar_json(&self) -> Value {
        let b = &self.ui.browser;
        match &b.bar {
            Some(l) if self.browser_active() => json!({
                "back": l.back, "forward": l.forward, "reload": l.reload, "field": l.field, "text": b.bar_text, "focus": !b.page_focus, "enabled": b.active.is_some(),
                "caret": self.ed.caret, "selection": self.ed.selected_text(),
            }),
            _ => Value::Null,
        }
    }
}

#[cfg(test)]
#[path = "browser_bar_tests.rs"]
mod tests;
