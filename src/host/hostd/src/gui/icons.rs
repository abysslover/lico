//! The icons of the window (`M22-P04`, `M22-P04b`, `M23-P03`): a plus (New session), a robot's head (New Bot session), a browser window (Open browser), a person (Accounts), the three things a session
//! can be (Claude, Codex, Local LLM) and a screen with a prompt (Terminal), drawn by the window itself from rectangles, so that no font glyph has to be checked and each icon is the same pixels in every font. Each is drawn
//! inside a rectangle, centred, in one colour; the caller draws the button's own fill (and the colour that says it is hovered or greyed). Nothing is drawn outside the rectangle.

use super::super::controls::{fill, Rect};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Icon {
    Plus,
    Bot,
    Browser,
    /// A person: a head over shoulders (the Accounts screen).
    Account,
    /// A spark of rays (a Claude session): no vendor logo, a shape of our own.
    Claude,
    /// The code brackets `</>` (a Codex session).
    Codex,
    /// A chip with pins (a Local LLM session).
    Local,
    /// A screen with the prompt `>_` cut out of it (a terminal).
    Terminal,
    /// An upward chevron over a dot (an `agy` session or the program `agy` in a terminal tab, `M23-P04a`): a shape of our own, like the others.
    Agy,
    /// A chevron pointing left, a chevron pointing right and an arc with an arrowhead (the browser bar's back, forward and reload, `M22-P05a`).
    Back,
    Forward,
    Reload,
    /// A key (sign in, `M24-P03`), a pencil (rename), a star (the default), a bin (remove) and a bin with a cross on it (remove and end its sessions): the five pictures of an account's page.
    Key,
    Pencil,
    Star,
    Trash,
    TrashX,
    /// A crescent (the theme is dark), a sun (light) and a disc that is solid on its left half and a ring on its right (system): the three states of the theme button (`M24-P11d`).
    Moon,
    Sun,
    Auto,
}

impl Icon {
    /// The name the window state gives it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Icon::Plus => "plus",
            Icon::Bot => "bot",
            Icon::Browser => "browser",
            Icon::Account => "account",
            Icon::Claude => "claude",
            Icon::Codex => "codex",
            Icon::Local => "local",
            Icon::Terminal => "terminal",
            Icon::Agy => "agy",
            Icon::Back => "back",
            Icon::Forward => "forward",
            Icon::Reload => "reload",
            Icon::Key => "key",
            Icon::Pencil => "pencil",
            Icon::Star => "star",
            Icon::Trash => "trash",
            Icon::TrashX => "trash_x",
            Icon::Moon => "moon",
            Icon::Sun => "sun",
            Icon::Auto => "auto",
        }
    }

    /// The picture of a program the host names in a terminal (`claude`, `codex`, `agy`); none for any other word.
    pub(crate) fn of_agent(word: &str) -> Option<Icon> {
        match word {
            "claude" => Some(Icon::Claude),
            "codex" => Some(Icon::Codex),
            "agy" => Some(Icon::Agy),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) const ALL: [Icon; 20] = [Icon::Plus, Icon::Bot, Icon::Browser, Icon::Account, Icon::Claude, Icon::Codex, Icon::Local, Icon::Terminal, Icon::Agy, Icon::Key, Icon::Pencil, Icon::Star, Icon::Trash, Icon::TrashX, Icon::Back, Icon::Forward, Icon::Reload, Icon::Moon, Icon::Sun, Icon::Auto];
}

type Put<'a> = dyn FnMut(f32, f32, f32, f32, u32) + 'a;

/// A line from `a` to `b` of squares `t` wide, one step along its longer side at a time.
fn stroke(put: &mut Put, a: (f32, f32), b: (f32, f32), t: f32, col: u32) {
    let n = (b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil().max(1.0) as usize;
    for i in 0..=n {
        let k = i as f32 / n as f32;
        let (x, y) = (((a.0 + (b.0 - a.0) * k) - t * 0.5).round(), ((a.1 + (b.1 - a.1) * k) - t * 0.5).round());
        put(x, y, x + t, y + t, col);
    }
}

/// A disc of radius `r` at `c`, one row of pixels at a time.
fn disc(put: &mut Put, c: (f32, f32), r: f32, col: u32) {
    for i in 0..(2.0 * r).ceil() as usize {
        let y = (c.1 - r).round() + i as f32;
        let dy = y + 0.5 - c.1;
        let half = (r * r - dy * dy).max(0.0).sqrt();
        put((c.0 - half).round(), y, (c.0 + half).round(), y + 1.0, col);
    }
}

/// The inside of a polygon (even-odd), one row of pixels at a time.
fn polygon(put: &mut Put, pts: &[(f32, f32)], col: u32) {
    let (top, bottom) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    for y in (top.floor() as i32)..(bottom.ceil() as i32) {
        let yc = y as f32 + 0.5;
        let mut xs: Vec<f32> = (0..pts.len())
            .filter_map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                ((a.1 <= yc && b.1 > yc) || (b.1 <= yc && a.1 > yc)).then(|| a.0 + (yc - a.1) / (b.1 - a.1) * (b.0 - a.0))
            })
            .collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks(2) {
            if let [from, to] = pair {
                put(from.round(), y as f32, to.round(), y as f32 + 1.0, col);
            }
        }
    }
}

