//! The window side of a pasted picture (plan M02-P04, S05b, deviation D12; the host side is `crate::pasted`, S05a).
//!
//! Ctrl+V in the input box of a session asks for the TEXT of the CLIPBOARD first (as it always did); only when there is none it asks for a PNG picture
//! (at most 3 MiB, sides 1..8000, 2 s). A picture becomes a CHIP of the open session, and only a Claude session takes one. One chip per session, shown on
//! the line under the box; Escape removes it; Enter with text sends it:
//!
//! 1. the chip moves to the pictures "on their way" and the window asks `picture.slot` (id `slot-<pid>-<n>`); the text waits in `pending_inputs` under that id;
//! 2. the host answers with a place: the window writes the PNG into the empty file it was handed (`open::write_slot`: no create, no link, a file of ours) and
//!    sends `input` with `attachment` = the NAME (never the path); the picture now waits under the request id of that input;
//! 3. the `input` answer ends it: ok drops the picture from memory, a refusal gives the text AND the chip back with a sentence.
//!
//! The window deletes nothing itself: the host removes the file after any answer to an `input` that named it, and sweeps a slot that was never named.
//! The window holds at most 6 MiB of pictures in all (chips of every session and pictures on their way). Nothing about the pixels is ever dumped.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use super::editor::Prompt;
use super::ui::clip::fetch_selection;
use super::ui::reconnect::input_note;
use super::{emit_ime, jstr, App};
use crate::clipboard::{open, read_selection, Clip, ClipError, Read, ReadLimits, Selection, Want};

/// The largest picture the window takes from the clipboard (the host takes the same).
pub(crate) const IMAGE_MAX: usize = 3 * 1024 * 1024;
/// The most bytes of pictures the window holds in all: the chips of all sessions and the pictures on their way.
pub(crate) const MEMORY_MAX: usize = 6 * 1024 * 1024;
/// The whole read of a picture, from the connection to the last chunk.
const READ_TIME: Duration = Duration::from_secs(2);

const ONLY_CLAUDE: &str = "Pictures can be pasted into Claude sessions only.";
const REPLACED: &str = "The picture was replaced.";
const ANOTHER_WAITS: &str = "Send or remove the waiting picture of another session first.";
const TOO_LARGE: &str = "Paste refused: the picture is over 3 MiB or 8000 pixels on a side.";
const BAD_IMAGE: &str = "Paste refused: that is not a usable PNG picture.";
const FORMAT: &str = "Nothing to paste: only text and PNG pictures can be pasted.";
const TIMEOUT: &str = "Paste: the owner of the selection did not answer.";
const NOTHING: &str = "Nothing to paste.";
const NEEDS_TEXT: &str = "Type a line to go with the picture.";
const NOT_RUNNING: &str = "Reconnect the session first: send a line without the picture, then paste it again.";
const BUSY: &str = "Too many pictures are waiting, try again in a moment.";
const NO_PLACE: &str = "The host could not make a place for the picture.";
const NOT_WRITTEN: &str = "The picture could not be written to the host's folder.";
const GONE_SESSION: &str = "The session is gone; the picture could not be sent.";
const NEWER: &str = "A newer picture was pasted meanwhile, so this one was dropped.";

