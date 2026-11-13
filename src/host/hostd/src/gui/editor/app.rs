//! `App` integration for `Editor`: keys (`Shift` selection, `Ctrl+C`/`Ctrl+X`, `Ctrl+Z`/`Ctrl+Y`), mouse drag, paste, and send.

use std::thread;

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::selection::Move;
use super::super::picture::fetch_pasted;
use super::super::{emit_ime, jstr, App, Ev};
use super::{Prompt, MAX_ROWS, PASTE_FETCH};

impl App {
    /// The editor's part of the dump. While a panel owns the line (a sign-in code is typed or pasted into it) the dump has the length and not the text.
    pub(crate) fn editor_json(&self) -> Value {
        let mut dump = self.dump_with_chip();
        if self.ui.panel.is_some() {
            (dump["text"], dump["cursor"], dump["selection"], dump["selection_range"], dump["len"]) =
                (Value::Null, Value::Null, Value::Null, Value::Null, json!(self.ed.text().len()));
        }
        dump
    }

    /// What the prompt of this frame is: the rows of the input box with the line under it, or the one line of a panel (a sign-in code is never drawn).
    pub(crate) fn prompt_view(&mut self, cols: usize, area_rows: usize) -> Prompt {
        let line = |text: String| Prompt { rows: vec![text], note: None, caret: None };
        if self.code_entry() {
            line(format!("> {}", self.shown_input()))
        } else if self.ui.panel.is_some() || !self.has_prompt_target() {
            line(format!("> {}[{}]", self.ed.text(), self.preedit))
        } else {
            self.chip_prompt(cols, (area_rows.saturating_sub(6) / 2).clamp(1, MAX_ROWS))
        }
    }

    /// True when either conversation/panel lines or prompt editor text are selected and copyable.
    pub(crate) fn can_copy(&self) -> bool {
        self.selection_text().is_some() || (self.ui.panel.is_none() && self.ed.has_selection())
    }

    pub(crate) fn can_cut(&self) -> bool {
        self.ui.panel.is_none() && self.has_prompt_target() && self.preedit.is_empty() && self.ed.has_selection()
    }

    /// Copies conversation lines when selected; otherwise copies an active prompt editor selection.
    pub(crate) fn copy_selection(&mut self) {
        if let Some(text) = self.selection_text() {
            let (n, before) = (text.bytes().filter(|&b| b == b'\n').count() + 1, self.ui.copied);
            self.copy_text(text);
            self.ui.flash = Some(if self.ui.copied > before {
                format!("Copied {n} {}.", if n == 1 { "line" } else { "lines" })
            } else {
                "Copy refused: more than 64 KiB selected.".into()
            });
            self.dirty = true;
            return;
        }
        if self.ui.panel.is_none() {
            if let Some(text) = self.ed.selected_text() {
                let before = self.ui.copied;
                self.copy_text(text);
                self.ui.flash = Some(if self.ui.copied > before {
                    "Copied selection.".into()
                } else {
                    "Copy refused: more than 64 KiB selected.".into()
                });
                self.dirty = true;
            }
        }
    }

    /// Cuts the active prompt editor selection onto `CLIPBOARD` without losing text if over the single-transfer limit.
    pub(crate) fn cut_editor_selection(&mut self) -> bool {
        if !self.can_cut() {
            return false;
        }
        let Some(text) = self.ed.selected_text() else { return false };
        if text.len() > super::super::ui::COPY_LIMIT {
            self.ui.flash = Some("Cut refused: more than 60 KiB selected.".into());
            self.dirty = true;
            return true;
        }
        let before = self.ui.copied;
        self.copy_text(text);
        if self.ui.copied > before && self.ed.cut_selection().is_some() {
            emit_ime("text", self.ed.text());
            self.ui.flash = Some("Cut selection.".into());
        }
        self.dirty = true;
        true
    }

    pub(crate) fn ed_mouse_press(&mut self, x: f32, y: f32) -> bool {
        if self.ui.panel.is_some() || !self.has_prompt_target() || !self.preedit.is_empty() || !self.ed.contains(x, y) {
            return false;
        }
        self.ui.sel = None;
        self.ui.dragging = false;
        self.ui.drag_origin = None;
        self.ui.drag_moved = false;
        if self.ed.mouse_press(x, y, self.ui.mods.shift_key()) {
            self.dirty = true;
        }
        true
    }

    pub(crate) fn ed_mouse_drag(&mut self, x: f32, y: f32) -> bool {
        if !self.ed.selecting {
            return false;
        }
        if self.ed.mouse_drag(x, y) {
            self.dirty = true;
        }
        true
    }

