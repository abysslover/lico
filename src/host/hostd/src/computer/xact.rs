//! X11/XTEST input injection, input-focus management (`CU-08`), and agent-to-screen coordinate
//! scaling (`CU-03`) for `lico xjob computer` (plan M02-P05, S03).
//!
//! Runs inside the short-lived `xjob` helper process only (`XF-07`); the long-lived host never
//! opens an X11 connection.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, InputFocus, MapState};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::capture::shrink::agent_size;
use crate::capture::{self, Kind, Options, AGENT_MAX_SIDE};

const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;
const BUTTON_PRESS: u8 = 4;
const BUTTON_RELEASE: u8 = 5;
const MOTION_NOTIFY: u8 = 6;
const SETTLE_MS: u64 = 40;
pub const MAX_TYPE_CHARS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClickKind {
    Left,
    Right,
    Middle,
    Double,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScrollDir {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    Look,
    Click { kind: ClickKind, x: i64, y: i64 },
    Move { x: i64, y: i64 },
    Drag { from_x: i64, from_y: i64, x: i64, y: i64 },
    Type { text: String },
    Key { combo: String },
    Scroll { x: i64, y: i64, dir: ScrollDir, amount: u32 },
    Wait { ms: u64 },
}

#[derive(Clone, Debug)]
pub struct JobArgs {
    pub display: Option<String>,
    pub out: PathBuf,
    pub act: Act,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActError {
    pub reason: &'static str,
    pub detail: String,
}

impl ActError {
    pub fn new(reason: &'static str, detail: impl Into<String>) -> ActError {
        ActError { reason, detail: detail.into() }
    }
    pub fn to_json(&self) -> Value {
        json!({"ok": 0, "error": self.reason, "detail": self.detail})
    }
}

/// Convert agent-image coordinates `(ax, ay)` into root-window screen coordinates `(sx, sy)`.
/// Uses exact pixel-centre arithmetic matching `capture::shrink::agent_size`.
pub fn scale_point(ax: i64, ay: i64, sw: u32, sh: u32, max_side: u32) -> Result<(i16, i16), ActError> {
    if sw == 0 || sh == 0 {
        return Err(ActError::new("unsupported_screen", "screen has zero size"));
    }
    let (aw, ah) = agent_size(sw, sh, max_side);
    if ax < 0 || ay < 0 || ax >= aw as i64 || ay >= ah as i64 {
        return Err(ActError::new(
            "out_of_bounds",
            format!("coordinates ({ax}, {ay}) are outside the {aw}x{ah} image"),
        ));
    }
    let map_axis = |a: u64, agent_len: u64, screen_len: u64| -> i16 {
        let s = ((2 * a + 1) * screen_len) / (2 * agent_len);
        s.min(screen_len.saturating_sub(1)).min(i16::MAX as u64) as i16
    };
    Ok((map_axis(ax as u64, aw as u64, sw as u64), map_axis(ay as u64, ah as u64, sh as u64)))
}

fn parse_i64(v: Option<&String>, name: &str) -> Result<i64, ActError> {
    v.and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| ActError::new("usage", format!("{name} must be an integer")))
}

pub fn parse_args(args: &[String]) -> Result<JobArgs, ActError> {
    let usage = |m: &str| ActError::new("usage", m);
    let (mut display, mut out, mut action) = (None, None, None);
    let (mut x, mut y, mut fx, mut fy) = (None, None, None, None);
    let (mut button, mut text, mut key, mut dir, mut amount, mut ms) = (None, None, None, None, None, None);
    let mut i = 0;
    while i < args.len() {
        let Some(v) = args.get(i + 1) else {
            return Err(usage(&format!("{} needs a value", args[i])));
        };
        match args[i].as_str() {
            "--display" => display = Some(v.clone()),
            "--out" => out = Some(PathBuf::from(v)),
            "--action" => action = Some(v.clone()),
            "--x" => x = Some(parse_i64(Some(v), "--x")?),
            "--y" => y = Some(parse_i64(Some(v), "--y")?),
            "--from-x" => fx = Some(parse_i64(Some(v), "--from-x")?),
            "--from-y" => fy = Some(parse_i64(Some(v), "--from-y")?),
            "--button" => button = Some(v.clone()),
            "--text" => text = Some(v.clone()),
            "--key" => key = Some(v.clone()),
            "--direction" => dir = Some(v.clone()),
            "--amount" => amount = Some(parse_i64(Some(v), "--amount")?),
            "--ms" => ms = Some(parse_i64(Some(v), "--ms")?),
            other => return Err(usage(&format!("unknown argument {other}"))),
        }
        i += 2;
    }
    let out = out.filter(|p| !p.as_os_str().is_empty()).ok_or_else(|| usage("--out FILE is required"))?;
    let act = match action.as_deref().ok_or_else(|| usage("--action is required"))? {
        "look" => Act::Look,
        "click" => {
            let kind = match button.as_deref().unwrap_or("left") {
                "left" => ClickKind::Left,
                "right" => ClickKind::Right,
                "middle" => ClickKind::Middle,
                "double" => ClickKind::Double,
                other => return Err(usage(&format!("unknown button {other}"))),
            };
            Act::Click { kind, x: x.ok_or_else(|| usage("click needs --x"))?, y: y.ok_or_else(|| usage("click needs --y"))? }
        }
        "move" => Act::Move { x: x.ok_or_else(|| usage("move needs --x"))?, y: y.ok_or_else(|| usage("move needs --y"))? },
        "drag" => Act::Drag {
            from_x: fx.ok_or_else(|| usage("drag needs --from-x"))?,
            from_y: fy.ok_or_else(|| usage("drag needs --from-y"))?,
            x: x.ok_or_else(|| usage("drag needs --x"))?,
            y: y.ok_or_else(|| usage("drag needs --y"))?,
        },
        "type" => {
            let t = text.ok_or_else(|| usage("type needs --text"))?;
            if t.is_empty() || t.chars().count() > MAX_TYPE_CHARS {
                return Err(usage("type text must be 1..=4096 characters"));
            }
            Act::Type { text: t }
        }
        "key" => {
            let k = key.filter(|s| !s.trim().is_empty()).ok_or_else(|| usage("key needs --key"))?;
            parse_combo(&k)?;
            Act::Key { combo: k }
        }
        "scroll" => {
            let d = match dir.as_deref().unwrap_or("down") {
                "up" => ScrollDir::Up,
                "down" => ScrollDir::Down,
                "left" => ScrollDir::Left,
                "right" => ScrollDir::Right,
                other => return Err(usage(&format!("unknown scroll direction {other}"))),
            };
            let n = amount.unwrap_or(3);
            if !(1..=20).contains(&n) {
                return Err(usage("scroll amount must be 1..=20"));
            }
            Act::Scroll { x: x.ok_or_else(|| usage("scroll needs --x"))?, y: y.ok_or_else(|| usage("scroll needs --y"))?, dir: d, amount: n as u32 }
        }
        "wait" => {
            let m = ms.unwrap_or(500);
            if !(0..=5000).contains(&m) {
                return Err(usage("wait ms must be 0..=5000"));
            }
            Act::Wait { ms: m as u64 }
        }
        other => return Err(usage(&format!("unknown action {other}"))),
    };
    Ok(JobArgs { display, out, act })
}

/// Resolve a named or single-character key to its X11 keysym.
pub fn keysym_of(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "return" | "enter" => return Some(0xff0d),
        "tab" => return Some(0xff09),
        "escape" | "esc" => return Some(0xff1b),
        "backspace" => return Some(0xff08),
        "delete" | "del" => return Some(0xffff),
        "insert" => return Some(0xff63),
        "space" => return Some(0x0020),
        "home" => return Some(0xff50),
        "end" => return Some(0xff57),
        "page_up" | "pageup" | "pgup" | "prior" => return Some(0xff55),
        "page_down" | "pagedown" | "pgdn" | "next" => return Some(0xff56),
        "left" | "arrowleft" => return Some(0xff51),
        "up" | "arrowup" => return Some(0xff52),
        "right" | "arrowright" => return Some(0xff53),
        "down" | "arrowdown" => return Some(0xff54),
        "minus" => return Some(b'-' as u32),
        "plus" => return Some(b'+' as u32),
        "equal" => return Some(b'=' as u32),
        "slash" => return Some(b'/' as u32),
        "backslash" => return Some(b'\\' as u32),
        "period" | "dot" => return Some(b'.' as u32),
        "comma" => return Some(b',' as u32),
        "semicolon" => return Some(b';' as u32),
        "apostrophe" | "quote" => return Some(b'\'' as u32),
        _ => {}
    }
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=12).contains(&n) {
                return Some(0xffbe + n - 1);
            }
        }
    }
    let mut chars = name.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(char_keysym(ch))
}

