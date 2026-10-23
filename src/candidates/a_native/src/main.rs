//! Candidate A: owned minimal native client (winit + softbuffer + fontdue)
//! for the synthetic host. Commands arrive on stdin, see `handle_command`.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::num::NonZeroU32;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread;

use fontdue::{Font, FontSettings, Metrics};
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
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

struct Fonts {
    primary: Font,
    fallback: Font,
}

struct View {
    lines: VecDeque<String>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    glyphs: HashMap<char, (Metrics, Vec<u8>)>,
    session_rgb: u32,
    seq_rgb: u32,
}

struct SessionMeta {
    id: String,
    cursor: i64,
}

struct App {
    meta: Vec<SessionMeta>,
    pending_show: Option<(String, Option<i64>)>,
    print_status: bool,
    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    view: Option<View>,
    fonts: Option<Fonts>,
    font_paths: (String, String),
    sock: UnixStream,
    session: String,
    cursor: i64,
    dups: i64,
    gaps: i64,
    subscribed: bool,
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
            "{{\"ev\":\"state\",\"why\":\"{}\",\"session\":\"{}\",\"view_resident\":{},\"line_buffer_lines\":{},\"cursor\":{},\"subscribed\":{},\"dups\":{},\"gaps\":{},\"known_sessions\":{}}}",
            why, self.session, self.view.is_some(),
            self.view.as_ref().map_or(0, |v| v.lines.len()),
            self.cursor, self.subscribed, self.dups, self.gaps, self.meta.len()
        );
        let _ = std::io::stdout().flush();
    }

    fn load_fonts(&mut self) {
        if self.fonts.is_none() {
            let load = |p: &str| {
                Font::from_bytes(std::fs::read(p).expect("font file"), FontSettings::default())
                    .expect("font parse")
            };
            self.fonts = Some(Fonts { primary: load(&self.font_paths.0), fallback: load(&self.font_paths.1) });
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
        if self.subscribed && !self.session.is_empty() {
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
        let rgb = 0xFF00_0000 | (u32::from_str_radix(id.get(1..7).unwrap_or("0"), 16).unwrap_or(0) & 0xFF_FFFF);
        self.view = Some(View {
            lines: VecDeque::new(), surface, glyphs: HashMap::new(), session_rgb: rgb, seq_rgb: 0xFF00_0000,
        });
        self.session = id.to_string();
        let remembered = self.meta.iter().find(|m| m.id == id).map(|m| m.cursor).filter(|c| *c >= 0);
        self.cursor = after.or(remembered).unwrap_or(-1);
        let msg = format!("{{\"op\":\"subscribe\",\"session\":\"{id}\",\"after\":{}}}", self.cursor);
        self.send(&msg);
        window.request_redraw();
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
        if let Some(w) = &self.window {
            w.request_redraw();
        }
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
                self.subscribed = false;
                self.emit_state("overflow");
            }
            _ if line.contains("\"ok\":1") && line.contains("\"gap\":") => {
                self.subscribed = true;
                if line.contains("\"gap\":true") && self.view.is_some() {
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
        let (Some(window), Some(view), Some(fonts)) = (&self.window, self.view.as_mut(), self.fonts.as_ref()) else {
            return;
        };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        if view.surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buf) = view.surface.buffer_mut() else { return };
        let (width, height) = (w.get() as usize, h.get() as usize);
        buf.fill(BACKGROUND);
        fill_rect(&mut buf, width, 0, 0, 16, view.session_rgb);
        fill_rect(&mut buf, width, 16, 0, 16, view.seq_rgb);
        let title = "\u{d55c}\u{ae00} Lico session view";
        let rows: Vec<&str> = std::iter::once(title).chain(view.lines.iter().map(String::as_str)).collect();
        for (row, text) in rows.iter().enumerate() {
            let baseline = 14 + row * LINE_HEIGHT;
            let mut x = if row == 0 { 40 } else { 4 };
            for ch in text.chars() {
                let font = if fonts.primary.lookup_glyph_index(ch) != 0 { &fonts.primary } else { &fonts.fallback };
                let (metrics, bitmap) = view.glyphs.entry(ch).or_insert_with(|| font.rasterize(ch, FONT_PX));
                let top = baseline as i32 - metrics.height as i32 - metrics.ymin;
                for gy in 0..metrics.height {
                    for gx in 0..metrics.width {
                        let (px, py) = (x as i32 + metrics.xmin + gx as i32, top + gy as i32);
                        if px >= 0 && py >= 0 && (px as usize) < width && (py as usize) < height {
                            let cov = bitmap[gy * metrics.width + gx] as u32;
                            let shade = 0x10 + cov * (0xE0 - 0x10) / 255;
                            buf[py as usize * width + px as usize] = 0xFF00_0000 | shade << 16 | shade << 8 | shade;
                        }
                    }
                }
                x += metrics.advance_width.ceil() as usize;
            }
        }
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
            WindowEvent::Ime(ime) => println!("{{\"ev\":\"ime\",\"kind\":\"{}\"}}", format!("{ime:?}").split('(').next().unwrap_or("")),
            _ => {}
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
    let (Some(socket), Some(font), Some(font2)) = (arg("--socket"), arg("--font"), arg("--font2")) else {
        eprintln!("usage: candidate-a --socket PATH --font TTF --font2 TTF");
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
        meta: Vec::new(), pending_show: None, print_status: false,
        window: None, context: None, view: None, fonts: None, font_paths: (font, font2), sock,
        session: String::new(), cursor: -1, dups: 0, gaps: 0, subscribed: false,
    };
    event_loop.run_app(&mut app).expect("run");
}
