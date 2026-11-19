//! The chips and cards of the new-session wizard's second and third step (`M22-P07`), painted over the lines of the rows that have choices: the account, the model, the effort, the permission
//! and the worktree as chips in one row, the recent folders, the servers, the sign-in, Back, Next and Start as cards. A veil of the window's own ground covers the row's text (the text stays
//! the model: the keys, the dump and the one-page form read it), then each box is a target of its own (`Action::WizardChoose`). What the boxes are and which is chosen is the wizard's
//! (`newsession_wizard.rs`, `wizard_cards`); nothing here reads the host.

use cosmic_text::{Color, SwashCache};

use super::super::super::controls::{self, Action};
use super::super::super::{App, BACKGROUND};
use super::icons::{self, Icon};
use super::picker::{FILL, GREY_FILL, GREY_INK, INK, LIT, RING};

const DIM: u32 = 0xFF8A_94A0;

/// The icon a box names.
fn icon_of(name: &str) -> Option<Icon> {
    Some(match name {
        "claude" => Icon::Claude,
        "codex" => Icon::Codex,
        "local" => Icon::Local,
        "plus" => Icon::Plus,
        "account" => Icon::Account,
        "back" => Icon::Back,
        "forward" => Icon::Forward,
        "reload" => Icon::Reload,
        _ => return None,
    })
}

/// The rectangles of boxes of the given widths on one line (`y0` to `y1`): left-aligned from `x0`, a gap between them, a little air above and below, and none that would end beyond
/// `x_max` (the first that does not fit and every one after it is left out: it can still be chosen with the keys).
pub(super) fn box_rects(widths: &[f32], (x0, x_max): (f32, f32), (y0, y1): (f32, f32), s: f32) -> Vec<[f32; 4]> {
    let (gap, air) = ((8.0 * s).round(), s.round().max(1.0));
    let mut x = x0;
    let mut out = Vec::new();
    for w in widths {
        if x + w > x_max {
            break;
        }
        out.push([x, y0 + air, x + w, y1 - air]);
        x += w + gap;
    }
    out
}

