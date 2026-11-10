//! `lico capture`: the screen, a window or a region of it as a PNG file, and a smaller copy for the agent (plan M02-P04, S02b; XF-01 to XF-03).
//!
//! The core is `run`: it takes a `Request` and `Options` and returns the `Answer` or the `CapError` as values; only `cli.rs` prints, and no global
//! state or standard output is used here, so S03 can call the same function as a hidden job. The screen is read once, a band of rows at a time
//! (`GetImage`, about 256 KiB a band), every row is converted to RGB and goes in the same pass to the encoder of the full file and to the shrinker
//! whose rows go to the encoder of the agent copy; both files therefore show the same instant and the memory does not grow with the height.
//!
//! Failures have a machine-readable reason: `no_display`, `usage`, `bad_region`, `no_such_window`, `unsupported_screen`, `write_failed`,
//! `x_error` (with the name of the X error). No file is ever left under a final name after an error.

pub mod layout;
pub(crate) mod out;
pub mod shrink;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_x;

use serde_json::{json, Value};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, ImageOrder, MapState, VisualClass};
use x11rb::protocol::ErrorKind;

use crate::png::PngEncoder;
use layout::{Layout, Parts};
use shrink::{agent_size, Shrinker};

/// The longest side of the agent copy, in pixels.
pub const AGENT_MAX_SIDE: u32 = 1568;
/// The size of a band of `GetImage` rows, in bytes of the reply.
pub const BAND_BYTES: usize = 256 * 1024;
pub use out::{fits, AGENT_LIMIT};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Screen,
    Window(u32),
    Region { x: i64, y: i64, w: i64, h: i64 },
}