    /// What the selection's owner sent, for the input box of a session: all of it, normalised, or the reason it was not taken.
    pub(crate) fn box_paste(&mut self, result: Result<String, &'static str>) {
        match result {
            Ok(text) => self.ed.paste(&text),
            Err("too_large") => self.ed.say(format!("Paste refused: more than {} KiB, or sent in pieces.", PASTE_FETCH / 1024)),
            Err("timeout") => self.ed.say("Paste: the owner of the selection did not answer."),
            Err(_) => self.ed.say("Nothing to paste."),
        }
        self.dirty = true;
    }

    /// Where a paste may go: the input line of a session or the text entry of a panel, never during a composition.
    pub(crate) fn paste_allowed(&self) -> bool {
        self.preedit.is_empty() && if self.ui.panel.is_some() { self.text_mode() } else { self.has_prompt_target() }
    }

    /// Ctrl+V and Ctrl+Shift+V paste the clipboard, Shift+Insert the primary selection; only on the person's own key.
    pub(crate) fn paste_key(&mut self, event: &KeyEvent) -> bool {
        let (ctrl, shift) = (self.ui.mods.control_key(), self.ui.mods.shift_key());
        let selection: &'static [u8] = match &event.logical_key {
            _ if ctrl && crate::gui::key_letter(event) == Some('v') => b"CLIPBOARD",
            Key::Named(NamedKey::Insert) if shift && !ctrl => b"PRIMARY",
            _ => return false,
        };
        if !event.repeat {
            self.start_paste(selection);
        }
        true
    }

    /// Ask the owner of a selection for its text (from the paste keys and the menu); nothing happens where no line can take text.
    pub(crate) fn start_paste(&mut self, selection: &'static [u8]) {
        if !self.paste_allowed() || self.ui.paste_pending {
            return;
        }
        let Some(proxy) = self.ui.proxy.clone() else { return };
        self.ui.paste_pending = true;
        let (limit, image) = if self.ui.panel.is_some() { (8192, false) } else { (PASTE_FETCH, selection == b"CLIPBOARD") };
        thread::spawn(move || {
            let _ = proxy.send_event(Ev::Paste(fetch_pasted(None, selection, limit, image)));
        });
    }

    /// What the selection's owner sent: it goes onto the input line (never echoed, never submitted).
    pub(crate) fn on_paste(&mut self, result: Result<String, &'static str>) {
        self.ui.paste_pending = false;
        if !self.paste_allowed() {
            return;
        }
        if self.ui.panel.is_none() {
            return self.box_paste(result);
        }
        match result {
            Ok(text) => {
                let line: String = text.split(|c| c == '\n' || c == '\r').next().unwrap_or("").chars().filter(|c| !c.is_control()).collect();
                if line.is_empty() {
                    self.paste_notice("Nothing to paste.");
                } else {
                    self.ed.put(&line);
                }
            }
            Err("too_large") => self.paste_notice("Paste refused: more than 8 KiB."),
            Err("timeout") => self.paste_notice("Paste: the owner of the selection did not answer."),
            Err(_) => self.paste_notice("Nothing to paste."),
        }
        self.dirty = true;
    }

    /// The keys of the input box, for a session that is open and no composition in progress (the input method owns the keys then).
    /// Note: `Ctrl+A` is intentionally never bound here — `panel_key` owns `Ctrl+A` for `Accounts`.
    pub(crate) fn edit_key(&mut self, event: &KeyEvent) -> bool {
        if self.ui.panel.is_some() || !self.has_prompt_target() || !self.preedit.is_empty() || self.ui.mods.alt_key() {
            return false;
        }
        let (ctrl, shift) = (self.ui.mods.control_key(), self.ui.mods.shift_key());
        let letter = crate::gui::key_letter(event);
        if ctrl && !shift && letter == Some('x') {
            if !event.repeat {
                let _ = self.cut_editor_selection();
            }
            return true;
        }
        let session = self.prompt_target_id();
        let ed = &mut self.ed;
        let (stamp, empty, note) = (ed.stamp, ed.is_empty(), ed.note.take());
        let handled = match &event.logical_key {
            Key::Named(NamedKey::Enter) if shift && !ctrl => ed.put("\n") || true,
            Key::Named(NamedKey::ArrowLeft) => {
                ed.move_cursor(shift, if ctrl { Move::WordLeft } else { Move::Left });
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                ed.move_cursor(shift, if ctrl { Move::WordRight } else { Move::Right });
                true
            }
            Key::Named(NamedKey::Home) if !ctrl && !empty => {
                ed.move_cursor(shift, Move::Home);
                true
            }
            Key::Named(NamedKey::End) if !ctrl && !empty => {
                ed.move_cursor(shift, Move::End);
                true
            }
            Key::Named(NamedKey::ArrowUp) if !ctrl => {
                if shift {
                    ed.move_cursor(true, Move::Up);
                } else {
                    ed.anchor = None;
                    ed.typing_run = false;
                    if !(ed.on_first_row() && ed.recall_older(&session)) && !ed.vertical(false) {
                        (ed.cursor, ed.goal) = (0, None);
                    }
                }
                true
            }
            Key::Named(NamedKey::ArrowDown) if !ctrl => {
                if shift {
                    ed.move_cursor(true, Move::Down);
                } else {
                    ed.anchor = None;
                    ed.typing_run = false;
                    if !(ed.on_last_row() && ed.recall_newer(&session)) && !ed.vertical(true) {
                        (ed.cursor, ed.goal) = (ed.text.len(), None);
                    }
                }
                true
            }
            Key::Named(NamedKey::Delete) if !ctrl => ed.delete() || true,
            Key::Named(NamedKey::Backspace) if ctrl => ed.delete_word_back() || true,
            _ if ctrl && !shift && letter == Some('k') => ed.kill_to_end() || true,
            _ if ctrl && !shift && letter == Some('u') => ed.clear() || true,
            _ if ctrl && letter == Some('z') => {
                if !event.repeat {
                    if shift {
                        ed.redo();
                    } else {
                        ed.undo();
                    }
                }
                true
            }
            _ if ctrl && !shift && letter == Some('y') => {
                if !event.repeat {
                    ed.redo();
                }
                true
            }
            _ => false,
        };
        if !handled {
            ed.note = note;
            return false;
        }
        if self.ed.stamp != stamp {
            emit_ime("text", self.ed.text());
        }
        self.dirty = true;
        true
    }

    /// Ctrl+U clears the whole input line (and only that); true when it did.
    pub(crate) fn ui_ctrl_u(&mut self, event: &KeyEvent) -> bool {
        if self.ui.mods.control_key() && crate::gui::key_letter(event) == Some('u') && self.preedit.is_empty() {
            self.ed.clear();
            true
        } else {
            false
        }
    }

    /// The keys that reach the box last: Enter sends, Backspace removes a character/selection, Ctrl+U clears, anything else that is text goes in.
    pub(crate) fn typed_key(&mut self, event: &KeyEvent) {
        if event.state != winit::event::ElementState::Pressed {
            return;
        }
        let key = &event.logical_key;
        if *key == Key::Named(NamedKey::Enter) {
            self.send_draft();
        } else if *key == Key::Named(NamedKey::Backspace) {
            if self.preedit.is_empty() && self.ed.backspace() {
                emit_ime("text", self.ed.text());
                self.dirty = true;
            }
        } else if self.ui_ctrl_u(event) {
            emit_ime("text", self.ed.text());
            self.dirty = true;
        } else if let Some(text) = &event.text {
            if !text.chars().any(char::is_control) {
                self.ed.put(text);
                emit_ime("text", self.ed.text());
                self.dirty = true;
            }
        }
    }

    /// Send the draft of the visible session as one input with its own request id; `/exit` and `/quit` dismiss the tab locally instead of sending to the agent.
    pub(crate) fn send_draft(&mut self) {
        if let Some(cid) = self.ui.calls.active_call.clone() {
            if matches!(self.ed.text().trim(), "/exit" | "/quit") {
                self.ed.clear();
                emit_ime("text", "");
                self.close_call_tab(&cid);
                self.dirty = true;
                return;
            }
            let Some(text) = self.ed.prepare_send(&cid) else {
                self.dirty = true;
                return;
            };
            self.send_call_say(&cid, text);
            emit_ime("text", self.ed.text());
            self.dirty = true;
            return;
        }
        if self.session.is_empty() || self.picture_send() {
            return;
        }
        if matches!(self.ed.text().trim(), "/exit" | "/quit") {
            let sid = self.session.clone();
            self.ed.clear();
            emit_ime("text", "");
            self.close_tab_by_id(sid);
            self.dirty = true;
            return;
        }
        if self.reconnect_instead() {
            return;
        }
        let Some(text) = self.ed.prepare_send(&self.session) else {
            self.dirty = true;
            return;
        };
        let (session, generation) = (self.session.clone(), self.generation);
        self.send_input(&session, generation, text.clone(), false);
        emit_ime("sent", &text);
        emit_ime("text", self.ed.text());
        self.dirty = true;
    }

    /// A reply to one of our inputs carries its request id: a refused input returns to its draft, and a line says what the host said.
    pub(crate) fn on_input_reply(&mut self, line: &str) -> bool {
        let Some(id) = jstr(line, "id").filter(|id| id.starts_with("gui-")) else { return false };
        let id = id.to_string();
        if line.contains("\"ok\":0") {
            if let Some((session, text)) = self.pending_inputs.remove(&id) {
                let note = self.input_refused(&id, &session, jstr(line, "error").unwrap_or("refused"));
                self.give_back_to(&session, &text, note);
                self.dirty = true;
            }
        } else {
            self.input_taken(&id);
        }
        true
    }
}
