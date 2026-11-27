//! The sign-in as pictures (`M25-P13`). The owner signed an agy account in and asked for the steps to be shown as a screen, and for the place where the code is pasted to be a box that one pastes into;
//! the same screen is the sign-in of Claude and of Codex. Three (two for Codex) pictures of the same size as the wizard's say what there is to do (open the page, sign in there, paste the code): the ones
//! done have a tick, the one to do now has the ring, a sign-in that ended without success shows a cross and a picture to try again. Under them the address with two buttons (open it, copy it), and a box
//! with a button to paste into it and one to send it. The text lines stay the model (the dump, the keys `Ctrl+C`, `Ctrl+O`, `D` and the words the tests read): a veil of the window's own ground covers
//! them and the pictures are painted over it, as the account page's are (`account_cards.rs`). The window reads the sign-in as the host reports it and asks the host nothing new; what is typed or pasted is
//! drawn as stars and only its length is ever put in the dump.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};

use super::super::super::controls::{self, Action};
use super::super::super::panel::{auto_open_off, Panel, Step};
use super::super::super::{App, BACKGROUND};
use super::icons::{self, Icon};
use super::picker::{draw_tiles, tile_rects, Look, FILL, GREY_FILL, GREY_INK, INK, LIT, RING};

/// How many lines the body of the sign-in keeps (the text first, blank lines after it): the pictures are painted over all of them.
pub(crate) const LOGIN_LINES: usize = 11;

const DIM: u32 = 0xFF8A_94A0;
const OK: u32 = 0xFF68_E0A8;
const BAD: u32 = 0xFFE0_5A5A;
const FIELD: u32 = 0xFF13_1922;
const EDGE: u32 = 0xFF3A_4658;
const BADGE_INK: u32 = 0xFF10_1820;

/// Where one step of the sign-in is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    Todo,
    Active,
    Done,
    Failed,
}

impl Progress {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Progress::Todo => "todo",
            Progress::Active => "active",
            Progress::Done => "done",
            Progress::Failed => "failed",
        }
    }
}

/// One picture of the row of steps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LoginStep {
    pub(crate) icon: Icon,
    pub(crate) word: &'static str,
    pub(crate) progress: Progress,
}

/// The box the code is pasted into: whether it takes input now, and how many stars it shows (the code itself is never here).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LoginField {
    pub(crate) active: bool,
    pub(crate) stars: usize,
}

/// What the sign-in screen shows now, from what the host says about the sign-in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LoginView {
    pub(crate) steps: Vec<LoginStep>,
    /// The address the program printed; none until it does (and none when the sign-in is over).
    pub(crate) address: Option<String>,
    /// The box for the code: only for a provider that asks for one, and only while the sign-in runs.
    pub(crate) field: Option<LoginField>,
    /// The sign-in ended without success: a picture to try again takes the place of the address and the box.
    pub(crate) retry: bool,
    /// Codex has a second way (a code of the device) on the key `D`.
    pub(crate) device: bool,
    /// What went wrong, in a sentence, when the sign-in ended without success.
    pub(crate) note: Option<&'static str>,
}

