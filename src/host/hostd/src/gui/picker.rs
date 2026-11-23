//! What the window paints over the main area after its text (`M22-P04b`): the browser page, and the icons of the new-session wizard's first step (Claude, agy, Codex, Local LLM, Terminal), drawn over
//! the blank lines the panel's body keeps for them. Each icon is a square with a target of its own (`Action::WizardPick`); the one the keys are on has a ring, the one the pointer is on is
//! lit, and an icon of a provider with no account is grey. Nothing here reads the host: what the icons are is the wizard's (`newsession_wizard.rs`).

use super::super::super::controls::{self, Action};
use super::super::super::scroll::{self, Composed};
use super::super::super::App;
use super::icons::{self, Icon};

pub(super) const FILL: u32 = 0xFF23_2E3E;
pub(super) const LIT: u32 = 0xFF2C_4A78;
pub(super) const GREY_FILL: u32 = 0xFF1B_222D;
pub(super) const RING: u32 = 0xFF5C_8AD0;
pub(super) const INK: u32 = 0xFFE0_E0E0;
pub(super) const GREY_INK: u32 = 0xFF70_7880;

/// How one icon is drawn.
pub(super) struct Look {
    pub(super) icon: Icon,
    pub(super) enabled: bool,
    pub(super) focused: bool,
    pub(super) hovered: bool,
}

/// The squares of `n` icons inside `region`: as tall as the region lets them be (at most 112 logical pixels), left-aligned, a gap of a fifth of their side between them. None when the region
/// cannot hold them (a window too small).
pub(super) fn tile_rects(region: [f32; 4], s: f32, n: usize) -> Vec<[f32; 4]> {
    let (pad, gap_k) = (4.0 * s, 0.22);
    let (h, w) = (region[3] - region[1] - 2.0 * pad, region[2] - region[0]);
    let side = h.min(112.0 * s).min(w / (n as f32 + gap_k * (n as f32 - 1.0)));
    if side < 28.0 * s {
        return Vec::new();
    }
    let (gap, top) = (side * gap_k, region[1] + pad + (h - side) * 0.5);
    (0..n)
        .map(|i| {
            let x = region[0] + i as f32 * (side + gap);
            [x.round(), top.round(), (x + side).round(), (top + side).round()]
        })
        .collect()
}

/// Draws the squares: lit when the keys or the pointer are on one, a ring around the one with the focus, its icon in the middle, all grey when it cannot be chosen.
pub(super) fn draw_tiles(buf: &mut [u32], width: usize, height: usize, s: f32, rects: &[[f32; 4]], looks: &[Look]) {
    for (r, look) in rects.iter().zip(looks) {
        let (fill, ink) = match (look.enabled, look.focused || look.hovered) {
            (false, _) => (GREY_FILL, GREY_INK),
            (true, true) => (LIT, INK),
            (true, false) => (FILL, INK),
        };
        if look.focused {
            let b = (2.0 * s).round().max(2.0);
            controls::fill(buf, width, height, [r[0] - b, r[1] - b, r[2] + b, r[3] + b], RING);
        }
        controls::fill(buf, width, height, *r, fill);
        icons::draw(buf, width, height, look.icon, *r, ink, fill);
    }
}

impl App {
    /// Over the text of the main area: the browser page, and the wizard's icons. (One call, so that `ui.rs` stays as small as it is.)
    pub(crate) fn paint_main_overlays(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, s: f32, (frame, first_bottom, prompt_fallback): (&Composed, f32, f32)) {
        if self.page_active() {
            let (top, bottom) = (frame.lines.first().map_or(first_bottom, |l| l.bottom), frame.lines.iter().find(|l| l.key == scroll::Key::Prompt).map_or(prompt_fallback, |l| l.top));
            if self.browser_active() {
                self.browser_paint(buf, width, height, (list_w as f32 + 4.0 * s, top, bottom));
            } else {
                self.terminal_paint(buf, width, height, (list_w as f32 + 4.0 * s, top, bottom));
            }
        }
        self.picker_paint(buf, width, height, list_w, s);
        self.cards_paint(buf, width, height, list_w, s);
        self.account_paint(buf, width, height, list_w, s);
    }