#[derive(Clone, Debug)]
pub struct Request {
    pub kind: Kind,
    pub out: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// The display name; `None` is `$DISPLAY`.
    pub display: Option<String>,
    /// The most bytes an agent copy may have; over it the copy is not delivered.
    pub agent_limit: u64,
    pub max_side: u32,
    pub band_bytes: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options { display: None, agent_limit: AGENT_LIMIT, max_side: AGENT_MAX_SIDE, band_bytes: BAND_BYTES }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapError {
    pub reason: &'static str,
    pub detail: String,
}

impl CapError {
    fn new(reason: &'static str, detail: impl Into<String>) -> CapError {
        CapError { reason, detail: detail.into() }
    }
    pub fn to_json(&self) -> Value {
        json!({"ok": 0, "error": self.reason, "detail": self.detail})
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Agent {
    /// A capture that was already small enough: the agent copy is the full file.
    Same,
    File { path: String, width: u32, height: u32, bytes: u64, scale: f64 },
    TooLarge { bytes: u64, limit: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// The top-left of what was captured, in screen coordinates.
    pub offset: (i64, i64),
    pub vendor: String,
    pub release: u32,
    pub agent: Agent,
}

impl Answer {
    pub fn to_json(&self) -> Value {
        let agent = match &self.agent {
            Agent::Same => json!({"path": self.path, "width": self.width, "height": self.height, "bytes": self.bytes, "scale": 1.0}),
            Agent::File { path, width, height, bytes, scale } => json!({"path": path, "width": width, "height": height, "bytes": bytes, "scale": scale}),
            Agent::TooLarge { bytes, limit } => json!({"error": "too_large", "bytes": bytes, "limit": limit}),
        };
        json!({
            "ok": 1, "path": self.path, "width": self.width, "height": self.height, "bytes": self.bytes,
            "offset": {"x": self.offset.0, "y": self.offset.1},
            "server": {"vendor": self.vendor, "release": self.release},
            "agent": agent,
        })
    }
}

// ---- the arguments ---------------------------------------------------------------------------------------------------------

fn parse_window(text: &str) -> Option<u32> {
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => u32::from_str_radix(hex, 16).ok(),
        Some(_) => None,
        None if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => text.parse().ok(),
        None => None,
    }
}

fn parse_region(text: &str) -> Result<Kind, CapError> {
    let parts: Vec<&str> = text.split(',').collect();
    let nums: Vec<Option<i64>> = parts.iter().map(|p| p.parse::<i64>().ok()).collect();
    if parts.len() != 4 || nums.iter().any(Option::is_none) {
        return Err(CapError::new("bad_region", "a region is four integers: x,y,w,h"));
    }
    let n: Vec<i64> = nums.into_iter().flatten().collect();
    Ok(Kind::Region { x: n[0], y: n[1], w: n[2], h: n[3] })
}

/// The words after `capture`.
pub fn parse_args(args: &[String]) -> Result<Request, CapError> {
    let usage = |m: &str| CapError::new("usage", m.to_string());
    let (mut kind, mut out): (Option<Kind>, Option<String>) = (None, None);
    let mut i = 0;
    let set_kind = |k: Kind, kind: &mut Option<Kind>| -> Result<(), CapError> {
        if kind.is_some() {
            return Err(usage("--screen, --window and --region exclude each other"));
        }
        *kind = Some(k);
        Ok(())
    };
    while i < args.len() {
        let value = args.get(i + 1);
        match (args[i].as_str(), value) {
            ("--screen", _) => {
                set_kind(Kind::Screen, &mut kind)?;
                i += 1;
                continue;
            }
            ("--window", Some(v)) => set_kind(Kind::Window(parse_window(v).ok_or_else(|| usage("--window needs a decimal or 0x hex number"))?), &mut kind)?,
            ("--region", Some(v)) => {
                if kind.is_some() {
                    return Err(usage("--screen, --window and --region exclude each other"));
                }
                set_kind(parse_region(v)?, &mut kind)?
            }
            ("--out", Some(v)) => {
                if out.replace(v.clone()).is_some() {
                    return Err(usage("--out is given twice"));
                }
            }
            (flag @ ("--window" | "--region" | "--out"), None) => return Err(usage(&format!("{flag} needs a value"))),
            (other, _) => return Err(usage(&format!("unknown argument {other}"))),
        }
        i += 2;
    }
    let out = out.filter(|o| !o.is_empty()).ok_or_else(|| usage("--out FILE is required"))?;
    let out = Path::new(&out);
    let out = if out.is_absolute() { out.to_path_buf() } else { std::env::current_dir().map_err(|e| usage(&format!("no working folder: {e}")))?.join(out) };
    Ok(Request { kind: kind.unwrap_or(Kind::Screen), out })
}

// ---- the server ------------------------------------------------------------------------------------------------------------

fn x_error(e: impl Into<ReplyError>, about_window: bool) -> CapError {
    match e.into() {
        ReplyError::X11Error(x) if about_window && matches!(x.error_kind, ErrorKind::Window | ErrorKind::Drawable) => CapError::new("no_such_window", format!("Bad{:?}", x.error_kind)),
        ReplyError::X11Error(x) => CapError::new("x_error", format!("Bad{:?}", x.error_kind)),
        ReplyError::ConnectionError(c) => CapError::new("x_error", format!("connection: {c}")),
    }
}

fn write_failed(e: io::Error) -> CapError {
    CapError::new("write_failed", e.to_string())
}

fn layout_of(setup: &x11rb::protocol::xproto::Setup, screen: &x11rb::protocol::xproto::Screen) -> Result<Layout, CapError> {
    let visual = screen.allowed_depths.iter().flat_map(|d| d.visuals.iter()).find(|v| v.visual_id == screen.root_visual);
    let format = setup.pixmap_formats.iter().find(|f| f.depth == screen.root_depth);
    let (Some(v), Some(f)) = (visual, format) else { return Err(CapError::new("unsupported_screen", "no visual or pixel format for the root window")) };
    Layout::new(&Parts {
        depth: screen.root_depth,
        bits_per_pixel: f.bits_per_pixel,
        scanline_pad: f.scanline_pad,
        msb_first: setup.image_byte_order == ImageOrder::MSB_FIRST,
        true_color: v.class == VisualClass::TRUE_COLOR,
        red_mask: v.red_mask,
        green_mask: v.green_mask,
        blue_mask: v.blue_mask,
    })
    .map_err(|m| CapError::new("unsupported_screen", m))
}

/// What a window is on the screen: its inside rectangle in screen coordinates (x, y, width, height), not yet clipped.
fn window_rect(conn: &impl Connection, root: u32, id: u32) -> Result<(i64, i64, i64, i64), CapError> {
    let attrs = conn.get_window_attributes(id).map_err(|e| x_error(e, true))?.reply().map_err(|e| x_error(e, true))?;
    if attrs.map_state != MapState::VIEWABLE {
        return Err(CapError::new("no_such_window", "the window is not viewable"));
    }
    let geo = conn.get_geometry(id).map_err(|e| x_error(e, true))?.reply().map_err(|e| x_error(e, true))?;
    // the inside of the window (inside its border) in the root's coordinates, whatever windows it sits in
    let at = conn.translate_coordinates(id, root, 0, 0).map_err(|e| x_error(e, true))?.reply().map_err(|e| x_error(e, true))?;
    if !at.same_screen {
        return Err(CapError::new("no_such_window", "the window is on another screen"));
    }
    Ok((at.dst_x as i64, at.dst_y as i64, geo.width as i64, geo.height as i64))
}

/// Clips a rectangle to the screen; `None` when nothing of it is on the screen.
fn clip(r: (i64, i64, i64, i64), sw: i64, sh: i64) -> Option<(i64, i64, i64, i64)> {
    let (x0, y0, x1, y1) = (r.0.max(0), r.1.max(0), (r.0 + r.2).min(sw), (r.1 + r.3).min(sh));
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

type Sink = PngEncoder<BufWriter<std::fs::File>>;

pub fn run(req: &Request, opts: &Options) -> Result<Answer, CapError> {
    let (conn, num) = x11rb::connect(opts.display.as_deref()).map_err(|e| CapError::new("no_display", e.to_string()))?;
    let setup = conn.setup();
    let screen = &setup.roots[num];
    let (root, sw, sh) = (screen.root, screen.width_in_pixels as i64, screen.height_in_pixels as i64);
    let layout = layout_of(setup, screen)?;
    let (rx, ry, rw, rh) = match &req.kind {
        Kind::Screen => (0, 0, sw, sh),
        Kind::Window(id) => {
            let r = window_rect(&conn, root, *id)?;
            clip(r, sw, sh).ok_or_else(|| CapError::new("no_such_window", "no part of the window is on the screen"))?
        }
        Kind::Region { x, y, w, h } => {
            // checked in i64, so a huge number cannot wrap
            if *w < 1 || *h < 1 || *x < 0 || *y < 0 || x.checked_add(*w).is_none_or(|e| e > sw) || y.checked_add(*h).is_none_or(|e| e > sh) {
                return Err(CapError::new("bad_region", format!("{x},{y},{w},{h} is not inside the {sw} x {sh} screen")));
            }
            (*x, *y, *w, *h)
        }
    };
    let (width, height) = (rw as u32, rh as u32);
    let (aw, ah) = agent_size(width, height, opts.max_side);
    let shrunk = (aw, ah) != (width, height);

    let (full_pending, full_file) = out::Pending::create(&req.out).map_err(write_failed)?;
    let mut full: Sink = PngEncoder::new(BufWriter::new(full_file), width, height).map_err(write_failed)?;
    let agent_path = agent_path_of(&req.out);
    let mut agent: Option<(out::Pending, Shrinker, Sink)> = if shrunk {
        let (p, f) = out::Pending::create(&agent_path).map_err(write_failed)?;
        let enc = PngEncoder::new(BufWriter::new(f), aw, ah).map_err(write_failed)?;
        Some((p, Shrinker::new(width as usize, height as usize, aw as usize, ah as usize), enc))
    } else {
        None
    };

    let stride = layout.stride(width as usize);
    let band_rows = (opts.band_bytes / stride).max(1) as i64;
    let mut rgb = vec![0u8; width as usize * 3];
    let mut y = 0;
    while y < rh {
        let rows = band_rows.min(rh - y);
        let reply = conn
            .get_image(ImageFormat::Z_PIXMAP, root, rx as i16, (ry + y) as i16, width as u16, rows as u16, !0)
            .map_err(|e| x_error(e, false))?
            .reply()
            .map_err(|e| x_error(e, false))?;
        if reply.data.len() < stride * rows as usize {
            return Err(CapError::new("x_error", "GetImage returned fewer bytes than the rows"));
        }
        for r in 0..rows as usize {
            layout.row_to_rgb(&reply.data[r * stride..], width as usize, &mut rgb);
            full.push_row(&rgb).map_err(write_failed)?;
            if let Some((_, shrinker, enc)) = agent.as_mut() {
                shrinker.push_row(&rgb, &mut |row| enc.push_row(row)).map_err(write_failed)?;
            }
        }
        y += rows;
    }
    full.finish().map_err(write_failed)?;
    let bytes = full_pending.len().map_err(write_failed)?;

    let mut delivered: Option<Agent> = None;
    let mut agent_commit: Option<out::Pending> = None;
    if let Some((pending, shrinker, mut enc)) = agent {
        debug_assert!(shrinker.done());
        enc.finish().map_err(write_failed)?;
        let size = pending.len().map_err(write_failed)?;
        if fits(size, opts.agent_limit) {
            delivered = Some(Agent::File { path: agent_path.to_string_lossy().into_owned(), width: aw, height: ah, bytes: size, scale: aw.max(ah) as f64 / width.max(height) as f64 });
            agent_commit = Some(pending);
        } else {
            drop(pending); // removes the temporary file
            let _ = std::fs::remove_file(&agent_path); // and any copy an earlier capture left under the final name
            delivered = Some(Agent::TooLarge { bytes: size, limit: opts.agent_limit });
        }
    }
    if let Some(p) = agent_commit {
        p.commit().map_err(write_failed)?;
    }
    if let Err(e) = full_pending.commit() {
        if matches!(delivered, Some(Agent::File { .. })) {
            let _ = std::fs::remove_file(&agent_path);
        }
        return Err(write_failed(e));
    }
    Ok(Answer {
        path: req.out.to_string_lossy().into_owned(),
        width,
        height,
        bytes,
        offset: (rx, ry),
        vendor: String::from_utf8_lossy(&setup.vendor).into_owned(),
        release: setup.release_number,
        agent: delivered.unwrap_or(Agent::Same),
    })
}

/// `<stem>.agent.png` next to the output file.
pub fn agent_path_of(out: &Path) -> PathBuf {
    let stem = out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "capture".into());
    out.with_file_name(format!("{stem}.agent.png"))
}

/// Captures the whole screen in bands into an in-memory PNG (shrunk to `AGENT_MAX_SIDE` if needed, capped at `MAX_IMAGE_FILE`), writing no file.
pub fn screen_png(display: Option<&str>) -> Result<(Vec<u8>, u32, u32), CapError> {
    let (conn, num) = x11rb::connect(display).map_err(|e| CapError::new("no_display", e.to_string()))?;
    let setup = conn.setup();
    let screen = &setup.roots[num];
    let (root, width, height) = (screen.root, screen.width_in_pixels as u32, screen.height_in_pixels as u32);
    let layout = layout_of(setup, screen)?;
    let (aw, ah) = agent_size(width, height, AGENT_MAX_SIDE);
    let mut enc = PngEncoder::new(Vec::new(), aw, ah).map_err(write_failed)?;
    let mut shrinker = ((aw, ah) != (width, height)).then(|| Shrinker::new(width as usize, height as usize, aw as usize, ah as usize));
    let stride = layout.stride(width as usize);
    let band_rows = (BAND_BYTES / stride).max(1) as u32;
    let mut rgb = vec![0u8; width as usize * 3];
    let mut y = 0;
    while y < height {
        let rows = band_rows.min(height - y);
        let reply = conn.get_image(ImageFormat::Z_PIXMAP, root, 0, y as i16, width as u16, rows as u16, !0).map_err(|e| x_error(e, false))?.reply().map_err(|e| x_error(e, false))?;
        if reply.data.len() < stride * rows as usize {
            return Err(CapError::new("x_error", "GetImage returned fewer bytes than the rows"));
        }
        for r in 0..rows as usize {
            layout.row_to_rgb(&reply.data[r * stride..], width as usize, &mut rgb);
            match shrinker.as_mut() {
                Some(s) => s.push_row(&rgb, &mut |row| enc.push_row(row)).map_err(write_failed)?,
                None => enc.push_row(&rgb).map_err(write_failed)?,
            }
        }
        y += rows;
    }
    let bytes = enc.finish().map_err(write_failed)?;
    let max = crate::clipboard::open::MAX_IMAGE_FILE;
    if !fits(bytes.len() as u64, max) {
        return Err(CapError::new("too_large", format!("{} bytes exceeds {max}", bytes.len())));
    }
    Ok((bytes, aw, ah))
}

/// The command: the words after `capture`. Prints one JSON line; the exit code is 0, 1 for a usage error and 2 for any other failure.
pub fn command(args: &[String]) -> ! {
    let result = parse_args(args).and_then(|req| run(&req, &Options::default()));
    let (line, code) = match &result {
        Ok(answer) => (answer.to_json(), 0),
        Err(e) => (e.to_json(), if e.reason == "usage" { 1 } else { 2 }),
    };
    println!("{line}");
    std::process::exit(code);
}
