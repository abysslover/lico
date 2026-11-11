//! Protocol and state shared by the toolkit experiments of M01-P07a.
//!
//! Every experiment client speaks the same command language on standard input (`show ID [CURSOR]`, `hide`, `status`,
//! `load`, `hoststatus`, `text`, `quit`) and prints one JSON object per line on standard output, exactly like the clients
//! of M01-P01, so the unchanged measurement rig in `src/lico_measure` can drive it. Only the drawing differs per toolkit.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;

use serde_json::Value;

pub const VISIBLE_LINES: usize = 40;
pub const MAX_LINE_CHARS: usize = 120;
/// Neutral colour of both marker blocks while no session view is shown.
pub const NEUTRAL_RGB: u32 = 0x20_2020;
/// The measurement markers are two blocks of 16 x 16 physical pixels at the top left of the window.
pub const MARKER_PHYSICAL_PX: f32 = 16.0;

pub enum Msg {
    Sock(String),
    Cmd(String),
}

pub enum Command {
    None,
    Quit,
    /// Print the current text of the entry line as an `ime` event of kind `text`.
    Text,
}

pub struct Core {
    sock: UnixStream,
    pub sessions: Vec<String>,
    cursors: Vec<i64>,
    pub session: String,
    pub cursor: i64,
    pub subscribed: bool,
    pub lines: VecDeque<String>,
    pub view_resident: bool,
    pub session_rgb: u32,
    pub seq_rgb: u32,
    pub dups: i64,
    pub gaps: i64,
    pub resyncs: i64,
    print_status: bool,
    /// Set whenever something visible changed; the toolkit clears it when it redraws.
    pub dirty: bool,
}

pub fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn out_line(line: &str) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let _ = writeln!(lock, "{line}");
    let _ = lock.flush();
}

pub fn emit_ready() {
    out_line(&format!("{{\"ev\":\"ready\",\"pid\":{}}}", std::process::id()));
}

pub fn emit_ime(kind: &str, text: &str) {
    out_line(&format!("{{\"ev\":\"ime\",\"kind\":\"{}\",\"text\":\"{}\"}}", kind, json_escape(text)));
}

/// Free-form observation of a toolkit (menu opened, accessibility state, ...), ignored by the measurement rig.
pub fn emit_note(kind: &str, text: &str) {
    out_line(&format!("{{\"ev\":\"note\",\"kind\":\"{}\",\"text\":\"{}\"}}", kind, json_escape(text)));
}

pub fn arg_value(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

/// Connect to the host and start the two reader threads (host socket, standard input). `deliver` is called from those
/// threads and must hand the message to the toolkit's event loop and wake it.
pub fn start<F>(socket_path: &str, deliver: F) -> Core
where
    F: Fn(Msg) + Send + Sync + 'static,
{
    let sock = UnixStream::connect(socket_path).expect("connect to host");
    let reader = sock.try_clone().expect("clone socket");
    let deliver = std::sync::Arc::new(deliver);
    let d1 = deliver.clone();
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            d1(Msg::Sock(line));
        }
    });
    thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            deliver(Msg::Cmd(line));
        }
        deliver(Msg::Cmd("quit".into())); // the measurement client ends with its input
    });
    Core {
        sock, sessions: Vec::new(), cursors: Vec::new(), session: String::new(), cursor: -1, subscribed: false,
        lines: VecDeque::new(), view_resident: false, session_rgb: NEUTRAL_RGB, seq_rgb: NEUTRAL_RGB,
        dups: 0, gaps: 0, resyncs: 0, print_status: false, dirty: true,
    }
}

impl Core {
    fn send(&mut self, line: &str) {
        let _ = self.sock.write_all(line.as_bytes());
        let _ = self.sock.write_all(b"\n");
    }

    pub fn emit_state(&self, why: &str) {
        out_line(&format!(
            "{{\"ev\":\"state\",\"why\":\"{}\",\"session\":\"{}\",\"view_resident\":{},\"line_buffer_lines\":{},\"cursor\":{},\"subscribed\":{},\"dups\":{},\"gaps\":{},\"resyncs\":{},\"known_sessions\":{}}}",
            why, self.session, self.view_resident, self.lines.len(), self.cursor, self.subscribed, self.dups, self.gaps,
            self.resyncs, self.sessions.len()
        ));
    }

    pub fn request_status(&mut self) {
        self.send("{\"op\":\"status\"}");
    }

    pub fn hide(&mut self) {
        if self.subscribed && !self.session.is_empty() {
            let msg = format!("{{\"op\":\"unsubscribe\",\"session\":\"{}\"}}", self.session);
            self.send(&msg);
        }
        self.subscribed = false;
        if let Some(i) = self.sessions.iter().position(|s| *s == self.session) {
            self.cursors[i] = self.cursor; // navigation metadata survives; the view does not
        }
        self.lines = VecDeque::new(); // a dropped view releases its lines
        self.view_resident = false;
        self.session_rgb = NEUTRAL_RGB;
        self.seq_rgb = NEUTRAL_RGB;
        self.dirty = true;
    }