impl App {
    /// The boxes of the wizard's rows, over the text of the lines they cover. The rectangles are kept for the dump and for the targets.
    pub(super) fn cards_paint(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, s: f32) {
        self.ui.toolbar_geom.cards.clear();
        let cards = self.wizard_cards();
        if cards.is_empty() {
            return;
        }
        let (x_text, char_w) = (list_w as f32 + 4.0 * s, self.char_w() * s);
        let x_value = x_text + 13.0 * char_w; // (where the value of a row starts in the text: the mark, a space and the word's eleven columns)
        let lines: Vec<(usize, f32, f32)> = cards.iter().filter_map(|c| self.ui.hit_lines.iter().find(|l| l.2 == c.line).map(|l| (c.line, l.0, l.1))).collect();
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let mut drawn = Vec::new();
        for row in &cards {
            let Some(&(_, y0, y1)) = lines.iter().find(|l| l.0 == row.line) else { continue };
            controls::fill(buf, width, height, [list_w as f32 + 1.0, y0, width as f32, y1], BACKGROUND); // the veil
            let label_h = (controls.font_px + 4.0) * controls.scale;
            if !row.label.is_empty() {
                controls.text(fs, swash, buf, width, height, x_text + 2.0 * char_w, y0 + ((y1 - y0 - label_h) * 0.5).max(0.0), x_value as i32, row.label, Color(DIM));
            }
            let (pad, side) = ((10.0 * s).round(), ((y1 - y0) - 6.0 * s).max(8.0));
            let widths: Vec<f32> = row
                .items
                .iter()
                .map(|i| {
                    let icon = if i.icon.and_then(icon_of).is_some() { side + pad * 0.5 } else { 0.0 };
                    let hint = if i.hint.is_empty() { 0.0 } else { controls.width(fs, &i.hint) + 2.0 * char_w };
                    2.0 * pad + icon + controls.width(fs, &i.text) + hint
                })
                .collect();
            let rects = box_rects(&widths, (x_value, width as f32 - 4.0 * s), (y0, y1), s);
            // a click beside the boxes is the row's own: nothing for a row of chips, the card for a card
            controls.target([list_w as f32 + 1.0, y0, width as f32, y1], Action::WizardChoose(row.line, if row.chips { usize::MAX } else { 0 }));
            for (i, (item, r)) in row.items.iter().zip(&rects).enumerate() {
                let (fill, ink) = match (item.enabled, item.chosen) {
                    (false, _) => (GREY_FILL, GREY_INK),
                    (true, true) => (LIT, INK),
                    (true, false) => (FILL, INK),
                };
                if row.focused && (item.chosen || !row.chips) {
                    let b = (2.0 * s).round().max(2.0);
                    controls::fill(buf, width, height, [r[0] - b, r[1] - b, r[2] + b, r[3] + b], RING);
                }
                controls::fill(buf, width, height, *r, fill);
                let mut x = r[0] + pad;
                let box_h = r[3] - r[1];
                if let Some(icon) = item.icon.and_then(icon_of) {
                    let q = (box_h - 3.0 * s).max(8.0);
                    let at = [x, r[1] + (box_h - q) * 0.5, x + q, r[1] + (box_h - q) * 0.5 + q];
                    icons::draw(buf, width, height, icon, at, ink, fill);
                    x += q + pad * 0.5;
                }
                let y = r[1] + (box_h - label_h) * 0.5; // (the label is a little taller than the box: its glyphs sit in the middle of it)
                x += controls.text(fs, swash, buf, width, height, x, y, r[2] as i32, &item.text, Color(ink));
                if !item.hint.is_empty() {
                    controls.text(fs, swash, buf, width, height, x + 2.0 * char_w, y, r[2] as i32, &item.hint, Color(if item.enabled { DIM } else { GREY_INK }));
                }
                controls.target(*r, Action::WizardChoose(row.line, i));
            }
            drawn.push((row.line, rects));
        }
        self.ui.toolbar_geom.cards = drawn;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_are_left_aligned_with_a_gap_and_a_little_air_above_and_below() {
        let r = box_rects(&[60.0, 80.0], (100.0, 400.0), (50.0, 68.0), 1.0);
        assert_eq!(r, vec![[100.0, 51.0, 160.0, 67.0], [168.0, 51.0, 248.0, 67.0]]);
        let twice = box_rects(&[60.0], (100.0, 400.0), (50.0, 86.0), 2.0);
        assert_eq!(twice, vec![[100.0, 52.0, 160.0, 84.0]], "the air grows with the scale");
    }

    #[test]
    fn a_box_that_would_end_beyond_the_edge_and_every_one_after_it_is_left_out() {
        let r = box_rects(&[100.0, 100.0, 10.0], (0.0, 215.0), (0.0, 20.0), 1.0);
        assert_eq!(r.len(), 2, "both of the first two fit and the third, which would end at 226, does not: {r:?}");
        let r = box_rects(&[100.0, 200.0, 10.0], (0.0, 215.0), (0.0, 20.0), 1.0);
        assert_eq!(r.len(), 1, "the second does not fit, so the third, which would, is not drawn either: the order stays the order of the row");
        assert!(box_rects(&[300.0], (0.0, 215.0), (0.0, 20.0), 1.0).is_empty());
        assert_eq!(box_rects(&[100.0, 107.0], (0.0, 215.0), (0.0, 20.0), 1.0).len(), 2, "a box that ends exactly at the edge is kept");
    }

    #[test]
    fn every_name_a_box_may_carry_is_an_icon_and_an_unknown_one_is_none() {
        for name in ["claude", "codex", "local", "plus", "account", "back", "forward", "reload"] {
            assert_eq!(icon_of(name).map(Icon::name), Some(name), "{name}");
        }
        assert!(icon_of("folder").is_none());
    }
}