    /// The wizard's icons, over the blank lines of its first step (from the top of the first to the bottom of the last that is on screen), each a target of its own.
    fn picker_paint(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, s: f32) {
        self.ui.toolbar_geom.tiles.clear();
        let lines = self.wizard_tile_lines();
        let Some(tiles) = self.wizard_tiles().filter(|_| lines > 0) else {
            self.ui.toolbar_geom.tile_hover = None;
            return;
        };
        let rows: Vec<(f32, f32)> = self.ui.hit_lines.iter().filter(|l| l.2 < lines).map(|l| (l.0, l.1)).collect();
        let (Some(first), Some(last)) = (rows.first(), rows.last()) else { return };
        let rects = tile_rects([list_w as f32 + 4.0 * s, first.0, width as f32 - 4.0 * s, last.1], s, tiles.len());
        let looks: Vec<Look> = tiles
            .iter()
            .map(|t| Look { icon: match t.provider { "claude" => Icon::Claude, "codex" => Icon::Codex, "agy" => Icon::Agy, "terminal" => Icon::Terminal, _ => Icon::Local }, enabled: t.enabled, focused: t.focused, hovered: t.hovered })
            .collect();
        draw_tiles(buf, width, height, s, &rects, &looks);
        for (i, r) in rects.iter().enumerate() {
            self.ui.controls.target(*r, Action::WizardPick(i));
        }
        self.ui.toolbar_geom.tiles = rects;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: u32 = 0xFF00_0000;

    #[test]
    fn three_squares_of_one_size_left_aligned_with_a_gap_and_inside_the_room() {
        let region = [334.0, 100.0, 900.0, 208.0];
        let r = tile_rects(region, 1.0, 3);
        assert_eq!(r.len(), 3);
        let sides: Vec<f32> = r.iter().map(|q| q[2] - q[0]).collect();
        assert!(sides.iter().all(|w| (*w - sides[0]).abs() < 1.0) && r.iter().all(|q| ((q[3] - q[1]) - sides[0]).abs() < 1.0), "squares of one size: {r:?}");
        assert!(sides[0] <= 112.0 && sides[0] >= 96.0, "as tall as the room lets them be: {}", sides[0]);
        assert!(r[0][0] >= region[0] && r[2][2] <= region[2] && r[0][1] >= region[1] && r[0][3] <= region[3], "inside the room: {r:?}");
        let gaps = [r[1][0] - r[0][2], r[2][0] - r[1][2]];
        assert!(gaps[0] > 6.0 && (gaps[0] - gaps[1]).abs() < 1.5, "a gap that holds the focus ring: {gaps:?}");
    }

    #[test]
    fn five_squares_of_one_size_fit_the_room_the_window_gives_them_with_a_gap_between() {
        let region = [334.0, 100.0, 900.0, 208.0];
        let r = tile_rects(region, 1.0, 5);
        assert_eq!(r.len(), 5, "the five icons of the first step (M23-P04c)");
        let sides: Vec<f32> = r.iter().map(|q| q[2] - q[0]).collect();
        assert!(sides.iter().all(|w| (*w - sides[0]).abs() <= 1.0) && sides[0] >= 40.0, "squares of one size, to the pixel: {sides:?}");
        assert!(r[0][0] >= region[0] && r[4][2] <= region[2], "inside the room: {r:?}");
        assert!(r.windows(2).all(|w| w[1][0] - w[0][2] > 6.0), "a gap that holds the focus ring: {r:?}");
    }

    #[test]
    fn a_taller_room_does_not_make_them_bigger_than_112_and_a_narrow_one_makes_them_smaller() {
        let tall = tile_rects([0.0, 0.0, 2000.0, 900.0], 1.0, 3);
        assert!((tall[0][2] - tall[0][0] - 112.0).abs() < 1.0);
        let narrow = tile_rects([0.0, 0.0, 240.0, 300.0], 1.0, 3);
        assert!(narrow[2][2] <= 240.0 && narrow[0][2] - narrow[0][0] < 112.0, "{narrow:?}");
        let scaled = tile_rects([0.0, 0.0, 2000.0, 900.0], 2.0, 3);
        assert!((scaled[0][2] - scaled[0][0] - 224.0).abs() < 1.0, "twice the pixels at twice the scale");
    }

    #[test]
    fn a_room_that_cannot_hold_them_holds_none() {
        assert!(tile_rects([0.0, 0.0, 600.0, 30.0], 1.0, 3).is_empty());
        assert!(tile_rects([0.0, 0.0, 60.0, 300.0], 1.0, 3).is_empty());
    }

    fn drawn(looks: &[Look]) -> (Vec<u32>, Vec<[f32; 4]>) {
        let mut buf = vec![BG; 600 * 200];
        let rects = tile_rects([10.0, 10.0, 590.0, 190.0], 1.0, looks.len());
        draw_tiles(&mut buf, 600, 200, 1.0, &rects, looks);
        (buf, rects)
    }

    fn look(icon: Icon, enabled: bool, focused: bool, hovered: bool) -> Look {
        Look { icon, enabled, focused, hovered }
    }

    #[test]
    fn a_tile_is_lit_when_the_pointer_or_the_keys_are_on_it_grey_when_it_cannot_be_chosen_and_the_focus_has_a_ring() {
        let (buf, r) = drawn(&[look(Icon::Claude, true, true, false), look(Icon::Codex, true, false, true), look(Icon::Local, true, false, false)]);
        let corner = |i: usize| buf[(r[i][1] as usize + 2) * 600 + r[i][0] as usize + 2];
        assert_eq!((corner(0), corner(1), corner(2)), (LIT, LIT, FILL));
        let just_outside = |i: usize| buf[(r[i][1] as usize + 20) * 600 + r[i][0] as usize - 1];
        assert_eq!((just_outside(0), just_outside(1)), (RING, BG), "only the focus has a ring");
        let (grey, g) = drawn(&[look(Icon::Claude, false, false, true), look(Icon::Codex, true, false, false)]);
        assert_eq!(grey[(g[0][1] as usize + 2) * 600 + g[0][0] as usize + 2], GREY_FILL, "grey stays grey under the pointer");
        let ink_in = |buf: &[u32], q: [f32; 4], col: u32| (q[1] as usize..q[3] as usize).any(|y| (q[0] as usize..q[2] as usize).any(|x| buf[y * 600 + x] == col));
        assert!(ink_in(&grey, g[0], GREY_INK) && !ink_in(&grey, g[0], INK), "the icon of a grey tile is dim");
        assert!(ink_in(&grey, g[1], INK), "the icon of a tile that can be chosen is not");
    }

    #[test]
    fn each_tile_holds_its_own_picture() {
        let (buf, r) = drawn(&[look(Icon::Claude, true, false, false), look(Icon::Codex, true, false, false), look(Icon::Local, true, false, false)]);
        let crop = |q: [f32; 4]| -> Vec<u32> { (q[1] as usize..q[3] as usize).flat_map(|y| (q[0] as usize..q[2] as usize).map(move |x| (y, x))).map(|(y, x)| buf[y * 600 + x]).collect() };
        let (a, b, c) = (crop(r[0]), crop(r[1]), crop(r[2]));
        assert!(a != b && b != c && a != c);
        assert!(a.iter().any(|p| *p == INK) && b.iter().any(|p| *p == INK) && c.iter().any(|p| *p == INK));
    }
}
