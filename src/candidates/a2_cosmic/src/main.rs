//! Candidate A2: candidate A with a real text stack (cosmic-text: shaping, font fallback)
//! for the synthetic host. Commands arrive on stdin, see `handle_command`.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::num::NonZeroU32;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread;

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, WindowEvent};
use winit::keyboard::{Key, NamedKey};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const VISIBLE_LINES: usize = 40;
const LINE_HEX: usize = 88;
const FONT_PX: f32 = 14.0;
const LINE_HEIGHT: usize = 18;
const BACKGROUND: u32 = 0xFF10_1418;
const NEUTRAL: u32 = 0xFF20_2020;

enum Ev {
    Sock(String),
    Cmd(String),
}

struct View {
    lines: VecDeque<String>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    buffer: Buffer,
    swash: SwashCache,
    session_rgb: u32,
    seq_rgb: u32,
}

struct SessionMeta {
    id: String,
    cursor: i64,
}

struct App {
    stale_commit: Option<String>,
    view_id: String,
    generation: i64,
    drafts: HashMap<String, String>,
    sent_count: u64,
    input: String,
    preedit: String,
    dirty: bool,
    meta: Vec<SessionMeta>,
    pending_show: Option<(String, Option<i64>)>,
    print_status: bool,
    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    view: Option<View>,
    font_system: Option<FontSystem>,
    sock: UnixStream,
    session: String,
    cursor: i64,
    dups: i64,
    gaps: i64,
    resyncs: i64,
    subscribed: bool,
}

