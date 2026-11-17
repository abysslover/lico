//! The icons of the toolbar (`M22-P04`): a plus (New session), a house (New Home session) and a browser window (Open browser), drawn by the window itself from rectangles, so that no
//! font glyph has to be checked and each icon is the same pixels in every font. Each is drawn inside a rectangle, centred, in one colour; the caller draws the button's own fill (and
//! the colour that says it is hovered or greyed). Nothing is drawn outside the rectangle.

use super::super::controls::{fill, Rect};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Icon {
    Plus,
    House,
    Browser,
}

impl Icon {
    /// The name the window state gives it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Icon::Plus => "plus",
            Icon::House => "house",
            Icon::Browser => "browser",
        }
    }
}

/// Draws `icon` centred in `r` in `ink`; `ground` is the colour of the button under it (the door of the house and the inside of the browser window are that colour).
pub(crate) fn draw(buf: &mut [u32], width: usize, height: usize, icon: Icon, r: Rect, ink: u32, ground: u32) {
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    let size = w.min(h);
    if size < 8.0 {
        return;
    }
    let (cx, cy) = ((r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5);
    let side = (size * 0.56).round().max(8.0);
    let (x0, y0) = ((cx - side * 0.5).round(), (cy - side * 0.5).round());
    let (x1, y1) = (x0 + side, y0 + side);
    let t = (side * 0.16).round().max(2.0); // the stroke
    let mut put = |a: f32, b: f32, c: f32, d: f32, col: u32| {
        // every rectangle is clipped to the button's, so that nothing is ever drawn outside it
        let q = [a.max(r[0]), b.max(r[1]), c.min(r[2]), d.min(r[3])];
        if q[2] > q[0] && q[3] > q[1] {
            fill(buf, width, height, q, col);
        }
    };
    match icon {
        Icon::Plus => {
            let (mx, my) = (cx.round(), cy.round());
            put(x0, my - t * 0.5, x1, my - t * 0.5 + t, ink);
            put(mx - t * 0.5, y0, mx - t * 0.5 + t, y1, ink);
        }
        Icon::House => {
            // the roof is a triangle, one row of pixels at a time; the body sits under it with a door of the button's colour
            let roof_h = (side * 0.46).round();
            let rows = roof_h as usize;
            for i in 0..rows {
                let half = (side * 0.5) * (i as f32 + 1.0) / roof_h;
                put((cx - half).round(), y0 + i as f32, (cx + half).round(), y0 + i as f32 + 1.0, ink);
            }
            let body_half = side * 0.38;
            put((cx - body_half).round(), y0 + roof_h, (cx + body_half).round(), y1, ink);
            let door = (side * 0.11).round().max(1.0);
            put((cx - door).round(), y1 - (side * 0.30).round(), (cx + door).round(), y1, ground);
        }
        Icon::Browser => {
            // a window: a frame, a title band across the top and a line for the address in it
            let (fy0, fy1) = ((y0 + side * 0.08).round(), (y1 - side * 0.08).round());
            put(x0, fy0, x1, fy1, ink);
            put(x0 + t, fy0 + (side * 0.30).round(), x1 - t, fy1 - t, ground);
            let ay = (fy0 + (side * 0.09).round()).max(fy0 + 1.0);
            put(x0 + (side * 0.30).round(), ay, x1 - t - 1.0, ay + (side * 0.10).round().max(1.0), ground);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: u32 = 0xFF00_0000;
    const INK: u32 = 0xFFFF_FFFF;
    const BTN: u32 = 0xFF23_2E3E;

    /// The icon in a 40x40 button inside a 64x64 canvas: the canvas, and the button's rectangle.
    fn canvas(icon: Icon) -> (Vec<u32>, Rect) {
        let mut buf = vec![BG; 64 * 64];
        let r = [12.0, 12.0, 52.0, 52.0];
        fill(&mut buf, 64, 64, r, BTN);
        draw(&mut buf, 64, 64, icon, r, INK, BTN);
        (buf, r)
    }

    fn inked(buf: &[u32]) -> Vec<(usize, usize)> {
        (0..64).flat_map(|y| (0..64).map(move |x| (x, y))).filter(|&(x, y)| buf[y * 64 + x] == INK).collect()
    }

    #[test]
    fn every_icon_inks_pixels_inside_its_button_and_never_outside_it() {
        for icon in [Icon::Plus, Icon::House, Icon::Browser] {
            let (buf, r) = canvas(icon);
            let px = inked(&buf);
            assert!(px.len() > 60, "{icon:?} draws something: {}", px.len());
            assert!(px.iter().all(|&(x, y)| (x as f32) >= r[0] && (x as f32) < r[2] && (y as f32) >= r[1] && (y as f32) < r[3]), "{icon:?} stays inside the button");
            assert!(buf[..64 * 12].iter().all(|&c| c == BG), "{icon:?}: nothing above the button");
        }
    }

    #[test]
    fn the_three_icons_are_three_different_pictures() {
        let [a, b, c] = [Icon::Plus, Icon::House, Icon::Browser].map(|i| canvas(i).0);
        assert!(a != b && b != c && a != c);
    }

    #[test]
    fn the_plus_and_the_house_are_centred_and_the_plus_is_a_cross() {
        let (plus, _) = canvas(Icon::Plus);
        let px = inked(&plus);
        let (min_x, max_x) = (px.iter().map(|p| p.0).min().unwrap(), px.iter().map(|p| p.0).max().unwrap());
        let (min_y, max_y) = (px.iter().map(|p| p.1).min().unwrap(), px.iter().map(|p| p.1).max().unwrap());
        assert!((min_x + max_x).abs_diff(64) <= 2 && (min_y + max_y).abs_diff(64) <= 2, "centred at 32,32: x {min_x}..{max_x}, y {min_y}..{max_y}");
        assert!(px.contains(&(32, 22)) && px.contains(&(32, 41)) && px.contains(&(22, 32)) && px.contains(&(41, 32)), "four arms");
        assert!(!px.contains(&(24, 24)) && !px.contains(&(40, 40)), "and a cross, not a square");
        let (house, _) = canvas(Icon::House);
        let hp = inked(&house);
        let (hmin, hmax) = (hp.iter().map(|p| p.0).min().unwrap(), hp.iter().map(|p| p.0).max().unwrap());
        assert!((hmin + hmax).abs_diff(64) <= 2, "the house is centred: {hmin}..{hmax}");
        assert!(house[41 * 64 + 32] == BTN, "the door is the colour of the button");
    }

    #[test]
    fn a_rectangle_too_small_to_hold_an_icon_gets_none() {
        let mut buf = vec![BG; 64 * 64];
        draw(&mut buf, 64, 64, Icon::House, [0.0, 0.0, 6.0, 6.0], INK, BTN);
        assert!(buf.iter().all(|&c| c == BG));
    }
}
