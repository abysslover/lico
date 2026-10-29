//! The clipboard reader (plan M02-P04, S04a; XF-04): what the owner of a selection (`CLIPBOARD` or `PRIMARY`) holds, as text or as a PNG.
//!
//! `read_selection` returns a VALUE and never prints and never touches the window: it opens a connection of its own to the X server with a
//! helper window of its own, asks the owner for `TARGETS` first, converts the one target the wanted kind can use (text: `UTF8_STRING`, and
//! `STRING` as `gui/clip.rs` does; image: `image/png` only) and reads the property. A property over the server's maximum request size arrives
//! by the incremental protocol (INCR): the property is deleted to get the next chunk, a chunk of size 0 ends it, the total is counted against
//! the limit as it arrives, and ONE deadline covers the whole read (connect, TARGETS and every chunk). It keeps no global state, so the window
//! (S05) and the job (`job.rs`, S04a; S07 through `helper::run_job`) call it alike. The job is `lico clipboard read` and, hidden, the job
//! `clipboard_read` of the helper process.
//!
//! Failures have a machine-readable reason: `empty` (no owner, no data, or the owner refuses), `timeout`, `too_large` (over the limit, or a
//! picture over `MAX_SIDE` pixels on a side: refused, never cut), `unsupported_format` (the owner lists targets but none the wanted kind can
//! use), `bad_image` (the bytes are not a PNG), `no_display`, `x_error`.

pub mod job;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_x;

use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, CreateWindowAux, EventMask, Property, WindowClass};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

/// The largest width or height of a picture that is accepted (read from the PNG header; nothing is decoded).
pub const MAX_SIDE: u32 = 8000;
/// The smallest PNG this reader can look at: the signature (8 bytes) and the IHDR chunk (4 + 4 + 13 + 4 bytes).
const HEAD_BYTES: usize = 33;
/// The most bytes of a `TARGETS` answer that are read (a list of atoms; far more than any owner offers).
const TARGETS_MAX: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Clipboard,
    Primary,
}

impl Selection {
    pub fn name(self) -> &'static str {
        match self {
            Selection::Clipboard => "clipboard",
            Selection::Primary => "primary",
        }
    }
    fn atom_name(self) -> &'static [u8] {
        match self {
            Selection::Clipboard => b"CLIPBOARD",
            Selection::Primary => b"PRIMARY",
        }
    }
}

/// What the caller wants: text, an image, or an image when the owner offers one and else text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    Text,
    Image,
    Auto,
}

/// What a read may take: the most bytes of a text and of an image, and the time for the whole read.
#[derive(Clone, Copy, Debug)]
pub struct ReadLimits {
    pub text_max: usize,
    pub image_max: usize,
    pub deadline: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clip {
    Text(String),
    /// The bytes of a PNG whose signature and header were checked, and its size from the header.
    Png { bytes: Vec<u8>, width: u32, height: u32 },
}

/// A clip and how it arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    pub clip: Clip,
    /// The chunks of an incremental transfer (0: the whole property came in one reply). Read by the tests as the evidence that INCR was taken.
    #[allow(dead_code)]
    pub incr_chunks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipError {
    pub reason: &'static str,
    pub detail: String,
}

impl ClipError {
    pub fn new(reason: &'static str, detail: impl Into<String>) -> ClipError {
        ClipError { reason, detail: detail.into() }
    }
}

/// The atoms the choice of a target needs.
#[derive(Clone, Copy, Debug)]
pub struct Atoms {
    pub utf8: Atom,
    pub png: Atom,
}

/// The target the owner is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Png,
    Text(Atom),
}

const STRING: Atom = 31; // the predefined atom STRING

/// Which target to ask for, from the list the owner offers; `unsupported_format` when it lists targets and none of them can be used.
pub fn pick(want: Want, offered: &[Atom], atoms: &Atoms) -> Result<Pick, ClipError> {
    let has = |a: Atom| offered.contains(&a);
    let text = || [atoms.utf8, STRING].into_iter().find(|a| has(*a)).map(Pick::Text);
    let found = match want {
        Want::Image => has(atoms.png).then_some(Pick::Png),
        Want::Text => text(),
        Want::Auto => has(atoms.png).then_some(Pick::Png).or_else(text),
    };
    found.ok_or_else(|| {
        let wanted = match want {
            Want::Text => "text (UTF8_STRING)",
            Want::Image => "image/png",
            Want::Auto => "image/png or text",
        };
        ClipError::new("unsupported_format", format!("the owner offers {} targets, none of them {wanted}", offered.len()))
    })
}