fn char_keysym(ch: char) -> u32 {
    match ch {
        '\n' | '\r' => 0xff0d,
        '\t' => 0xff09,
        '\x08' => 0xff08,
        c if (' '..='~').contains(&c) => c as u32,
        c if (0xa0..=0xff).contains(&(c as u32)) => c as u32,
        c => 0x0100_0000 | (c as u32),
    }
}

/// Parse `"ctrl+shift+l"` or `"Return"` into `(modifier_keysyms, main_keysym)`.
pub fn parse_combo(combo: &str) -> Result<(Vec<u32>, u32), ActError> {
    let parts: Vec<&str> = combo.split('+').map(str::trim).filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Err(ActError::new("bad_key", "empty key specification"));
    }
    let mut mods = Vec::new();
    for &m in &parts[..parts.len() - 1] {
        let sym = match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => 0xffe3,  // Control_L
            "shift" => 0xffe1,             // Shift_L
            "alt" | "meta" => 0xffe9,      // Alt_L
            "super" | "win" | "cmd" => 0xffeb, // Super_L
            other => return Err(ActError::new("bad_key", format!("unknown modifier {other}"))),
        };
        if !mods.contains(&sym) {
            mods.push(sym);
        }
    }
    let last = parts[parts.len() - 1];
    // For modified ASCII letters like `ctrl+l`, use lowercase keysym unless Shift is meant via `shift+...`.
    let key_str = if !mods.is_empty() && last.len() == 1 && last.as_bytes()[0].is_ascii_uppercase() {
        if !mods.contains(&0xffe1) {
            mods.push(0xffe1);
        }
        last.to_ascii_lowercase()
    } else {
        last.to_string()
    };
    let sym = keysym_of(&key_str).ok_or_else(|| ActError::new("bad_key", format!("unknown key {last}")))?;
    Ok((mods, sym))
}