/// The steps and the parts of the screen for a sign-in in the state `state` (the host's word: `waiting`, `browser` with the address known, `code` when the program asks for the code, `succeeded`,
/// `failed`, `timed_out`, `cancelled`). A provider that finishes by itself once the page is done (Codex) has two steps; the others a third, the code; the code step is also there for any provider
/// that has asked for one. After the code was sent the program is still checking it (`waiting` with the address known): every step is done.
pub(crate) fn login_view(provider: &str, state: &str, url: Option<&str>, needs_code: bool, stars: usize) -> LoginView {
    let ended = matches!(state, "failed" | "timed_out" | "cancelled");
    let over = ended || state == "succeeded";
    let uses_code = provider != "codex" || needs_code;
    let mut kinds = vec![(Icon::Browser, "Open"), (Icon::Key, "Sign in")];
    if uses_code {
        kinds.push((Icon::Paste, "Code"));
    }
    let done = match state {
        "succeeded" => kinds.len(),
        _ if ended => 0,
        "code" => 2,
        "browser" => 1,
        _ if url.is_some() => kinds.len(),
        _ => 0,
    };
    let steps = kinds
        .iter()
        .enumerate()
        .map(|(i, &(icon, word))| {
            let progress = match (ended, i) {
                (true, 0) => Progress::Failed,
                (true, _) => Progress::Todo,
                _ if i < done => Progress::Done,
                _ if i == done => Progress::Active,
                _ => Progress::Todo,
            };
            LoginStep { icon, word, progress }
        })
        .collect();
    LoginView {
        steps,
        address: url.filter(|_| !over).map(str::to_string),
        field: (uses_code && !over).then_some(LoginField { active: needs_code, stars: if needs_code { stars } else { 0 } }),
        retry: ended,
        device: provider == "codex" && !over,
        note: match state {
            "failed" => Some("The sign-in did not work. Press Again to try once more."),
            "timed_out" => Some("The sign-in took too long. Press Again to try once more."),
            "cancelled" => Some("The sign-in was stopped. Press Again to start it."),
            _ => None,
        },
    }
}

/// Where the parts of the screen are, inside the band the veil covers: the pictures in a row, the address with its two buttons under them, the box with its two buttons under that (the buttons
/// of the two rows are in the same two columns).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layout {
    pub(crate) tiles: Vec<[f32; 4]>,
    pub(crate) address: Option<[f32; 4]>,
    pub(crate) open: Option<[f32; 4]>,
    pub(crate) copy: Option<[f32; 4]>,
    pub(crate) field: Option<[f32; 4]>,
    pub(crate) paste: Option<[f32; 4]>,
    pub(crate) send: Option<[f32; 4]>,
    /// The row of a sentence: the second way (Codex) where the box would be, or what went wrong under the pictures of a sign-in that ended.
    pub(crate) hint: Option<[f32; 4]>,
}

/// The layout of `view` in `band` (left, top, right, bottom in pixels) with lines `line_h` tall and a character `char_w` wide at scale `s`; none when the band cannot hold it (a window too small: the
/// text stays, nothing is painted over it).
pub(crate) fn layout(band: [f32; 4], s: f32, line_h: f32, char_w: f32, view: &LoginView) -> Option<Layout> {
    let n = view.steps.len() + usize::from(view.retry);
    let tiles = tile_rects([band[0], band[1], band[2], band[1] + 5.0 * line_h], s, n);
    if tiles.is_empty() {
        return None;
    }
    let (row_h, gap, narrow) = ((1.7 * line_h).round(), (0.5 * line_h).round(), (0.25 * line_h).round());
    let (a0, p0) = (band[1] + 5.0 * line_h + gap, band[1] + 5.0 * line_h + gap + row_h + gap);
    let right = (band[2] - 4.0 * s).min(band[0] + 64.0 * char_w);
    let mut out = Layout { tiles, address: None, open: None, copy: None, field: None, paste: None, send: None, hint: None };
    if view.retry {
        out.hint = (a0 + row_h <= band[3]).then_some([band[0], a0, band[2] - 4.0 * s, a0 + row_h]);
        return Some(out);
    }
    if p0 + row_h > band[3] || right - band[0] < 4.0 * row_h {
        return None;
    }
    let copy = |y: f32| [right - row_h, y, right, y + row_h];
    let open = |y: f32| [right - 2.0 * row_h - narrow, y, right - row_h - narrow, y + row_h];
    let wide = |y: f32| [band[0], y, right - 2.0 * row_h - 2.0 * narrow, y + row_h];
    (out.address, out.open, out.copy) = (Some(wide(a0)), Some(open(a0)), Some(copy(a0)));
    if view.field.is_some() {
        (out.field, out.paste, out.send) = (Some(wide(p0)), Some(open(p0)), Some(copy(p0)));
    } else if view.device {
        out.hint = Some([band[0], p0, right, p0 + row_h]);
    }
    Some(out)
}

