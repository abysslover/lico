//! The small shared control set of the hand-drawn window: text labels (cached once shaped), buttons and list rows, each drawn and hit-tested
//! here and nowhere else. The menu bar and its entries, the Allow / Deny buttons, the session list and the hint line all go through it, so
//! a box that is drawn is exactly the box a click is tested against.

use std::collections::HashMap;

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap};

use super::ui::{blit, rect};
use super::FONT_PX;

pub type Rect = [f32; 4]; // x0, y0, x1, y1 in window pixels

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Menu(usize),
    Item(usize, usize),
    Session(String),
    Allow,
    Deny,
    StopRealScreen,
}

pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}

#[derive(Default)]
pub struct Controls {
    pub hits: Vec<Hit>,
    labels: HashMap<String, (Buffer, f32)>,
    pub scale: f32,
    pub font_px: f32,
    /// How many labels were shaped since the start (a label whose text and scale are unchanged is shaped once and reused).
    pub shapes: u64,
    /// The Allow and Deny buttons of the last frame were drawn dimmed (a card that may not be answered yet).
    pub dimmed: bool,
}

impl Controls {
    /// Start a frame: the boxes of the last one are forgotten, and shaped labels are dropped when the scale or text size changed or too many piled up.
    pub fn begin(&mut self, scale: f32, font_px: f32) {
        self.hits.clear();
        self.dimmed = false;
        let fp = if font_px <= 0.0 { FONT_PX } else { font_px };
        if (self.scale - scale).abs() > f32::EPSILON || (self.font_px - fp).abs() > f32::EPSILON || self.labels.len() > 600 {
            self.labels.clear();
        }
        self.scale = scale;
        self.font_px = fp;
    }

    fn label(&mut self, fs: &mut FontSystem, text: &str) -> &mut (Buffer, f32) {
        let s = self.scale;
        let fp = if self.font_px <= 0.0 { FONT_PX } else { self.font_px };
        let lh = fp + 4.0;
        if !self.labels.contains_key(text) {
            let mut b = Buffer::new(fs, Metrics::new(fp * s, lh * s));
            b.set_wrap(Wrap::None);
            b.set_size(Some(4000.0), Some(lh * s));
            b.set_text(text, &Attrs::new().family(Family::Monospace), Shaping::Advanced, None);
            b.shape_until_scroll(fs, true);
            let width = b.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
            self.shapes += 1;
            self.labels.insert(text.to_string(), (b, width));
        }
        self.labels.get_mut(text).unwrap()
    }

    /// The width a text takes when drawn.
    pub fn width(&mut self, fs: &mut FontSystem, text: &str) -> f32 {
        self.label(fs, text).1
    }

    /// A text with its top left at (x, y), cut at `clip_x`; returns its width.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, x: f32, y: f32, clip_x: i32, text: &str, ink: Color) -> f32 {
        let entry = self.label(fs, text);
        let w = entry.1;
        blit(&mut entry.0, fs, swash, buf, width, height, x as i32, y as i32, clip_x, ink);
        w
    }

    /// A filled box with its label inside, and the action a click on it means.
    #[allow(clippy::too_many_arguments)]
    pub fn button(&mut self, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, r: Rect, label: &str, fill: u32, ink: Color, action: Action) {
        rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, fill);
        let pad = 6.0 * self.scale;
        self.text(fs, swash, buf, width, height, r[0] + pad, r[1], r[2] as i32, label, ink);
        self.hits.push(Hit { rect: r, action });
    }

    /// The Allow and Deny buttons of the first pending approval, at the right edge of its row: drawn dimmed while `live` is false (a card of the host's own that
    /// has not been on screen for the dwell yet; a click on them answers nothing then).
    #[allow(clippy::too_many_arguments)]
    pub fn answer_buttons(&mut self, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, y: f32, ink: Color, live: bool) {
        let fp = if self.font_px <= 0.0 { FONT_PX } else { self.font_px };
        let (s, lh) = (self.scale, (fp + 4.0) * self.scale);
        let (allow, deny, ink) = if live { (0xFF2E_6B3A, 0xFF7A_2E2E, ink) } else { (0xFF26_3A2C, 0xFF3A_2828, Color::rgb(0x9A, 0xA4, 0xB0)) };
        self.dimmed = !live;
        self.button(fs, swash, buf, width, height, [width as f32 - 190.0 * s, y, width as f32 - 100.0 * s, y + lh], "Allow", allow, ink, Action::Allow);
        self.button(fs, swash, buf, width, height, [width as f32 - 95.0 * s, y, width as f32 - 5.0 * s, y + lh], "Deny", deny, ink, Action::Deny);
    }

    /// A row of a list: highlighted when selected, its text cut at the row's right edge.
    #[allow(clippy::too_many_arguments)]
    pub fn row(&mut self, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, r: Rect, label: &str, selected: bool, ink: Color, action: Action) {
        if selected {
            rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, 0xFF22_3A58);
        }
        let pad = 4.0 * self.scale;
        self.text(fs, swash, buf, width, height, r[0] + pad, r[1], (r[2] - 2.0) as i32, label, ink);
        self.hits.push(Hit { rect: r, action });
    }

    /// Only a hit box; for a thing that was drawn some other way.
    pub fn target(&mut self, r: Rect, action: Action) {
        self.hits.push(Hit { rect: r, action });
    }

    /// What a click at (x, y) means: the box drawn last (on top) wins.
    pub fn hit_at(&self, x: f32, y: f32) -> Option<Action> {
        self.hits.iter().rev().find(|h| x >= h.rect[0] && x < h.rect[2] && y >= h.rect[1] && y < h.rect[3]).map(|h| h.action.clone())
    }
}

/// Fill a box (used for backgrounds that belong to a control but carry no text).
pub fn fill(buf: &mut [u32], width: usize, height: usize, r: Rect, color: u32) {
    rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, color);
}