struct KeyMap {
    min_kc: u8,
    per: u8,
    syms: Vec<u32>,
    scratch_kc: u8,
    shift_kc: Option<u8>,
}

impl KeyMap {
    fn load(conn: &RustConnection) -> Result<KeyMap, ActError> {
        let setup = conn.setup();
        let (min_kc, max_kc) = (setup.min_keycode, setup.max_keycode);
        let count = max_kc.saturating_sub(min_kc).saturating_add(1);
        let reply = conn
            .get_keyboard_mapping(min_kc, count)
            .map_err(|e| ActError::new("x_error", e.to_string()))?
            .reply()
            .map_err(|e| ActError::new("x_error", e.to_string()))?;
        let per = reply.keysyms_per_keycode.max(1);
        let syms = reply.keysyms;
        // Pick an unused keycode near max_kc for on-the-fly Unicode / unmapped keysyms.
        let mut scratch_kc = max_kc;
        for kc in (min_kc..=max_kc).rev() {
            let idx = (kc - min_kc) as usize * per as usize;
            if syms.get(idx..idx + per as usize).is_some_and(|s| s.iter().all(|&k| k == 0)) {
                scratch_kc = kc;
                break;
            }
        }
        let mut km = KeyMap { min_kc, per, syms, scratch_kc, shift_kc: None };
        km.shift_kc = km.find(0xffe1).or_else(|| km.find(0xffe2)).map(|(kc, _)| kc);
        Ok(km)
    }