impl App {
    /// The lines of the sign-in: what the host says about it as text (the model that the keys and the dump read), then blank lines (an empty one has no height) up to `LOGIN_LINES`, so that the pictures
    /// have the room that the veil covers.
    pub(crate) fn login_body(&self, p: &Panel) -> (String, Vec<String>) {
        let mut body = Vec::new();
        body.push(format!("state: {}", p.login["state"].as_str().unwrap_or("starting")));
        if let Some(h) = self.ui.ssh_hosts.get(&p.account) {
            body.push(format!("host: ssh:{h} (runs login on {h}; remote connects directly to provider)"));
        }
        if let Some(url) = p.login["url"].as_str() {
            body.push("Open this address and sign in:".into());
            body.push(url.to_string());
            body.push(if auto_open_off() { "Ctrl+C  copy the address    Ctrl+O  open it in the browser" } else { "Opening the browser. Ctrl+O opens it again, Ctrl+C copies the address." }.into());
        }
        if p.login["needs_code"] == json!(true) {
            body.push("The provider asks for a code: paste it below and press Enter.".into());
        } else if p.provider == "codex" && p.login["state"] != json!("succeeded") {
            body.push("D  use the device-code sign-in instead".into());
        }
        while body.len() < LOGIN_LINES {
            body.push(" ".into());
        }
        (format!("Sign in: {}", self.account_name(p)), body)
    }

    /// The screen of the sign-in as it is now (none on any other screen).
    pub(crate) fn login_view_now(&self) -> Option<LoginView> {
        let p = self.ui.panel.as_ref().filter(|p| p.step == Step::Login)?;
        let stars = if self.code_entry() { self.ed.text().chars().count().min(64) } else { 0 };
        Some(login_view(&p.provider, p.login["state"].as_str().unwrap_or("starting"), p.login["url"].as_str(), p.login["needs_code"] == json!(true), stars))
    }

    /// Whether the last frame painted the pictures (then the box is where the code goes, and the line at the bottom of the window says nothing).
    pub(crate) fn login_pictured(&self) -> bool {
        !self.ui.toolbar_geom.login.is_empty()
    }

    /// A click on one of the buttons of the screen: the same as the key that does it (`o` open, `c` copy, `v` paste, `g` send, `r` again).
    pub(crate) fn login_act(&mut self, key: char) {
        match key {
            'o' => self.login_address(false),
            'c' => self.login_address(true),
            'v' => self.start_paste(b"CLIPBOARD"),
            'g' => {
                let text = self.ed.take().text();
                self.preedit.clear();
                self.submit_text(Step::Login, text.trim());
            }
            'r' => self.start_login("browser"),
            _ => {}
        }
        self.dirty = true;
    }

    /// The pictures over the veil: the row of steps, the address with its buttons, the box with its buttons. Each button is a target of its own. Nothing is painted (and the text shows) when the band does
    /// not fit on the screen.
    pub(super) fn login_paint(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, s: f32) {
        let before = self.login_pictured();
        self.ui.toolbar_geom.login.clear();
        self.login_draw(buf, width, height, list_w, s);
        if before != self.login_pictured() {
            self.dirty = true; // the line at the bottom changes with it
        }
    }