/// Draws `icon` centred in `r` in `ink`; `ground` is the colour of the button under it (the eyes and the mouth of the bot and the inside of the browser window are that colour).
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
        Icon::Bot => {
            // a robot's head (`M25-P10`): an antenna with a ball on it, a box with an ear on each side, and two square eyes and a mouth cut out of the box in the button's colour
            let ear = (side * 0.10).round().max(2.0);
            let (hx0, hx1) = (x0 + ear, x1 - ear);
            let (hy0, hy1) = ((y0 + side * 0.24).round(), y1);
            let (head_w, head_h) = (hx1 - hx0, hy1 - hy0);
            put(hx0, hy0, hx1, hy1, ink);
            let (ey0, ey1) = ((hy0 + head_h * 0.28).round(), (hy0 + head_h * 0.68).round());
            put(x0, ey0, hx0, ey1, ink);
            put(hx1, ey0, x1, ey1, ink);
            let stem = (side * 0.10).round().max(2.0);
            put((cx - stem * 0.5).round(), (y0 + side * 0.10).round(), (cx - stem * 0.5).round() + stem, hy0 + 1.0, ink); // whole pixels, and one row into the head, so that no row is lost between them
            disc(&mut put, (cx, y0 + side * 0.09), side * 0.09, ink);
            let eye = (side * 0.13).round().max(2.0);
            let eye_top = (hy0 + head_h * 0.24).round();
            for centre in [hx0 + head_w * 0.28, hx0 + head_w * 0.72] {
                put((centre - eye * 0.5).round(), eye_top, (centre - eye * 0.5).round() + eye, eye_top + eye, ground);
            }
            let (mouth_h, mouth_w) = ((side * 0.08).round().max(2.0), (head_w * 0.46).round());
            let mouth_top = (hy0 + head_h * 0.66).round();
            put((cx - mouth_w * 0.5).round(), mouth_top, (cx - mouth_w * 0.5).round() + mouth_w, mouth_top + mouth_h, ground);
        }
        Icon::Browser => {
            // a window: a frame, a title band across the top and a line for the address in it
            let (fy0, fy1) = ((y0 + side * 0.08).round(), (y1 - side * 0.08).round());
            put(x0, fy0, x1, fy1, ink);
            put(x0 + t, fy0 + (side * 0.30).round(), x1 - t, fy1 - t, ground);
            let ay = (fy0 + (side * 0.09).round()).max(fy0 + 1.0);
            put(x0 + (side * 0.30).round(), ay, x1 - t - 1.0, ay + (side * 0.10).round().max(1.0), ground);
        }
        Icon::Account => {
            // a head, and under it the top half of a wider disc for the shoulders, with a gap of the button's colour between them
            disc(&mut put, (cx, y0 + side * 0.24), side * 0.22, ink);
            let (rad, base) = (side * 0.46, y1);
            for i in 0..rad.ceil() as usize {
                let y = (base - rad).round() + i as f32;
                let half = (rad * rad - (base - y - 0.5).powi(2)).max(0.0).sqrt();
                put((cx - half).round(), y, (cx + half).round(), y + 1.0, ink);
            }
        }
        Icon::Claude => {
            // a spark: a hub, long rays up, down, left and right, short ones between
            let (mx, my, d) = (cx.round(), cy.round(), side * 0.36);
            put(x0, my - t * 0.5, x1, my - t * 0.5 + t, ink);
            put(mx - t * 0.5, y0, mx - t * 0.5 + t, y1, ink);
            stroke(&mut put, (cx - d, cy - d), (cx + d, cy + d), t, ink);
            stroke(&mut put, (cx - d, cy + d), (cx + d, cy - d), t, ink);
            disc(&mut put, (cx, cy), side * 0.15, ink);
        }
        Icon::Codex => {
            // `</>`: two chevrons and a slash between them, in a thinner stroke so that the three stay three pieces
            let (tc, a, vh, s, sv) = ((side * 0.10).round().max(2.0), side * 0.20, side * 0.30, side * 0.08, side * 0.40);
            let h = tc * 0.5;
            stroke(&mut put, (x0 + h + a, cy - vh), (x0 + h, cy), tc, ink);
            stroke(&mut put, (x0 + h, cy), (x0 + h + a, cy + vh), tc, ink);
            stroke(&mut put, (x1 - h - a, cy - vh), (x1 - h, cy), tc, ink);
            stroke(&mut put, (x1 - h, cy), (x1 - h - a, cy + vh), tc, ink);
            stroke(&mut put, (cx + s, cy - sv), (cx - s, cy + sv), tc, ink);
        }
        Icon::Local => {
            // a chip: a body with a hollow in it and three pins on each of its four sides
            let inset = (side * 0.20).round();
            let (b0, b1) = ((x0 + inset, y0 + inset), (x1 - inset, y1 - inset));
            put(b0.0, b0.1, b1.0, b1.1, ink);
            put(b0.0 + t, b0.1 + t, b1.0 - t, b1.1 - t, ground);
            let pin = (t * 0.5).round().max(1.0);
            for k in 0..3 {
                let f = (k as f32 + 0.5) / 3.0;
                let (px, py) = ((b0.0 + (b1.0 - b0.0) * f - pin * 0.5).round(), (b0.1 + (b1.1 - b0.1) * f - pin * 0.5).round());
                put(px, y0, px + pin, b0.1, ink);
                put(px, b1.1, px + pin, y1, ink);
                put(x0, py, b0.0, py + pin, ink);
                put(b1.0, py, x1, py + pin, ink);
            }
        }
        Icon::Terminal => {
            // a screen: a body, with the prompt `>_` cut out of it in the button's colour (one piece of ink, as the browser window is)
            let (fy0, fy1) = ((y0 + side * 0.10).round(), (y1 - side * 0.10).round());
            put(x0, fy0, x1, fy1, ink);
            let (mid, tc, ah) = ((fy0 + fy1) * 0.5, (side * 0.10).round().max(2.0), side * 0.17);
            let ax = x0 + side * 0.20;
            stroke(&mut put, (ax, mid - ah), (ax + ah, mid), tc, ground);
            stroke(&mut put, (ax + ah, mid), (ax, mid + ah), tc, ground);
            let uy = (mid + ah - tc * 0.5).round();
            put((x0 + side * 0.50).round(), uy, (x0 + side * 0.80).round(), uy + tc, ground);
        }
        Icon::Agy => {
            // an upward chevron (an apex in the middle, an arm down to each side) with a dot under the apex: the picture is as wide and as tall as the others' box
            let foot_y = y0 + side * 0.62;
            stroke(&mut put, (cx, y0 + t * 0.5), (x0 + t * 0.5, foot_y), t, ink);
            stroke(&mut put, (cx, y0 + t * 0.5), (x1 - t * 0.5, foot_y), t, ink);
            disc(&mut put, (cx, y1 - side * 0.11), side * 0.10, ink);
        }
        Icon::Back | Icon::Forward => {
            // a chevron: two strokes meeting at a tip on the middle row, as wide as it is tall (a `<` or a `>`)
            let (reach, vh) = (side * 0.24, side * 0.44);
            let dir = if icon == Icon::Back { -1.0 } else { 1.0 };
            let (tip, tail) = (cx + dir * reach, cx - dir * reach);
            stroke(&mut put, (tail, cy - vh), (tip, cy), t, ink);
            stroke(&mut put, (tip, cy), (tail, cy + vh), t, ink);
        }
        Icon::Reload => {
            // an arc round the centre from just above the right side, over the top, down the left and under, to just below the right side (the gap is on the right), and an arrowhead at its
            // end pointing up into the gap
            let r = side * 0.40;
            let at = |deg: f32| {
                let a = deg.to_radians();
                (cx + r * a.cos(), cy - r * a.sin())
            };
            let steps = 24;
            for i in 0..steps {
                stroke(&mut put, at(45.0 + 270.0 * i as f32 / steps as f32), at(45.0 + 270.0 * (i + 1) as f32 / steps as f32), t, ink);
            }
            let end = at(315.0);
            let (tip, wing) = ((end.0 + side * 0.04, end.1 - side * 0.34), side * 0.26);
            stroke(&mut put, tip, (end.0 - wing, end.1 - side * 0.04), t, ink);
            stroke(&mut put, tip, (end.0 + wing, end.1 - side * 0.04), t, ink);
        }
        Icon::Key => {
            // a ring with a hole of the button's colour at the left, a shaft to the right and two teeth hanging from the shaft's end
            let (rr, ring) = (side * 0.27, (x0 + side * 0.27, cy));
            disc(&mut put, ring, rr, ink);
            disc(&mut put, ring, side * 0.11, ground);
            put(ring.0 + rr * 0.7, cy - t * 0.5, x1, cy - t * 0.5 + t, ink);
            put(x1 - t, cy, x1, cy + side * 0.24, ink);
            put(x1 - side * 0.26 - t, cy, x1 - side * 0.26, cy + side * 0.17, ink);
        }
        Icon::Pencil => {
            // a slanted body from the top right, a point at the bottom left and a line under the point to write on
            let w = side * 0.24;
            stroke(&mut put, (x1 - side * 0.12, y0 + side * 0.12), (x0 + side * 0.36, y1 - side * 0.36), w, ink);
            polygon(&mut put, &[(x0 + side * 0.22, y1 - side * 0.44), (x0 + side * 0.44, y1 - side * 0.22), (x0, y1 - t - 1.0)], ink);
            put(x0 + side * 0.34, y1 - t * 0.8, x1, y1, ink);
        }
        Icon::Star => {
            let (outer, c) = (side * 0.52, (cx, cy + side * 0.05));
            let inner = outer * 0.42;
            let pts: Vec<(f32, f32)> = (0..10)
                .map(|i| {
                    let (a, r) = ((-90.0 + 36.0 * i as f32).to_radians(), if i % 2 == 0 { outer } else { inner });
                    (c.0 + r * a.cos(), c.1 + r * a.sin())
                })
                .collect();
            polygon(&mut put, &pts, ink);
        }
        Icon::Trash | Icon::TrashX => {
            // a lid with a handle over a bin that narrows towards the bottom; the bin has two grooves, or a cross when it also ends the sessions
            let lid_y = y0 + side * 0.16;
            put(x0 + side * 0.04, lid_y, x1 - side * 0.04, lid_y + t, ink);
            put(cx - side * 0.15, y0, cx + side * 0.15, lid_y, ink);
            put(cx - side * 0.15 + t, y0 + t, cx + side * 0.15 - t, lid_y, ground);
            let top = lid_y + t + 1.0;
            polygon(&mut put, &[(x0 + side * 0.12, top), (x1 - side * 0.12, top), (x1 - side * 0.20, y1), (x0 + side * 0.20, y1)], ink);
            let (a, b) = (top + side * 0.14, y1 - side * 0.12);
            if icon == Icon::TrashX {
                stroke(&mut put, (cx - side * 0.15, a), (cx + side * 0.15, b), t * 0.8, ground);
                stroke(&mut put, (cx + side * 0.15, a), (cx - side * 0.15, b), t * 0.8, ground);
            } else {
                put(cx - side * 0.15 - 1.0, a, cx - side * 0.15 + 1.0, b, ground);
                put(cx + side * 0.15 - 1.0, a, cx + side * 0.15 + 1.0, b, ground);
            }
        }
        Icon::Moon => {
            // a crescent: a disc of ink with a second disc of the button's colour taken out of its upper right (the tips are at the top and at the bottom; the whole is nudged right so that it is centred)
            let (c, r1) = ((cx + side * 0.035, cy), side * 0.46);
            disc(&mut put, c, r1, ink);
            disc(&mut put, (c.0 + side * 0.26, c.1 - side * 0.14), side * 0.40, ground);
        }
        Icon::Sun => {
            // a disc and eight rays round it, thinner than the stroke of the others so that the nine pieces stay nine
            let tc = (side * 0.09).round().max(2.0);
            disc(&mut put, (cx, cy), side * 0.20, ink);
            for k in 0..8 {
                let a = (k as f32 * 45.0).to_radians();
                let at = |d: f32| (cx + d * a.cos(), cy - d * a.sin());
                stroke(&mut put, at(side * 0.32), at(side * 0.5 - tc * 0.5), tc, ink);
            }
        }
        Icon::Auto => {
            // a disc whose left half is solid and whose right half is a ring: the dark and the light of the desktop in one picture
            let (rad, ring) = (side * 0.46, (side * 0.16).round().max(2.0));
            let inner = rad - ring;
            for i in 0..(2.0 * rad).ceil() as usize {
                let y = (cy - rad).round() + i as f32;
                let dy = y + 0.5 - cy;
                let outer = (rad * rad - dy * dy).max(0.0).sqrt();
                put((cx - outer).round(), y, cx.round(), y + 1.0, ink);
                if dy.abs() < inner {
                    put((cx + (inner * inner - dy * dy).sqrt()).round(), y, (cx + outer).round(), y + 1.0, ink);
                } else {
                    put(cx.round(), y, (cx + outer).round(), y + 1.0, ink);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: u32 = 0xFF00_0000;
    const INK: u32 = 0xFFFF_FFFF;
    const BTN: u32 = 0xFF23_2E3E;

    /// The icon in a button 24 pixels smaller than a square canvas of `n`: the canvas, and the button's rectangle.
    fn canvas_n(icon: Icon, n: usize) -> (Vec<u32>, Rect) {
        let mut buf = vec![BG; n * n];
        let r = [12.0, 12.0, (n - 12) as f32, (n - 12) as f32];
        fill(&mut buf, n, n, r, BTN);
        draw(&mut buf, n, n, icon, r, INK, BTN);
        (buf, r)
    }

    /// The icon in a 40x40 button inside a 64x64 canvas.
    fn canvas(icon: Icon) -> (Vec<u32>, Rect) {
        canvas_n(icon, 64)
    }

    fn inked_n(buf: &[u32], n: usize) -> Vec<(usize, usize)> {
        (0..n).flat_map(|y| (0..n).map(move |x| (x, y))).filter(|&(x, y)| buf[y * n + x] == INK).collect()
    }

    fn inked(buf: &[u32]) -> Vec<(usize, usize)> {
        inked_n(buf, 64)
    }

    /// How many separate pieces of ink the icon is (4-connected).
    fn pieces(buf: &[u32], n: usize) -> usize {
        let mut seen = vec![false; n * n];
        let mut count = 0;
        for start in 0..n * n {
            if buf[start] != INK || seen[start] {
                continue;
            }
            count += 1;
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(i) = stack.pop() {
                let (x, y) = (i % n, i / n);
                for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                    if nx < n && ny < n && buf[ny * n + nx] == INK && !seen[ny * n + nx] {
                        seen[ny * n + nx] = true;
                        stack.push(ny * n + nx);
                    }
                }
            }
        }
        count
    }

    #[test]
    fn every_icon_inks_pixels_inside_its_button_and_never_outside_it() {
        for icon in Icon::ALL {
            let (buf, r) = canvas(icon);
            let px = inked(&buf);
            assert!(px.len() > 60, "{icon:?} draws something: {}", px.len());
            assert!(px.iter().all(|&(x, y)| (x as f32) >= r[0] && (x as f32) < r[2] && (y as f32) >= r[1] && (y as f32) < r[3]), "{icon:?} stays inside the button");
            assert!(buf[..64 * 12].iter().all(|&c| c == BG), "{icon:?}: nothing above the button");
        }
    }

    #[test]
    fn the_icons_are_all_different_pictures_and_have_different_names() {
        let pictures: Vec<Vec<u32>> = Icon::ALL.iter().map(|&i| canvas(i).0).collect();
        for a in 0..pictures.len() {
            for b in a + 1..pictures.len() {
                assert!(pictures[a] != pictures[b], "{:?} and {:?} are one picture", Icon::ALL[a], Icon::ALL[b]);
            }
        }
        let mut names: Vec<&str> = Icon::ALL.iter().map(|i| i.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Icon::ALL.len());
    }

    #[test]
    fn the_plus_and_the_bot_are_centred_and_the_plus_is_a_cross() {
        let (plus, _) = canvas(Icon::Plus);
        let px = inked(&plus);
        let (min_x, max_x) = (px.iter().map(|p| p.0).min().unwrap(), px.iter().map(|p| p.0).max().unwrap());
        let (min_y, max_y) = (px.iter().map(|p| p.1).min().unwrap(), px.iter().map(|p| p.1).max().unwrap());
        assert!((min_x + max_x).abs_diff(64) <= 2 && (min_y + max_y).abs_diff(64) <= 2, "centred at 32,32: x {min_x}..{max_x}, y {min_y}..{max_y}");
        assert!(px.contains(&(32, 22)) && px.contains(&(32, 41)) && px.contains(&(22, 32)) && px.contains(&(41, 32)), "four arms");
        assert!(!px.contains(&(24, 24)) && !px.contains(&(40, 40)), "and a cross, not a square");
        let (bot, _) = canvas(Icon::Bot);
        let bp = inked(&bot);
        let (bmin, bmax) = (bp.iter().map(|p| p.0).min().unwrap(), bp.iter().map(|p| p.0).max().unwrap());
        assert!((bmin + bmax).abs_diff(64) <= 2, "the bot is centred: {bmin}..{bmax}");
    }

    /// The pieces of the button's colour that the ink shuts in (4-connected): what is cut out of a picture, as the eyes of a face.
    fn holes(buf: &[u32], n: usize) -> usize {
        let mut seen = vec![false; n * n];
        let flood = |start: usize, seen: &mut Vec<bool>| {
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(i) = stack.pop() {
                let (x, y) = (i % n, i / n);
                for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                    if nx < n && ny < n && buf[ny * n + nx] != INK && !seen[ny * n + nx] {
                        seen[ny * n + nx] = true;
                        stack.push(ny * n + nx);
                    }
                }
            }
        };
        flood(0, &mut seen); // the outside: the corner of the canvas
        let mut count = 0;
        for start in 0..n * n {
            if buf[start] != INK && !seen[start] {
                count += 1;
                flood(start, &mut seen);
            }
        }
        count
    }

    #[test]
    fn the_bot_is_one_head_with_an_antenna_two_eyes_and_a_mouth_cut_out_of_it() {
        // the owner's words of 2026-11-26: a robot, not a house; one piece of ink, and the three things cut out of it in the button's colour
        let (buf, _) = canvas_n(Icon::Bot, 128);
        assert_eq!(pieces(&buf, 128), 1, "the antenna, the ears and the head are one piece");
        assert_eq!(holes(&buf, 128), 3, "two eyes and a mouth");
        let rows: Vec<usize> = (0..128).filter(|&y| (0..128).any(|x| buf[y * 128 + x] == INK)).collect();
        let width = |y: usize| (0..128).filter(|&x| buf[y * 128 + x] == INK).count();
        let (top, widest) = (width(rows[0]), rows.iter().map(|&y| width(y)).max().unwrap());
        assert!(top * 3 < widest, "the antenna is a narrow tip over a wide head: {top} against {widest}");
        assert!(widest >= width(*rows.last().unwrap()) + 8, "an ear on each side sticks out of the head: {widest} against {}", width(*rows.last().unwrap()));
        let inked: Vec<(usize, usize)> = inked_n(&buf, 128);
        let (min_y, max_y) = (inked.iter().map(|p| p.1).min().unwrap(), inked.iter().map(|p| p.1).max().unwrap());
        assert!(max_y - min_y + 1 >= 56, "it fills the size of the other pictures: {min_y}..{max_y}");
    }

    #[test]
    fn every_icon_is_centred_in_its_button() {
        for icon in Icon::ALL {
            let (buf, _) = canvas(icon);
            let px = inked(&buf);
            let (min_x, max_x) = (px.iter().map(|p| p.0).min().unwrap(), px.iter().map(|p| p.0).max().unwrap());
            let (min_y, max_y) = (px.iter().map(|p| p.1).min().unwrap(), px.iter().map(|p| p.1).max().unwrap());
            assert!((min_x + max_x).abs_diff(63) <= 3, "{icon:?} is centred across: {min_x}..{max_x}");
            assert!((min_y + max_y).abs_diff(63) <= 4, "{icon:?} is centred down: {min_y}..{max_y}");
        }
    }

    #[test]
    fn the_person_is_a_head_over_wider_shoulders_with_a_gap_between() {
        let (buf, _) = canvas_n(Icon::Account, 96);
        let rows: Vec<usize> = (0..96).filter(|&y| (0..96).any(|x| buf[y * 96 + x] == INK)).collect();
        let (top, bottom) = (rows[0], *rows.last().unwrap());
        let gap = (top..=bottom).find(|y| !rows.contains(y)).expect("a row of the button's colour between the head and the shoulders");
        let width = |ys: std::ops::Range<usize>| ys.map(|y| (0..96).filter(|&x| buf[y * 96 + x] == INK).count()).max().unwrap();
        assert!(width(top..gap) < width(gap..bottom + 1), "the head ({}) is narrower than the shoulders ({})", width(top..gap), width(gap..bottom + 1));
        assert_eq!(pieces(&buf, 96), 2, "a head and shoulders, two pieces");
    }

    #[test]
    fn the_pictures_have_the_pieces_they_are_made_of() {
        // at a size the pieces can be told apart: the code brackets are a `<`, a slash and a `>`
        let want = [(Icon::Plus, 1), (Icon::Bot, 1), (Icon::Browser, 1), (Icon::Account, 2), (Icon::Claude, 1), (Icon::Codex, 3), (Icon::Local, 1), (Icon::Terminal, 1), (Icon::Agy, 2), (Icon::Back, 1), (Icon::Forward, 1), (Icon::Reload, 1), (Icon::Moon, 1), (Icon::Sun, 9), (Icon::Auto, 1)];
        for (icon, n) in want {
            let (buf, _) = canvas_n(icon, 128);
            assert_eq!(pieces(&buf, 128), n, "{icon:?}");
        }
    }

    #[test]
    fn the_chip_has_pins_on_all_four_sides_and_the_spark_reaches_all_four_edges() {
        // the picture's box is the full side (58 of a 104 button) only when the pins (or the rays) reach out to it
        for icon in [Icon::Local, Icon::Claude] {
            let (buf, _) = canvas_n(icon, 128);
            let px = inked_n(&buf, 128);
            let (min_x, max_x) = (px.iter().map(|p| p.0).min().unwrap(), px.iter().map(|p| p.0).max().unwrap());
            let (min_y, max_y) = (px.iter().map(|p| p.1).min().unwrap(), px.iter().map(|p| p.1).max().unwrap());
            assert!(max_x - min_x + 1 >= 56 && max_y - min_y + 1 >= 56, "{icon:?} reaches out: x {min_x}..{max_x}, y {min_y}..{max_y}");
        }
    }

    #[test]
    fn the_three_programs_a_terminal_can_name_have_their_pictures_and_no_other_word_has_one() {
        assert_eq!([Icon::of_agent("claude"), Icon::of_agent("codex"), Icon::of_agent("agy")], [Some(Icon::Claude), Some(Icon::Codex), Some(Icon::Agy)]);
        for other in ["", "vim", "Claude", "local", "terminal"] {
            assert_eq!(Icon::of_agent(other), None, "{other}");
        }
    }

    #[test]
    fn the_agy_picture_is_an_upward_chevron_over_a_dot_that_fills_the_box() {
        let (buf, _) = canvas_n(Icon::Agy, 128);
        let px = inked_n(&buf, 128);
        let (min_x, max_x) = (px.iter().map(|p| p.0).min().unwrap(), px.iter().map(|p| p.0).max().unwrap());
        let (min_y, max_y) = (px.iter().map(|p| p.1).min().unwrap(), px.iter().map(|p| p.1).max().unwrap());
        assert!(max_x - min_x + 1 >= 56 && max_y - min_y + 1 >= 56, "it reaches out like the others: x {min_x}..{max_x}, y {min_y}..{max_y}");
        let row = |y: usize| (0..128).filter(|&x| buf[y * 128 + x] == INK).collect::<Vec<usize>>();
        // the apex is one narrow place at the top and in the middle; the feet are far apart, lower; the dot is the last rows, in the middle
        let top = row(min_y);
        assert!(!top.is_empty() && top.iter().all(|&x| (50..78).contains(&x)), "the apex is in the middle: {top:?}");
        let feet = row(min_y + (max_y - min_y) * 55 / 100);
        assert!(feet.iter().min().copied().unwrap() < 50 && feet.iter().max().copied().unwrap() > 78, "the two arms are apart lower down: {feet:?}");
        let last = row(max_y);
        assert!(!last.is_empty() && last.iter().all(|&x| (50..78).contains(&x)), "the dot is in the middle: {last:?}");
        // a clear row between the chevron and the dot (two pieces)
        assert!((min_y..=max_y).any(|y| row(y).is_empty()), "a gap between the chevron and the dot");
    }

    #[test]
    fn back_and_forward_point_opposite_ways_and_reload_leaves_its_gap_on_the_right() {
        let tip_rows = |icon: Icon, left: bool| {
            let (buf, _) = canvas_n(icon, 128);
            let px = inked_n(&buf, 128);
            let edge = if left { px.iter().map(|p| p.0).min().unwrap() } else { px.iter().map(|p| p.0).max().unwrap() };
            let ys: Vec<usize> = px.iter().filter(|p| p.0 <= edge + 1 && left || p.0 + 1 >= edge && !left).map(|p| p.1).collect();
            (ys.iter().sum::<usize>() as f32 / ys.len() as f32, ys.iter().min().copied().unwrap(), ys.iter().max().copied().unwrap())
        };
        let (mid_back, lo_b, hi_b) = tip_rows(Icon::Back, true);
        let (mid_fwd, lo_f, hi_f) = tip_rows(Icon::Forward, false);
        assert!((mid_back - 63.5).abs() <= 3.0 && (mid_fwd - 63.5).abs() <= 3.0, "the tip is on the middle row: {mid_back} {mid_fwd}");
        assert!(hi_b - lo_b <= 12 && hi_f - lo_f <= 12, "the tip is a point, not the whole height: {lo_b}..{hi_b} {lo_f}..{hi_f}");
        // the chevron `<` has its open side to the right: ink at the left of the middle row, none at the right end of it
        let (back, _) = canvas_n(Icon::Back, 128);
        let row = |buf: &[u32], y: usize| (0..128).filter(|&x| buf[y * 128 + x] == INK).collect::<Vec<usize>>();
        let mid = row(&back, 63);
        assert!(!mid.is_empty() && mid.iter().all(|&x| x < 64), "the tip of `<` is left of the middle: {mid:?}");
        let (fwd, _) = canvas_n(Icon::Forward, 128);
        assert!(row(&fwd, 63).iter().all(|&x| x >= 64), "the tip of `>` is right of the middle");
        // the reload arc: ink on the left, the top and the bottom of the circle, a gap on the right above the arrowhead (the circle there, 30 degrees up, is not drawn)
        let (reload, _) = canvas_n(Icon::Reload, 128);
        let high = row(&reload, 52);
        assert!(!high.is_empty() && high.iter().all(|&x| x < 64), "the gap is on the right: {high:?}");
        assert!(row(&reload, 41).iter().any(|&x| (56..72).contains(&x)), "the arc passes over the top");
        assert!(pieces(&reload, 128) == 1, "the arc and its arrowhead are one piece");
    }

    #[test]
    fn the_moon_is_a_crescent_the_sun_a_disc_with_eight_rays_and_the_auto_picture_a_half_solid_disc() {
        let row = |buf: &[u32], y: usize| (0..128).filter(|&x| buf[y * 128 + x] == INK).collect::<Vec<usize>>();
        let col = |buf: &[u32], x: usize| (0..128).filter(|&y| buf[y * 128 + x] == INK).collect::<Vec<usize>>();
        // the moon: its tips are at the top and the bottom, its belly is on the left, and the middle row is a bar at the left only (the right is cut away)
        let (moon, _) = canvas_n(Icon::Moon, 128);
        let mid = row(&moon, 64);
        assert!(!mid.is_empty() && mid.iter().all(|&x| x < 64), "the crescent's belly is at the left of the middle row: {mid:?}");
        let px = inked_n(&moon, 128);
        let (min_y, max_y) = (px.iter().map(|p| p.1).min().unwrap(), px.iter().map(|p| p.1).max().unwrap());
        assert!(max_y - min_y + 1 >= 52, "the tips reach the top and the bottom of the box: {min_y}..{max_y}");
        assert!(row(&moon, min_y + 1).iter().all(|&x| x > 64 - 16) && !row(&moon, min_y + 1).is_empty(), "the upper tip is near the middle");
        let (min_x, max_x) = (px.iter().map(|p| p.0).min().unwrap(), px.iter().map(|p| p.0).max().unwrap());
        assert!((min_x + max_x).abs_diff(128) <= 4, "the crescent as a whole is in the middle of the box, not hung on the left: {min_x}..{max_x}");
        // the sun: a disc in the middle, a ray up, down, left and right and one in each corner, and clear ground between the disc and the rays
        let (sun, _) = canvas_n(Icon::Sun, 128);
        assert!(sun[64 * 128 + 64] == INK, "the disc");
        for (x, y) in [(64usize, 36usize), (64, 92), (36, 64), (92, 64), (44, 44), (84, 44), (44, 84), (84, 84)] {
            assert!(sun[y * 128 + x] == INK, "a ray passes {x},{y}");
        }
        assert!(sun[64 * 128 + 78] == BTN && sun[49 * 128 + 64] == BTN, "the gap between the disc and a ray is the colour of the button");
        let rays = inked_n(&sun, 128);
        let (lo, hi) = (rays.iter().map(|p| p.0.min(p.1)).min().unwrap(), rays.iter().map(|p| p.0.max(p.1)).max().unwrap());
        assert!(lo >= 12 && hi < 128 - 12, "no ray leaves the box of the icon: {lo}..{hi}");
        assert!((33..=37).contains(&lo) && (91..=95).contains(&hi), "the rays end where they are drawn to end, short of the box's edge: {lo}..{hi}");
        // the auto picture: a disc that is solid on the left of its middle line and hollow (a ring) on the right
        let (auto, _) = canvas_n(Icon::Auto, 128);
        assert!(auto[64 * 128 + 50] == INK && auto[64 * 128 + 62] == INK, "the left half is solid");
        assert!(auto[64 * 128 + 80] == BTN, "the right half is hollow");
        let right = row(&auto, 64).into_iter().filter(|&x| x >= 64).collect::<Vec<_>>();
        assert!(!right.is_empty() && right.len() < 14, "the ring on the right is thin: {right:?}");
        assert!(col(&auto, 80).len() >= 2 && col(&auto, 80).iter().any(|&y| y < 50) && col(&auto, 80).iter().any(|&y| y > 78), "the ring closes at the top and the bottom");
    }

    #[test]
    fn a_rectangle_too_small_to_hold_an_icon_gets_none() {
        for icon in Icon::ALL {
            let mut buf = vec![BG; 64 * 64];
            draw(&mut buf, 64, 64, icon, [0.0, 0.0, 6.0, 6.0], INK, BTN);
            assert!(buf.iter().all(|&c| c == BG), "{icon:?}");
        }
    }
}