    fn find(&self, sym: u32) -> Option<(u8, bool)> {
        let per = self.per as usize;
        for (i, chunk) in self.syms.chunks(per).enumerate() {
            let kc = self.min_kc.saturating_add(i as u8);
            if kc == self.scratch_kc {
                continue;
            }
            if chunk.first() == Some(&sym) {
                return Some((kc, false));
            }
            if chunk.get(1) == Some(&sym) {
                return Some((kc, true));
            }
        }
        None
    }

    fn bind_and_tap(&self, conn: &RustConnection, root: u32, sym: u32) -> Result<(), ActError> {
        if let Some((kc, need_shift)) = self.find(sym) {
            if need_shift {
                if let Some(skc) = self.shift_kc {
                    fake(conn, KEY_PRESS, skc, root, 0, 0)?;
                }
            }
            fake(conn, KEY_PRESS, kc, root, 0, 0)?;
            fake(conn, KEY_RELEASE, kc, root, 0, 0)?;
            if need_shift {
                if let Some(skc) = self.shift_kc {
                    fake(conn, KEY_RELEASE, skc, root, 0, 0)?;
                }
            }
            return Ok(());
        }
        let mut row = vec![0u32; self.per as usize];
        row[0] = sym;
        conn.change_keyboard_mapping(1, self.scratch_kc, self.per, &row)
            .map_err(|e| ActError::new("x_error", e.to_string()))?
            .check()
            .map_err(|e| ActError::new("x_error", e.to_string()))?;
        conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))?;
        fake(conn, KEY_PRESS, self.scratch_kc, root, 0, 0)?;
        fake(conn, KEY_RELEASE, self.scratch_kc, root, 0, 0)?;
        conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))
    }
}

fn fake(conn: &RustConnection, kind: u8, detail: u8, root: u32, x: i16, y: i16) -> Result<(), ActError> {
    conn.xtest_fake_input(kind, detail, 0, root, x, y, 0)
        .map_err(|e| ActError::new("x_error", e.to_string()))?;
    Ok(())
}

/// Topmost mapped viewable window on `root` ( optionally containing `(px, py)` ).
fn pick_focus_window(conn: &RustConnection, root: u32, at: Option<(i16, i16)>) -> Option<u32> {
    let tree = conn.query_tree(root).ok()?.reply().ok()?;
    for &win in tree.children.iter().rev() {
        let Ok(attrs_c) = conn.get_window_attributes(win) else { continue };
        let Ok(attrs) = attrs_c.reply() else { continue };
        if attrs.map_state != MapState::VIEWABLE || attrs.override_redirect {
            continue;
        }
        let Ok(geo_c) = conn.get_geometry(win) else { continue };
        let Ok(geo) = geo_c.reply() else { continue };
        if geo.width <= 1 && geo.height <= 1 {
            continue;
        }
        if let Some((px, py)) = at {
            let (x0, y0) = (geo.x as i32, geo.y as i32);
            let (x1, y1) = (x0 + geo.width as i32, y0 + geo.height as i32);
            if (px as i32) < x0 || (px as i32) >= x1 || (py as i32) < y0 || (py as i32) >= y1 {
                continue;
            }
        }
        return Some(win);
    }
    None
}

/// `CU-08`: ensure a mapped client window has keyboard focus on WM-less `Xvfb` without warping the pointer.
fn ensure_focus(conn: &RustConnection, root: u32, at: Option<(i16, i16)>) {
    let unfocused = match conn.get_input_focus().ok().and_then(|c| c.reply().ok()) {
        Some(r) => r.focus == 0 || r.focus == 1 || r.focus == root,
        None => true,
    };
    if unfocused || at.is_some() {
        if let Some(win) = pick_focus_window(conn, root, at).or_else(|| pick_focus_window(conn, root, None)) {
            let _ = conn.set_input_focus(InputFocus::POINTER_ROOT, win, x11rb::CURRENT_TIME);
            let _ = conn.flush();
        }
    }
}