/// What the paste thread found: the text of the selection, (only in the box of a session, only from the CLIPBOARD, only when there was no text) a picture, or a screen capture.
pub(crate) enum Pasted {
    Text(Result<String, &'static str>),
    Picture(Result<Read, ClipError>),
    Captured(Result<Picture, &'static str>),
}

/// The paste thread's work on `display` (`None`: `$DISPLAY`): the text of `selection` first, exactly as before; ONLY when that answers `empty` and `image` is
/// set, the CLIPBOARD is asked for a PNG. So a copy that carries a text and a picture pastes as TEXT, and Shift+Insert (PRIMARY) never asks for a picture.
pub(super) fn fetch_pasted(display: Option<&str>, selection: &[u8], limit: usize, image: bool) -> Pasted {
    let limits = ReadLimits { text_max: 0, image_max: IMAGE_MAX, deadline: READ_TIME };
    // Some owners (xclip) answer a request for text with whatever they hold, whatever their list of targets says. So when a picture is allowed the list is
    // looked at first: an owner that lists targets and no text offers no text, and the text is not asked for at all.
    let lists_no_text = image && matches!(read_selection(display, Selection::Clipboard, Want::Text, &limits), Err(e) if e.reason == "unsupported_format");
    if !lists_no_text {
        let text = fetch_selection(display, selection, limit);
        if !image || text != Err("empty") {
            return Pasted::Text(text);
        }
    }
    Pasted::Picture(read_selection(display, Selection::Clipboard, Want::Image, &limits))
}

/// A picture exactly as read (no copy is kept besides the one on its way).
pub(crate) struct Picture {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// What `put` did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Put {
    Added,
    Replaced,
    Full,
}

/// The chips by SESSION id (a chip stays with its session when another one is opened), and the pictures on their way by the id they wait under.
#[derive(Default)]
pub(crate) struct Pictures {
    chips: HashMap<String, Picture>,
    flying: HashMap<String, (String, Picture)>,
}

impl Pictures {
    /// The bytes held, chips and pictures on their way together.
    pub(crate) fn held(&self) -> usize {
        self.chips.values().map(|p| p.bytes.len()).sum::<usize>() + self.flying.values().map(|(_, p)| p.bytes.len()).sum::<usize>()
    }

    /// The chip of `session`: a second picture replaces the first; one that would take the window past 6 MiB is refused whole.
    pub(crate) fn put(&mut self, session: &str, picture: Picture) -> Put {
        let old = self.chips.get(session).map_or(0, |p| p.bytes.len());
        if self.held() - old + picture.bytes.len() > MEMORY_MAX {
            return Put::Full;
        }
        if self.chips.insert(session.to_string(), picture).is_some() { Put::Replaced } else { Put::Added }
    }

    pub(crate) fn has(&self, session: &str) -> bool {
        self.chips.contains_key(session)
    }

    /// The chip of `session` removed (and its memory freed): true when there was one.
    pub(crate) fn remove(&mut self, session: &str) -> bool {
        self.chips.remove(session).is_some()
    }

    /// The session is gone: its chip and its pictures on their way go with it.
    pub(crate) fn forget(&mut self, session: &str) {
        self.chips.remove(session);
        self.flying.retain(|_, (owner, _)| owner != session);
    }

    /// The chip of `session` goes on its way under `id`.
    pub(crate) fn launch(&mut self, session: &str, id: &str) -> bool {
        let Some(picture) = self.chips.remove(session) else { return false };
        self.flying.insert(id.to_string(), (session.to_string(), picture));
        true
    }

    /// The picture on its way under `from` waits under `to` from now on (the id of the slot becomes the request id of the input).
    pub(crate) fn rekey(&mut self, from: &str, to: &str) {
        if let Some(entry) = self.flying.remove(from) {
            self.flying.insert(to.to_string(), entry);
        }
    }

    pub(crate) fn bytes_of(&self, id: &str) -> Option<&[u8]> {
        self.flying.get(id).map(|(_, p)| p.bytes.as_slice())
    }

    /// The picture that waited under `id`, with its session; the memory is free again unless the caller puts it back.
    pub(crate) fn take_flying(&mut self, id: &str) -> Option<(String, Picture)> {
        self.flying.remove(id)
    }

    /// A refused picture goes back to its session; false (and the picture is dropped) when that session has a newer chip by now.
    pub(crate) fn restore(&mut self, session: &str, picture: Picture) -> bool {
        if self.chips.contains_key(session) {
            return false;
        }
        self.chips.insert(session.to_string(), picture);
        true
    }

    /// The host was replaced: what was on its way is let go (the host's own rules remove the files it was told about).
    pub(crate) fn let_go(&mut self) {
        self.flying.clear();
    }

    /// The size as the host writes it in the transcript: whole KiB rounded to the nearest, at least 1.
    pub(crate) fn kib(bytes: usize) -> usize {
        ((bytes + 512) / 1024).max(1)
    }

    /// The line under the box while the session has a chip.
    pub(crate) fn chip_line(&self, session: &str) -> Option<String> {
        let p = self.chips.get(session)?;
        Some(format!("[picture {} KiB, {}x{}] - Esc removes it", Self::kib(p.bytes.len()), p.width, p.height))
    }

    /// `picture` (the size, never the pixels) and `chip` (the line) of the dump.
    pub(crate) fn dump(&self, session: &str) -> (Value, Value) {
        match self.chips.get(session) {
            Some(p) => (json!({"bytes": p.bytes.len(), "width": p.width, "height": p.height}), json!(self.chip_line(session))),
            None => (Value::Null, Value::Null),
        }
    }
}

/// The sentence for a picture the clipboard could not give.
fn refusal(error: &ClipError) -> &'static str {
    match error.reason {
        "timeout" => TIMEOUT,
        "too_large" => TOO_LARGE,
        "bad_image" => BAD_IMAGE,
        "unsupported_format" => FORMAT,
        _ => NOTHING, // empty, no_display, x_error
    }
}

impl App {
    /// The answer of the paste or screen-capture thread: a text goes the old way, a picture becomes the chip of the open session.
    pub(crate) fn on_paste_answer(&mut self, answer: Pasted) {
        match answer {
            Pasted::Text(result) => self.on_paste(result),
            Pasted::Picture(result) => self.on_picture_paste(result),
            Pasted::Captured(result) => self.on_captured_picture(result),
        }
    }