    pub fn show(&mut self, id: &str, after: Option<i64>) {
        self.hide();
        self.session = id.to_string();
        self.session_rgb = u32::from_str_radix(id.get(1..7).unwrap_or("0"), 16).unwrap_or(0) & 0xFF_FFFF;
        self.seq_rgb = 0;
        self.view_resident = true;
        let remembered = self.sessions.iter().position(|s| s == id).map(|i| self.cursors[i]).unwrap_or(-1);
        self.cursor = after.unwrap_or(remembered);
        let snapshot = if after.is_none() { ",\"snapshot\":true" } else { "" };
        let msg = format!("{{\"op\":\"subscribe\",\"session\":\"{id}\",\"after\":{}{snapshot}}}", self.cursor);
        self.send(&msg);
        self.dirty = true;
    }

    fn append_line(&mut self, seq: i64, text: &str) {
        let clipped: String = text.chars().take(MAX_LINE_CHARS).collect();
        self.lines.push_back(clipped);
        while self.lines.len() > VISIBLE_LINES {
            self.lines.pop_front();
        }
        self.seq_rgb = seq as u32 & 0xFF_FFFF;
        self.cursor = seq;
        self.dirty = true;
    }

    fn on_snapshot(&mut self, snap: &Value) {
        self.lines.clear();
        if let Some(lines) = snap.get("lines").and_then(Value::as_array) {
            for entry in lines {
                if let (Some(seq), Some(text)) =
                    (entry.get(0).and_then(Value::as_i64), entry.get(1).and_then(Value::as_str))
                {
                    self.append_line(seq, text);
                }
            }
        }
        if let Some(seq) = snap.get("seq").and_then(Value::as_i64) {
            self.cursor = seq;
            self.seq_rgb = seq as u32 & 0xFF_FFFF;
        }
        self.dirty = true;
    }

    /// Returns true when the list of sessions changed.
    pub fn on_sock_line(&mut self, line: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return false };
        match value.get("ev").and_then(Value::as_str) {
            Some("rec") => {
                if !self.view_resident || value.get("session").and_then(Value::as_str) != Some(self.session.as_str()) {
                    return false;
                }
                let Some(seq) = value.get("seq").and_then(Value::as_i64) else { return false };
                if seq <= self.cursor {
                    self.dups += 1;
                    return false;
                }
                if self.cursor >= 0 && seq != self.cursor + 1 {
                    self.gaps += 1;
                }
                if let Some(data) = value.get("data").and_then(Value::as_str) {
                    let text = format!("{seq:08} {data}");
                    self.append_line(seq, &text);
                }
                return false;
            }
            Some("overflow") => {
                self.subscribed = false;
                self.resyncs += 1;
                self.emit_state("overflow");
                let msg = format!(
                    "{{\"op\":\"subscribe\",\"session\":\"{}\",\"after\":{},\"snapshot\":true}}", self.session, self.cursor);
                self.send(&msg);
                return false;
            }
            _ => {}
        }
        if value.get("ok").and_then(Value::as_i64) == Some(1) && value.get("gap").is_some() {
            self.subscribed = true;
            if self.view_resident {
                if let Some(snap) = value.get("snapshot") {
                    self.on_snapshot(snap);
                }
            }
            self.emit_state("subscribed");
            return false;
        }
        if let Some(list) = value.get("sessions").and_then(Value::as_array) {
            self.sessions = list.iter().filter_map(|s| s.get("session").and_then(Value::as_str).map(String::from)).collect();
            self.cursors = vec![-1; self.sessions.len()];
            if self.print_status {
                out_line(line);
                self.print_status = false;
            }
            self.dirty = true;
            self.emit_state("loaded");
            return true;
        }
        false
    }

    pub fn on_command(&mut self, line: &str) -> Command {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("show") => {
                if let Some(id) = words.next() {
                    let after = words.next().and_then(|a| a.parse().ok());
                    self.show(id, after);
                }
            }
            Some("hide") => {
                self.hide();
                self.emit_state("hidden");
            }
            Some("status") => self.emit_state("status"),
            Some("text") => return Command::Text,
            Some("hoststatus") => {
                self.print_status = true;
                self.request_status();
            }
            Some("load") => self.request_status(),
            Some("quit") => return Command::Quit,
            _ => {}
        }
        Command::None
    }

    /// Short label of a session for the list: its position, then the start of its identifier.
    pub fn label(&self, index: usize) -> String {
        format!("Session {:03}  {}", index + 1, self.sessions[index].get(..7).unwrap_or(""))
    }
}