fn json_escape(s: &str) -> String {
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

fn emit_ime(kind: &str, text: &str) {
    println!("{{\"ev\":\"ime\",\"kind\":\"{}\",\"text\":\"{}\"}}", kind, json_escape(text));
    let _ = std::io::stdout().flush();
}

fn jint(s: &str, key: &str) -> Option<i64> {
    let pat = format!("\"{key}\":");
    let rest = &s[s.find(&pat)? + pat.len()..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn jstr<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\":\"");
    let rest = &s[s.find(&pat)? + pat.len()..];
    Some(&rest[..rest.find('"')?])
}

fn fill_rect(buf: &mut [u32], width: usize, x: usize, y: usize, size: usize, color: u32) {
    for row in y..y + size {
        for col in x..(x + size).min(width) {
            if let Some(px) = buf.get_mut(row * width + col) {
                *px = color;
            }
        }
    }
}

impl App {
    fn send(&mut self, line: &str) {
        let _ = self.sock.write_all(line.as_bytes());
        let _ = self.sock.write_all(b"\n");
    }

    fn emit_state(&self, why: &str) {
        println!(
            "{{\"ev\":\"state\",\"why\":\"{}\",\"session\":\"{}\",\"view_resident\":{},\"line_buffer_lines\":{},\"cursor\":{},\"subscribed\":{},\"dups\":{},\"gaps\":{},\"resyncs\":{},\"known_sessions\":{}}}",
            why, self.session, self.view.is_some(),
            self.view.as_ref().map_or(0, |v| v.lines.len()),
            self.cursor, self.subscribed, self.dups, self.gaps, self.resyncs, self.meta.len()
        );
        let _ = std::io::stdout().flush();
    }

    fn load_fonts(&mut self) {
        if self.font_system.is_none() {
            self.font_system = Some(FontSystem::new()); // scans the installed fonts
        }
    }

    fn present_neutral(&mut self) {
        // leave the compositor-visible window blank once the view is released
        if let (Some(window), Some(context)) = (&self.window, &self.context) {
            let size = window.inner_size();
            if let (Some(w), Some(h), Ok(mut surface)) =
                (NonZeroU32::new(size.width), NonZeroU32::new(size.height), Surface::new(context, window.clone()))
            {
                surface.resize(w, h).ok();
                if let Ok(mut buf) = surface.buffer_mut() {
                    buf.fill(NEUTRAL);
                    buf.present().ok();
                }
            }
        }
    }

    fn hide(&mut self) {
        if !self.session.is_empty() {
            // a composition in progress is committed to the session it was typed for
            let pending = std::mem::take(&mut self.preedit);
            let mut draft = std::mem::take(&mut self.input);
            draft.push_str(&pending);
            self.drafts.insert(self.session.clone(), draft);
            if !pending.is_empty() {
                // the input method keeps composing it (winit offers no reset without losing the
                // input context); the commit it sends at the next key belongs to the old session
                self.stale_commit = Some(pending);
            }
        }
        if !self.view_id.is_empty() {
            let msg = format!("{{\"op\":\"view.detach\",\"view\":\"{}\"}}", self.view_id);
            self.send(&msg);
            self.view_id.clear();
        } else if self.subscribed && !self.session.is_empty() {
            let msg = format!("{{\"op\":\"unsubscribe\",\"session\":\"{}\"}}", self.session);
            self.send(&msg);
        }
        self.subscribed = false;
        let cursor = self.cursor;
        if let Some(m) = self.meta.iter_mut().find(|m| m.id == self.session) {
            m.cursor = cursor; // navigation metadata survives; the view does not
        }
        if self.view.take().is_some() {
            self.present_neutral(); // the dropped view released its surface, glyph cache and lines
        }
    }

    fn show(&mut self, id: &str, after: Option<i64>) {
        let Some(window) = self.window.clone() else {
            self.pending_show = Some((id.to_string(), after)); // applied once the window exists
            return;
        };
        self.hide();
        self.load_fonts();
        let Some(context) = self.context.as_ref() else { return };
        let surface = Surface::new(context, window.clone()).expect("surface");
        let fs = self.font_system.as_mut().expect("fonts");
        let buffer = Buffer::new(fs, Metrics::new(FONT_PX, LINE_HEIGHT as f32));
        let rgb = 0xFF00_0000 | (u32::from_str_radix(id.get(1..7).unwrap_or("0"), 16).unwrap_or(0) & 0xFF_FFFF);
        self.view = Some(View {
            lines: VecDeque::new(), surface, buffer, swash: SwashCache::new(), session_rgb: rgb, seq_rgb: 0xFF00_0000,
        });
        self.session = id.to_string();
        let remembered = self.meta.iter().find(|m| m.id == id).map(|m| m.cursor).unwrap_or(-1);
        // an explicit cursor replays after it; otherwise ask for the latest screen
        self.cursor = after.unwrap_or(remembered);
        self.input = self.drafts.remove(id).unwrap_or_default();
        let msg = if after.is_none() {
            // a view is registered with the host and gets the latest screen first
            format!("{{\"op\":\"view.attach\",\"session\":\"{id}\",\"kind\":\"gui\",\"after\":{}}}", self.cursor)
        } else {
            format!("{{\"op\":\"subscribe\",\"session\":\"{id}\",\"after\":{}}}", self.cursor)
        };
        self.send(&msg);
        self.dirty = true;
    }

    fn append_line(&mut self, seq: i64, hex: &str) {
        let Some(view) = self.view.as_mut() else { return };
        let hex = &hex[..hex.len().min(LINE_HEX)];
        view.lines.push_back(format!("{seq:08} {hex}"));
        while view.lines.len() > VISIBLE_LINES {
            view.lines.pop_front();
        }
        view.seq_rgb = 0xFF00_0000 | (seq as u32 & 0xFF_FFFF);
        self.cursor = seq;
        self.dirty = true; // drawn once when the event queue is drained
    }

    /// Send the draft of the visible session as one input with its own request id.
    fn send_draft(&mut self) {
        if self.session.is_empty() || self.input.is_empty() {
            return;
        }
        self.sent_count += 1;
        let request_id = format!("gui-{}-{}", std::process::id(), self.sent_count);
        let text = std::mem::take(&mut self.input);
        let msg = format!(
            "{{\"op\":\"input\",\"session\":\"{}\",\"generation\":{},\"request_id\":\"{}\",\"text\":\"{}\"}}",
            self.session, self.generation, request_id, json_escape(&text)
        );
        self.send(&msg);
        emit_ime("sent", &text);
        emit_ime("text", &self.input);
        self.dirty = true;
    }

    fn on_snapshot_lines(&mut self, reply: &str) {
        let Some(start) = reply.find("\"lines\":[") else { return };
        let mut rest = &reply[start + 9..];
        while let Some(open) = rest.find('[') {
            let body = &rest[open + 1..];
            let Some(comma) = body.find(',') else { break };
            let Ok(seq) = body[..comma].parse::<i64>() else { break };
            let Some(q1) = body.find('"') else { break };
            let Some(q2) = body[q1 + 1..].find('"') else { break };
            let text = &body[q1 + 1..q1 + 1 + q2];
            self.append_line(seq, text.get(9..).unwrap_or(""));
            rest = &body[q1 + 1 + q2..];
        }
    }

    fn on_status(&mut self, line: &str) {
        let mut rest = line;
        let mut meta = Vec::new();
        while let Some(i) = rest.find("\"session\":\"") {
            let after = &rest[i + 11..];
            let Some(end) = after.find('"') else { break };
            meta.push(SessionMeta { id: after[..end].to_string(), cursor: -1 });
            rest = &after[end..];
        }
        self.meta = meta;
    }

    fn on_sock_line(&mut self, line: &str) {
        match jstr(line, "ev") {
            Some("rec") => {
                if self.view.is_none() || jstr(line, "session") != Some(self.session.as_str()) {
                    return;
                }
                let Some(seq) = jint(line, "seq") else { return };
                if seq <= self.cursor {
                    self.dups += 1;
                    return;
                }
                if self.cursor >= 0 && seq != self.cursor + 1 {
                    self.gaps += 1;
                }
                if let Some(data) = jstr(line, "data") {
                    self.append_line(seq, data);
                }
            }
            Some("overflow") => {
                // the host dropped this stream: fetch a fresh snapshot instead of guessing
                self.subscribed = false;
                self.resyncs += 1;
                self.emit_state("overflow");
                let msg = format!("{{\"op\":\"subscribe\",\"session\":\"{}\",\"after\":{},\"snapshot\":true}}", self.session, self.cursor);
                self.send(&msg);
            }
            _ if line.contains("\"ok\":1") && line.contains("\"gap\":") => {
                self.subscribed = true;
                if let Some(view) = jstr(line, "view") {
                    self.view_id = view.to_string();
                }
                if let Some(generation) = jint(line, "generation") {
                    self.generation = generation;
                }
                if line.contains("\"snapshot\":") && self.view.is_some() {
                    if let Some(v) = self.view.as_mut() {
                        v.lines.clear();
                    }
                    self.on_snapshot_lines(line);
                    if let Some(snap) = line.find("\"snapshot\":").and_then(|i| jint(&line[i..], "seq")) {
                        self.cursor = snap;
                    }
                }
                self.emit_state("subscribed");
            }
            _ if line.contains("\"sessions\":[") => {
                self.on_status(line);
                if self.print_status {
                    println!("{line}");
                    let _ = std::io::stdout().flush();
                }
                self.print_status = false;
                self.emit_state("loaded");
            }
            _ => {}
        }
    }

    /// Draw a full page of distinct Hangul and then of distinct ideographs so the
    /// glyph caches reach a realistic worst case; the harness reads memory after.
    fn stress(&mut self) {
        for base in [0xAC00u32, 0x4E00u32] {
            let lines: VecDeque<String> = (0..VISIBLE_LINES)
                .map(|row| (0..59).filter_map(|col| char::from_u32(base + (row * 59 + col) as u32)).collect())
                .collect();
            let Some(view) = self.view.as_mut() else { return };
            view.lines = lines;
            self.draw();
        }
        self.emit_state("stressed");
    }

    fn handle_command(&mut self, line: &str, event_loop: &ActiveEventLoop) {
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
            Some("text") => emit_ime("text", &self.input),
            Some("stress") => self.stress(),
            Some("hoststatus") => {
                self.print_status = true;
                self.send("{\"op\":\"status\"}");
            }
            Some("load") => self.send("{\"op\":\"status\"}"),
            Some("quit") => event_loop.exit(),
            _ => {}
        }
    }

    fn draw(&mut self) {
        let prompt = format!("> {}[{}]", self.input, self.preedit);
        let (Some(window), Some(view), Some(fs)) = (&self.window, self.view.as_mut(), self.font_system.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        if view.surface.resize(w, h).is_err() {
            return;
        }
        let (width, height) = (w.get() as usize, h.get() as usize);
        let mut text = String::from("      \u{d55c}\u{ae00} Lico session view");
        for line in &view.lines {
            text.push('\n');
            text.push_str(line);
        }
        text.push('\n');
        text.push_str(&prompt);
        view.buffer.set_size(Some(width as f32), Some(height as f32));
        view.buffer.set_text(&text, &Attrs::new().family(Family::Monospace), Shaping::Advanced, None);
        view.buffer.shape_until_scroll(fs, true);
        let Ok(mut buf) = view.surface.buffer_mut() else { return };
        buf.fill(BACKGROUND);
        fill_rect(&mut buf, width, 0, 0, 16, view.session_rgb);
        fill_rect(&mut buf, width, 16, 0, 16, view.seq_rgb);
        let ink = Color::rgb(0xE0, 0xE0, 0xE0);
        view.buffer.draw(fs, &mut view.swash, ink, |x, y, gw, gh, color| {
            let alpha = color.a() as u32;
            for row in 0..gh as i32 {
                for col in 0..gw as i32 {
                    let (px, py) = (x + col, y + row);
                    if px >= 0 && py >= 0 && (px as usize) < width && (py as usize) < height {
                        let idx = py as usize * width + px as usize;
                        let dst = buf[idx];
                        let mix = |s: u8, shift: u32| {
                            let d = (dst >> shift) & 0xFF;
                            (d + ((s as u32).wrapping_sub(d) as i32 * alpha as i32 / 255) as u32) & 0xFF
                        };
                        buf[idx] = 0xFF00_0000 | mix(color.r(), 16) << 16 | mix(color.g(), 8) << 8 | mix(color.b(), 0);
                    }
                }
            }
        });
        window.pre_present_notify();
        let _ = buf.present();
    }
}

impl ApplicationHandler<Ev> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Lico candidate A")
            .with_inner_size(LogicalSize::new(1200.0, 800.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        window.set_ime_allowed(true);
        self.context = Some(Context::new(window.clone()).expect("context"));
        self.window = Some(window);
        self.present_neutral();
        if let Some((id, after)) = self.pending_show.take() {
            self.show(&id, after);
        }
        println!("{{\"ev\":\"ready\",\"pid\":{}}}", std::process::id());
        let _ = std::io::stdout().flush();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // keys the input method did not take (space, digits, Latin text) arrive here
                if event.state == ElementState::Pressed && event.logical_key == Key::Named(NamedKey::Enter) {
                    self.send_draft();
                } else if event.state == ElementState::Pressed {
                    if let Some(text) = &event.text {
                        if !text.chars().any(char::is_control) {
                            self.input.push_str(text);
                            emit_ime("text", &self.input);
                            self.dirty = true;
                        }
                    }
                }
            }
            WindowEvent::Ime(ime) => {
                match ime {
                    Ime::Preedit(text, _) => {
                        emit_ime("preedit", &text);
                        self.preedit = text;
                    }
                    Ime::Commit(text) if self.stale_commit.as_deref() == Some(text.as_str()) => {
                        self.stale_commit = None; // the old composition finishing, not new typing
                        emit_ime("ignored", &text);
                    }
                    Ime::Commit(text) => {
                        emit_ime("commit", &text);
                        self.input.push_str(&text);
                        self.preedit.clear();
                        emit_ime("text", &self.input);
                    }
                    Ime::Enabled => emit_ime("enabled", ""),
                    Ime::Disabled => emit_ime("disabled", ""),
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.dirty {
            self.dirty = false;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Ev) {
        match event {
            Ev::Sock(line) => self.on_sock_line(&line),
            Ev::Cmd(line) => self.handle_command(&line, event_loop),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone());
    let Some(socket) = arg("--socket") else {
        eprintln!("usage: candidate-a2 --socket PATH");
        std::process::exit(2);
    };
    let sock = UnixStream::connect(&socket).expect("connect");
    let reader = sock.try_clone().expect("clone");
    let event_loop = EventLoop::<Ev>::with_user_event().build().expect("event loop");
    let proxy = event_loop.create_proxy();
    let sock_proxy = proxy.clone();
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if sock_proxy.send_event(Ev::Sock(line)).is_err() {
                break;
            }
        }
    });
    thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if proxy.send_event(Ev::Cmd(line)).is_err() {
                break;
            }
        }
        let _ = proxy.send_event(Ev::Cmd("quit".into()));
    });
    let mut app = App {
        stale_commit: None, view_id: String::new(), generation: 1, drafts: HashMap::new(), sent_count: 0,
        input: String::new(), preedit: String::new(), dirty: false, meta: Vec::new(), pending_show: None, print_status: false,
        window: None, context: None, view: None, font_system: None, sock,
        session: String::new(), cursor: -1, dups: 0, gaps: 0, resyncs: 0, subscribed: false,
    };
    event_loop.run_app(&mut app).expect("run");
}