    pub(crate) fn capture_input_reason(&self) -> Option<&'static str> {
        if self.session.is_empty() {
            Some("no session is open")
        } else if !self.preedit.is_empty() {
            Some("finish composing text first")
        } else if self.ui.paste_pending {
            Some("a picture is already being read")
        } else if self.row_of(&self.session).map(|r| r.provider.as_str()) != Some("claude") {
            Some("only Claude sessions take pictures")
        } else {
            None
        }
    }

    /// Captures the whole screen off the UI thread and puts the shrunk PNG into the open Claude session's chip.
    pub(crate) fn capture_to_input(&mut self) {
        if self.ui.panel.is_some() || self.session.is_empty() || !self.preedit.is_empty() || self.ui.paste_pending {
            return;
        }
        if self.row_of(&self.session).map(|r| r.provider.as_str()) != Some("claude") {
            self.ed.say(ONLY_CLAUDE);
            self.dirty = true;
            return;
        }
        let Some(proxy) = self.ui.proxy.clone() else { return };
        self.ui.paste_pending = true;
        std::thread::spawn(move || {
            let result = crate::capture::screen_png(None)
                .map(|(bytes, width, height)| Picture { bytes, width, height })
                .map_err(|e| if e.reason == "too_large" { "Capture refused: the screen picture is over 3 MiB." } else { "Could not capture the screen." });
            let _ = proxy.send_event(super::Ev::Paste(Pasted::Captured(result)));
        });
    }

    fn on_captured_picture(&mut self, result: Result<Picture, &'static str>) {
        self.ui.paste_pending = false;
        if !self.paste_allowed() || self.ui.panel.is_some() {
            return;
        }
        let session = self.session.clone();
        let said = match result {
            Err(why) => Some(why),
            Ok(pic) => {
                if self.row_of(&session).map(|r| r.provider.as_str()) != Some("claude") {
                    Some(ONLY_CLAUDE)
                } else {
                    match self.ed.pictures.put(&session, pic) {
                        Put::Added => None,
                        Put::Replaced => Some(REPLACED),
                        Put::Full => Some(ANOTHER_WAITS),
                    }
                }
            }
        };
        if let Some(line) = said {
            self.ed.say(line);
        }
        self.dirty = true;
    }

    fn row_of(&self, session: &str) -> Option<&super::ui::Row> {
        self.ui.rows.iter().find(|r| r.session == session)
    }

    fn on_picture_paste(&mut self, result: Result<Read, ClipError>) {
        self.ui.paste_pending = false;
        if !self.paste_allowed() || self.ui.panel.is_some() {
            return; // the person moved on while the owner was answering
        }
        let session = self.session.clone();
        let said = match result {
            Err(error) => Some(refusal(&error)),
            Ok(Read { clip: Clip::Png { bytes, width, height }, .. }) => {
                if self.row_of(&session).map(|r| r.provider.as_str()) != Some("claude") {
                    Some(ONLY_CLAUDE)
                } else {
                    match self.ed.pictures.put(&session, Picture { bytes, width, height }) {
                        Put::Added => None,
                        Put::Replaced => Some(REPLACED),
                        Put::Full => Some(ANOTHER_WAITS),
                    }
                }
            }
            Ok(_) => Some(NOTHING), // a text cannot come from a read that asked for a picture
        };
        if let Some(line) = said {
            self.ed.say(line);
        }
        self.dirty = true;
    }

    /// Escape: the chip of the open session is removed (the text in the box stays). True when there was one.
    pub(crate) fn drop_chip(&mut self) -> bool {
        !self.session.is_empty() && self.ed.pictures.remove(&self.session)
    }

    /// The rows of the input box with the line under it; the chip is shown there while the box has no other line to say.
    pub(crate) fn chip_prompt(&mut self, cols: usize, rows: usize) -> Prompt {
        let mut prompt = self.ed.prompt(cols, rows, &self.preedit);
        if prompt.note.is_none() {
            prompt.note = self.ed.pictures.chip_line(&self.session);
        }
        prompt
    }

    /// The editor's dump with `picture` and `chip` added.
    pub(crate) fn dump_with_chip(&self) -> Value {
        let mut dump = self.ed.dump(&self.session);
        let (picture, chip) = self.ed.pictures.dump(&self.session);
        (dump["picture"], dump["chip"]) = (picture, chip);
        dump
    }