/// Run the requested action and capture the post-action screen in one short-lived helper call.
pub fn execute(job: &JobArgs) -> Result<Value, ActError> {
    let disp_str = job.display.clone().or_else(|| std::env::var("DISPLAY").ok().filter(|s| !s.is_empty()));
    let Some(disp) = disp_str else {
        return Err(ActError::new("no_display", "no DISPLAY is set"));
    };
    let mut screen_pt: Option<(i16, i16)> = None;
    if job.act != Act::Look {
        let (conn, screen_num) = x11rb::connect(Some(disp.as_str())).map_err(|e| ActError::new("no_display", e.to_string()))?;
        let screen = conn.setup().roots.get(screen_num).ok_or_else(|| ActError::new("unsupported_screen", "no screen"))?;
        let (root, sw, sh) = (screen.root, screen.width_in_pixels as u32, screen.height_in_pixels as u32);
        match &job.act {
            Act::Look => {}
            Act::Wait { ms } => std::thread::sleep(Duration::from_millis(*ms)),
            Act::Move { x, y } => {
                let (sx, sy) = scale_point(*x, *y, sw, sh, AGENT_MAX_SIDE)?;
                screen_pt = Some((sx, sy));
                fake(&conn, MOTION_NOTIFY, 0, root, sx, sy)?;
                conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))?;
            }
            Act::Click { kind, x, y } => {
                let (sx, sy) = scale_point(*x, *y, sw, sh, AGENT_MAX_SIDE)?;
                screen_pt = Some((sx, sy));
                fake(&conn, MOTION_NOTIFY, 0, root, sx, sy)?;
                ensure_focus(&conn, root, Some((sx, sy)));
                let (btn, reps) = match kind {
                    ClickKind::Left => (1u8, 1usize),
                    ClickKind::Middle => (2, 1),
                    ClickKind::Right => (3, 1),
                    ClickKind::Double => (1, 2),
                };
                for r in 0..reps {
                    if r > 0 {
                        std::thread::sleep(Duration::from_millis(35));
                    }
                    fake(&conn, BUTTON_PRESS, btn, root, 0, 0)?;
                    fake(&conn, BUTTON_RELEASE, btn, root, 0, 0)?;
                }
                conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))?;
            }
            Act::Drag { from_x, from_y, x, y } => {
                let (fx, fy) = scale_point(*from_x, *from_y, sw, sh, AGENT_MAX_SIDE)?;
                let (sx, sy) = scale_point(*x, *y, sw, sh, AGENT_MAX_SIDE)?;
                screen_pt = Some((sx, sy));
                fake(&conn, MOTION_NOTIFY, 0, root, fx, fy)?;
                ensure_focus(&conn, root, Some((fx, fy)));
                fake(&conn, BUTTON_PRESS, 1, root, 0, 0)?;
                conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))?;
                for step in 1..=5i32 {
                    let ix = (fx as i32 + (sx as i32 - fx as i32) * step / 5) as i16;
                    let iy = (fy as i32 + (sy as i32 - fy as i32) * step / 5) as i16;
                    fake(&conn, MOTION_NOTIFY, 0, root, ix, iy)?;
                }
                fake(&conn, BUTTON_RELEASE, 1, root, 0, 0)?;
                conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))?;
            }
            Act::Scroll { x, y, dir, amount } => {
                let (sx, sy) = scale_point(*x, *y, sw, sh, AGENT_MAX_SIDE)?;
                screen_pt = Some((sx, sy));
                fake(&conn, MOTION_NOTIFY, 0, root, sx, sy)?;
                let btn = match dir {
                    ScrollDir::Up => 4u8,
                    ScrollDir::Down => 5,
                    ScrollDir::Left => 6,
                    ScrollDir::Right => 7,
                };
                for _ in 0..*amount {
                    fake(&conn, BUTTON_PRESS, btn, root, 0, 0)?;
                    fake(&conn, BUTTON_RELEASE, btn, root, 0, 0)?;
                }
                conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))?;
            }
            Act::Type { text } => {
                ensure_focus(&conn, root, None);
                let km = KeyMap::load(&conn)?;
                for ch in text.chars() {
                    km.bind_and_tap(&conn, root, char_keysym(ch))?;
                }
                conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))?;
            }
            Act::Key { combo } => {
                ensure_focus(&conn, root, None);
                let (mods, sym) = parse_combo(combo)?;
                let km = KeyMap::load(&conn)?;
                let mut held = Vec::new();
                for m in mods {
                    if let Some((kc, _)) = km.find(m) {
                        fake(&conn, KEY_PRESS, kc, root, 0, 0)?;
                        held.push(kc);
                    }
                }
                km.bind_and_tap(&conn, root, sym)?;
                for kc in held.into_iter().rev() {
                    fake(&conn, KEY_RELEASE, kc, root, 0, 0)?;
                }
                conn.sync().map_err(|e| ActError::new("x_error", e.to_string()))?;
            }
        }
        drop(conn);
        if !matches!(job.act, Act::Wait { .. }) {
            std::thread::sleep(Duration::from_millis(SETTLE_MS));
        }
    }
    let cap = capture::run(
        &capture::Request { kind: Kind::Screen, out: job.out.clone() },
        &Options { display: Some(disp.clone()), ..Options::default() },
    )
    .map_err(|e| ActError::new(e.reason, e.detail))?;
    let mut out = cap.to_json();
    out["display"] = json!(disp);
    if let Some((sx, sy)) = screen_pt {
        out["screen_x"] = json!(sx);
        out["screen_y"] = json!(sy);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_point_is_exact_on_unscaled_and_downscaled_screens_and_refuses_outside() {
        assert_eq!(scale_point(0, 0, 1280, 800, 1568).unwrap(), (0, 0));
        assert_eq!(scale_point(1279, 799, 1280, 800, 1568).unwrap(), (1279, 799));
        assert_eq!(scale_point(640, 400, 1280, 800, 1568).unwrap(), (640, 400));
        assert_eq!(scale_point(1280, 400, 1280, 800, 1568).unwrap_err().reason, "out_of_bounds");
        // 1920x1080 -> agent 1568x882
        assert_eq!(scale_point(0, 0, 1920, 1080, 1568).unwrap(), (0, 0));
        assert_eq!(scale_point(1567, 881, 1920, 1080, 1568).unwrap(), (1919, 1079));
        assert_eq!(scale_point(784, 441, 1920, 1080, 1568).unwrap(), (960, 540));
        assert_eq!(scale_point(1568, 0, 1920, 1080, 1568).unwrap_err().reason, "out_of_bounds");
        assert_eq!(scale_point(-1, 0, 1920, 1080, 1568).unwrap_err().reason, "out_of_bounds");
    }

    #[test]
    fn combo_parser_maps_modifiers_special_keys_and_unicode() {
        assert_eq!(parse_combo("Return").unwrap(), (vec![], 0xff0d));
        assert_eq!(parse_combo("ctrl+l").unwrap(), (vec![0xffe3], b'l' as u32));
        assert_eq!(parse_combo("ctrl+shift+Tab").unwrap(), (vec![0xffe3, 0xffe1], 0xff09));
        assert_eq!(parse_combo("alt+F4").unwrap(), (vec![0xffe9], 0xffc1));
        assert_eq!(keysym_of("가"), Some(0x0100_0000 | ('가' as u32)));
        assert!(parse_combo("bogusmod+a").is_err());
        assert!(parse_combo("UnknownKeyName").is_err());
    }
}