/// The size of a PNG from its header (signature, then an IHDR chunk of 13 bytes); nothing is decoded. A side of 0 or bytes that are not a PNG header
/// are `bad_image`; a side over `MAX_SIDE` is `too_large`.
pub fn png_size(data: &[u8]) -> Result<(u32, u32), ClipError> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.len() < HEAD_BYTES || data[..8] != SIGNATURE || data[8..12] != [0, 0, 0, 13] || &data[12..16] != b"IHDR" {
        return Err(ClipError::new("bad_image", "the bytes are not a PNG (no signature and IHDR chunk)"));
    }
    let side = |at: usize| u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    let (w, h) = (side(16), side(20));
    if w == 0 || h == 0 {
        return Err(ClipError::new("bad_image", format!("the PNG header says {w} x {h}")));
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(ClipError::new("too_large", format!("the picture is {w} x {h}, over {MAX_SIDE} pixels on a side")));
    }
    Ok((w, h))
}

/// The bytes of an incremental transfer, counted against a limit as they arrive.
#[derive(Debug)]
pub struct Collector {
    limit: usize,
    bytes: Vec<u8>,
}

impl Collector {
    pub fn new(limit: usize) -> Collector {
        Collector { limit, bytes: Vec::new() }
    }
    /// Adds a chunk; over the limit (the total so far plus the chunk) is `too_large`, never a cut.
    pub fn push(&mut self, chunk: &[u8]) -> Result<(), ClipError> {
        if chunk.len() > self.limit - self.bytes.len() {
            return Err(ClipError::new("too_large", format!("more than {} bytes", self.limit)));
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

// ---- the server ------------------------------------------------------------------------------------------------------------

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn poll(fds: *mut PollFd, nfds: std::ffi::c_ulong, timeout: i32) -> i32;
}

const POLLIN: i16 = 1;

fn xe(e: impl std::fmt::Display) -> ClipError {
    ClipError::new("x_error", e.to_string())
}

fn timeout_error(since: Instant, deadline: Instant) -> ClipError {
    ClipError::new("timeout", format!("the owner did not finish within {} ms", deadline.saturating_duration_since(since).as_millis()))
}

/// What one transfer gave.
struct Taken {
    value: Vec<u8>,
    chunks: u32,
}

struct Session {
    conn: RustConnection,
    win: u32,
    selection: Atom,
    property: Atom,
    incr: Atom,
    start: Instant,
    deadline: Instant,
}

impl Session {
    /// The next event, or `timeout` at the deadline of the whole read.
    fn event(&self) -> Result<Event, ClipError> {
        loop {
            if let Some(event) = self.conn.poll_for_event().map_err(xe)? {
                return Ok(event);
            }
            let now = Instant::now();
            if now >= self.deadline {
                return Err(timeout_error(self.start, self.deadline));
            }
            let wait = (self.deadline - now).as_micros().div_ceil(1000).clamp(1, i32::MAX as u128) as i32;
            let mut fd = PollFd { fd: self.conn.stream().as_raw_fd(), events: POLLIN, revents: 0 };
            unsafe { poll(&mut fd, 1, wait) }; // woken by the connection or by the deadline; the loop looks at both
        }
    }

    /// Asks the owner to convert the selection to `target` into our property; `false` when the owner refuses.
    fn ask(&self, target: Atom) -> Result<bool, ClipError> {
        self.conn.convert_selection(self.win, self.selection, target, self.property, x11rb::CURRENT_TIME).map_err(xe)?;
        self.conn.flush().map_err(xe)?;
        loop {
            if let Event::SelectionNotify(e) = self.event()? {
                if e.requestor == self.win && e.selection == self.selection && e.target == target {
                    return Ok(e.property != x11rb::NONE);
                }
            }
        }
    }

    /// Reads the property the owner set (and deletes it), in one reply or, when the owner announces INCR, chunk by chunk; at most `max` bytes.
    fn take(&self, max: usize) -> Result<Taken, ClipError> {
        let long_length = (max / 4 + 2) as u32; // room for one byte over the limit, so that "over" can be told from "exactly"
        let first = self.conn.get_property(true, self.win, self.property, AtomEnum::ANY, 0, long_length).map_err(xe)?.reply().map_err(xe)?;
        if first.type_ == x11rb::NONE {
            return Err(ClipError::new("empty", "the owner set no data"));
        }
        if first.type_ != self.incr {
            if first.bytes_after > 0 || first.value.len() > max {
                return Err(ClipError::new("too_large", format!("more than {max} bytes")));
            }
            return Ok(Taken { value: first.value, chunks: 0 });
        }
        // the incremental transfer: the property was deleted by the read above, which is the owner's cue to send the first chunk
        let mut got = Collector::new(max);
        let mut chunks = 0;
        loop {
            match self.event()? {
                Event::PropertyNotify(e) if e.window == self.win && e.atom == self.property && e.state == Property::NEW_VALUE => {
                    let left = (max - got.bytes.len()) as u32;
                    let reply = self.conn.get_property(true, self.win, self.property, AtomEnum::ANY, 0, left / 4 + 2).map_err(xe)?.reply().map_err(xe)?;
                    if reply.type_ == x11rb::NONE {
                        continue; // the owner changed its mind about the property; wait for the next value
                    }
                    if reply.value.is_empty() && reply.bytes_after == 0 {
                        return Ok(Taken { value: got.into_bytes(), chunks });
                    }
                    if reply.bytes_after > 0 {
                        return Err(ClipError::new("too_large", format!("more than {max} bytes")));
                    }
                    got.push(&reply.value)?;
                    chunks += 1;
                }
                _ => {}
            }
        }
    }

    fn targets(&self) -> Result<Option<Vec<Atom>>, ClipError> {
        let targets = self.atom(b"TARGETS")?;
        if !self.ask(targets)? {
            return Ok(None);
        }
        let taken = match self.take(TARGETS_MAX) {
            Ok(t) => t,
            Err(e) if e.reason == "empty" => return Ok(None),
            Err(e) => return Err(e),
        };
        Ok(Some(taken.value.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect()))
    }

    fn atom(&self, name: &[u8]) -> Result<Atom, ClipError> {
        Ok(self.conn.intern_atom(false, name).map_err(xe)?.reply().map_err(xe)?.atom)
    }

    /// Converts to `pick` and reads it, within `max` bytes.
    fn fetch(&self, pick: Pick, atoms: &Atoms, limits: &ReadLimits) -> Result<Option<Read>, ClipError> {
        let (target, max) = match pick {
            Pick::Png => (atoms.png, limits.image_max),
            Pick::Text(a) => (a, limits.text_max),
        };
        if !self.ask(target)? {
            return Ok(None); // the owner refuses this form
        }
        let taken = self.take(max)?;
        let clip = match pick {
            Pick::Png => {
                let (width, height) = png_size(&taken.value)?;
                Clip::Png { bytes: taken.value, width, height }
            }
            Pick::Text(_) if taken.value.is_empty() => return Err(ClipError::new("empty", "the clipboard holds an empty text")),
            Pick::Text(_) => Clip::Text(String::from_utf8_lossy(&taken.value).into_owned()),
        };
        Ok(Some(Read { clip, incr_chunks: taken.chunks }))
    }
}

/// Reads what the owner of `selection` holds, as `want` says. `display` is the display name (`None`: `$DISPLAY`).
pub fn read_selection(display: Option<&str>, selection: Selection, want: Want, limits: &ReadLimits) -> Result<Read, ClipError> {
    let start = Instant::now();
    let deadline = start + limits.deadline;
    let (conn, num) = x11rb::connect(display).map_err(|e| ClipError::new("no_display", e.to_string()))?;
    let root = conn.setup().roots[num].root;
    let win = conn.generate_id().map_err(xe)?;
    // the property changes of our own window are the cue of every chunk of an incremental transfer
    let aux = CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);
    conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &aux).map_err(xe)?;
    let mut session = Session { conn, win, selection: 0, property: 0, incr: 0, start, deadline };
    session.selection = session.atom(selection.atom_name())?;
    session.property = session.atom(b"LICO_CLIP")?;
    session.incr = session.atom(b"INCR")?;
    let atoms = Atoms { utf8: session.atom(b"UTF8_STRING")?, png: session.atom(b"image/png")? };
    let owner = session.conn.get_selection_owner(session.selection).map_err(xe)?.reply().map_err(xe)?.owner;
    if owner == x11rb::NONE {
        return Err(ClipError::new("empty", format!("nobody owns the {} selection", selection.name())));
    }
    match session.targets()? {
        Some(offered) if !offered.is_empty() => {
            let pick = pick(want, &offered, &atoms)?;
            session.fetch(pick, &atoms, limits)?.ok_or_else(|| ClipError::new("empty", "the owner refused the target it offers"))
        }
        Some(_) => Err(ClipError::new("empty", "the owner offers no targets")),
        None => {
            // an owner that does not answer TARGETS: ask for the wanted forms in turn (an image first when any will do)
            let tries: &[Pick] = match want {
                Want::Text => &[Pick::Text(atoms.utf8), Pick::Text(STRING)],
                Want::Image => &[Pick::Png],
                Want::Auto => &[Pick::Png, Pick::Text(atoms.utf8), Pick::Text(STRING)],
            };
            for pick in tries {
                if let Some(read) = session.fetch(*pick, &atoms, limits)? {
                    return Ok(read);
                }
            }
            Err(ClipError::new("empty", "the owner does not list its targets and refuses every form asked for"))
        }
    }
}