    // ------------------------------------------------------------------ Enter with a chip
    /// Enter in the box of a session that has a chip. False: no chip, the ordinary path. True: the key was taken here (nothing sent for an empty box or a
    /// session that is not running; otherwise the text waits for a place for the picture).
    pub(crate) fn picture_send(&mut self) -> bool {
        if !self.ed.pictures.has(&self.session) {
            return false;
        }
        self.dirty = true;
        if self.ed.text().trim().is_empty() {
            self.ed.say(NEEDS_TEXT);
            return true;
        }
        if matches!(self.selected_row().map(|r| r.state.as_str()), Some("disconnected" | "exited")) {
            self.ed.say(NOT_RUNNING);
            return true;
        }
        let Some(text) = self.ed.prepare_send(&self.session) else { return true }; // over the limit: kept, with the reason under the box
        self.sent_count += 1;
        let id = format!("slot-{}-{}", std::process::id(), self.sent_count);
        self.ed.pictures.launch(&self.session, &id);
        emit_ime("sent", &text);
        emit_ime("text", self.ed.text());
        self.pending_inputs.insert(id.clone(), (self.session.clone(), text));
        self.send(&format!("{{\"op\":\"picture.slot\",\"id\":\"{id}\"}}"));
        true
    }

    /// The host's answer to `picture.slot`: write the picture into the place and send the `input` that names it, or give text and chip back.
    pub(crate) fn on_picture_reply(&mut self, line: &str) -> bool {
        let Some(id) = jstr(line, "id").filter(|id| id.starts_with("slot-")) else { return false };
        let id = id.to_string();
        let Some((session, text)) = self.pending_inputs.remove(&id) else {
            self.ed.pictures.take_flying(&id); // the session was deleted, or the host replaced: nothing waits for this place
            return true;
        };
        match self.place_picture(line, &id, &session) {
            Ok((name, generation)) => {
                let request_id = self.send_input_with(&session, generation, text, false, Some(&name));
                self.ed.pictures.rekey(&id, &request_id);
            }
            Err(why) => {
                let why = self.chip_back(&id, &session, why.to_string());
                self.give_back_to(&session, &text, why);
            }
        }
        self.dirty = true;
        true
    }

    /// The picture written into the place the host made: the name for the `input` and the generation to aim it at, or the sentence for what went wrong.
    fn place_picture(&self, line: &str, id: &str, session: &str) -> Result<(String, i64), &'static str> {
        if !line.contains("\"ok\":1") {
            return Err(if jstr(line, "error") == Some("busy") { BUSY } else { NO_PLACE });
        }
        let (Some(name), Some(path)) = (jstr(line, "name"), jstr(line, "path")) else { return Err(NO_PLACE) };
        let shaped = name.len() == 36 && name.ends_with(".png") && name[..32].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !shaped || Path::new(path).file_name().and_then(|n| n.to_str()) != Some(name) {
            return Err(NO_PLACE); // the name is what the host will look for in its own folder; a shape the host would refuse is not sent
        }
        let generation = self.row_of(session).map(|r| r.generation).ok_or(GONE_SESSION)?;
        let bytes = self.ed.pictures.bytes_of(id).ok_or(NO_PLACE)?;
        open::write_slot(Path::new(path), bytes).map_err(|_| NOT_WRITTEN)?;
        Ok((name.to_string(), generation))
    }

    /// The picture that waited under `id` goes back to `session` as its chip; `why` gets the sentence about a newer chip when that could not be done.
    fn chip_back(&mut self, id: &str, session: &str, why: String) -> String {
        let Some((_, picture)) = self.ed.pictures.take_flying(id) else { return why };
        if self.ed.pictures.restore(session, picture) { why } else { format!("{why} {NEWER}") }
    }

    /// The `input` was taken: the picture it carried is dropped from memory (the host removed the file).
    pub(crate) fn input_taken(&mut self, id: &str) {
        self.pending_inputs.remove(id);
        self.ed.pictures.take_flying(id);
    }

    /// The `input` was refused: what the person is told, and the picture it carried goes back to its session as its chip.
    pub(crate) fn input_refused(&mut self, id: &str, session: &str, error: &str) -> String {
        let said = match error {
            "attachment_gone" => "The picture file vanished; paste it again.".to_string(),
            "attachment_too_large" => "The picture is over 3 MiB.".to_string(),
            "bad_image" => "That is not a usable PNG picture.".to_string(),
            "attachment_unsupported" => "This session cannot take pictures.".to_string(),
            _ => input_note(id, error),
        };
        self.chip_back(id, session, said)
    }

    /// The host was replaced while a text waited for a place for its picture: both come back.
    pub(crate) fn slot_replaced(&mut self, id: &str, session: &str) -> String {
        self.chip_back(id, session, "Not sent: the host was replaced while the picture was being sent. The text and the picture are back in the box.".into())
    }
}

#[cfg(test)]
#[path = "picture_tests.rs"]
mod tests;