    fn login_draw(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, s: f32) {
        let Some(view) = self.login_view_now() else { return };
        let (Some(first), Some(last)) = (self.line_range(0), self.line_range(LOGIN_LINES - 1)) else { return };
        let (line_h, char_w) = ((last.1 - first.0) / LOGIN_LINES as f32, self.char_w() * s);
        let band = [list_w as f32 + 4.0 * s, first.0, width as f32 - 4.0 * s, last.1];
        let Some(lay) = layout(band, s, line_h, char_w, &view) else { return };
        controls::fill(buf, width, height, [list_w as f32 + 1.0, first.0, width as f32, last.1], BACKGROUND); // the veil over the text
        let mouse = (self.ui.mouse.0 as f32, self.ui.mouse.1 as f32);
        let over = |r: &[f32; 4]| mouse.0 >= r[0] && mouse.0 < r[2] && mouse.1 >= r[1] && mouse.1 < r[3];
        let have_address = view.address.is_some();
        let tile_act = |i: usize| match (i, view.steps.get(i)) {
            (0, Some(_)) if have_address => Some('o'),
            (_, Some(_)) => None,
            (_, None) => Some('r'), // the picture after the steps: try again
        };
        let looks: Vec<Look> = lay
            .tiles
            .iter()
            .enumerate()
            .map(|(i, r)| match view.steps.get(i) {
                Some(st) => Look { icon: st.icon, enabled: st.progress != Progress::Todo, focused: st.progress == Progress::Active, hovered: tile_act(i).is_some() && over(r) },
                None => Look { icon: Icon::Reload, enabled: true, focused: true, hovered: over(r) },
            })
            .collect();
        draw_tiles(buf, width, height, s, &lay.tiles, &looks);
        for (st, r) in view.steps.iter().zip(&lay.tiles) {
            match st.progress {
                Progress::Done => badge(buf, width, height, *r, Icon::Check, OK, s),
                Progress::Failed => badge(buf, width, height, *r, Icon::Cross, BAD, s),
                _ => {}
            }
        }
        let b = s.round().max(1.0);
        let ring = (2.0 * s).round().max(2.0);
        let field_look = view.field.as_ref();
        let active = field_look.is_some_and(|f| f.active);
        let stars = field_look.map_or(0, |f| f.stars);
        let rows = [(lay.address, lay.open, lay.copy, false), (lay.field, lay.paste, lay.send, true)];
        for (box_r, first_r, second_r, is_field) in rows {
            let Some(box_r) = box_r else { continue };
            let lit = is_field && active;
            if lit {
                controls::fill(buf, width, height, [box_r[0] - ring, box_r[1] - ring, box_r[2] + ring, box_r[3] + ring], RING);
            }
            controls::fill(buf, width, height, box_r, EDGE);
            controls::fill(buf, width, height, [box_r[0] + b, box_r[1] + b, box_r[2] - b, box_r[3] - b], FIELD);
            for (r, icon, enabled) in [(first_r, if is_field { Icon::Paste } else { Icon::Browser }, if is_field { active } else { have_address }), (second_r, if is_field { Icon::Check } else { Icon::Copy }, if is_field { active && stars > 0 } else { have_address })] {
                let Some(r) = r else { continue };
                let (fill, ink) = match (enabled, enabled && over(&r)) {
                    (false, _) => (GREY_FILL, GREY_INK),
                    (true, true) => (LIT, INK),
                    (true, false) => (FILL, INK),
                };
                controls::fill(buf, width, height, r, fill);
                icons::draw(buf, width, height, icon, r, ink, fill);
            }
        }
        let text_of = |is_field: bool| -> (String, u32) {
            match (is_field, &view.address, active, stars) {
                (false, Some(url), _, _) => (url.clone(), INK),
                (false, None, _, _) => ("Getting the address...".into(), DIM),
                (true, _, true, 0) => ("Paste the code here".into(), DIM),
                (true, _, true, n) => ("*".repeat(n), INK),
                (true, _, false, _) => ("The code goes here".into(), GREY_INK),
            }
        };
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let label_h = (controls.font_px + 4.0) * controls.scale;
        let pad = (8.0 * s).round();
        for (st, r) in view.steps.iter().zip(&lay.tiles) {
            let w = controls.width(fs, st.word);
            let x = (r[0] + (r[2] - r[0] - w) * 0.5).max(r[0] + char_w * 0.25);
            controls.text(fs, swash, buf, width, height, x, r[3] - label_h - 2.0 * s, r[2] as i32, st.word, Color(if st.progress == Progress::Todo { GREY_INK } else { DIM }));
        }
        if view.retry {
            if let Some(r) = lay.tiles.get(view.steps.len()) {
                let w = controls.width(fs, "Again");
                controls.text(fs, swash, buf, width, height, r[0] + (r[2] - r[0] - w) * 0.5, r[3] - label_h - 2.0 * s, r[2] as i32, "Again", Color(INK));
            }
        }
        let mut caret = None;
        for (box_r, is_field) in [(lay.address, false), (lay.field, true)] {
            let Some(r) = box_r else { continue };
            let (text, ink) = text_of(is_field);
            let y = r[1] + (r[3] - r[1] - label_h) * 0.5;
            let w = controls.text(fs, swash, buf, width, height, r[0] + pad, y, (r[2] - pad) as i32, &text, Color(ink));
            if is_field && active {
                caret = Some([r[0] + pad + if stars > 0 { w } else { 0.0 }, y, r[0] + pad + if stars > 0 { w } else { 0.0 } + (2.0 * s).max(2.0), y + label_h]);
            }
        }
        if let Some(c) = caret {
            controls::fill(buf, width, height, c, INK);
        }
        if let Some(r) = lay.hint {
            let (words, ink) = view.note.map_or(("Another way: press D for a device code", DIM), |n| (n, BAD));
            controls.text(fs, swash, buf, width, height, r[0] + pad, r[1] + (r[3] - r[1] - label_h) * 0.5, r[2] as i32, words, Color(ink));
        }
        let mut targets: Vec<(&'static str, [f32; 4])> = Vec::new();
        for (name, rect, key) in [("open", lay.open, 'o'), ("copy", lay.copy, 'c'), ("paste", lay.paste, 'v'), ("send", lay.send, 'g')] {
            if let Some(r) = rect {
                targets.push((name, r));
                controls.target(r, Action::LoginAct(key));
            }
        }
        for (i, r) in lay.tiles.iter().enumerate() {
            if let Some(key) = tile_act(i) {
                controls.target(*r, Action::LoginAct(key));
            }
        }
        for (name, rect) in [("address", lay.address), ("field", lay.field)] {
            if let Some(r) = rect {
                targets.push((name, r));
            }
        }
        if view.retry {
            if let Some(r) = lay.tiles.get(view.steps.len()) {
                targets.push(("retry", *r));
            }
        }
        if targets.is_empty() {
            targets.push(("tiles", lay.tiles[0])); // the row alone (a sign-in with nothing else to show) is still a picture
        }
        self.ui.toolbar_geom.tiles = lay.tiles;
        self.ui.toolbar_geom.login = targets;
    }

    /// The sign-in screen for the window dump: the steps with the rectangles they were painted at, the address, the box (its length, never its text) and the buttons.
    pub(crate) fn login_json(&self) -> Value {
        let Some(view) = self.login_view_now() else { return Value::Null };
        let g = &self.ui.toolbar_geom;
        let rect = |name: &str| g.login.iter().find(|(n, _)| *n == name).map(|(_, r)| json!(r));
        let steps: Vec<_> = view
            .steps
            .iter()
            .enumerate()
            .map(|(i, st)| json!({"icon": st.icon.name(), "word": st.word, "progress": st.progress.name(), "rect": g.tiles.get(i)}))
            .collect();
        json!({
            "steps": steps,
            "address": view.address,
            "field": view.field.as_ref().map(|f| json!({"active": f.active, "stars": f.stars, "rect": rect("field")})),
            "retry": view.retry,
            "note": view.note,
            "retry_rect": rect("retry"),
            "device": view.device,
            "buttons": {"open": rect("open"), "copy": rect("copy"), "paste": rect("paste"), "send": rect("send")},
            "address_rect": rect("address"),
            "pictured": self.login_pictured(),
        })
    }
}

/// The small square on a picture's top right corner: a tick on green, or a cross on red.
fn badge(buf: &mut [u32], width: usize, height: usize, tile: [f32; 4], icon: Icon, colour: u32, s: f32) {
    let side = ((tile[2] - tile[0]) * 0.34).round();
    let (x1, y0) = (tile[2] - 3.0 * s, tile[1] + 3.0 * s);
    let r = [x1 - side, y0, x1, y0 + side];
    controls::fill(buf, width, height, r, colour);
    icons::draw(buf, width, height, icon, r, BADGE_INK, colour);
}

#[cfg(test)]
#[path = "login_cards_tests.rs"]
mod tests;
