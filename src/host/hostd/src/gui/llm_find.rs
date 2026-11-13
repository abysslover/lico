//! "Find Local LLM servers..." (`M12`): a centred dialog, opened from the Session menu, that asks the host which listening ports of this machine answer like an
//! OpenAI-compatible or an Ollama server (`llm.discover`) and lists them with their model names. The look starts only when the person presses Enter (or clicks Scan), never by
//! itself, and nothing is kept: the result lives in the dialog until it is closed. Keys: Enter or S scan, A toggles all users' ports, Up and Down scroll, Esc closes.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::approval_modal::{darken_region, draw_centered_btn, ALLOW_LIVE, DIVIDER, MODAL_BG, READ_LIVE, SHADOW_BG};
use super::controls::{self, Action, Rect};
use super::jobs::AsyncJob;
use super::sidebar_chats::chats_error_text;
use super::ui::rect;
use super::{clean, App};

const BORDER: u32 = 0xFF4A_D6A8;
const HEADER_BG: u32 = 0xFF14_2B26;
const VISIBLE_ROWS: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Scan {
    Idle,
    Running,
    Done(Value),
    Failed(String),
}

#[derive(Clone, Default)]
pub(crate) struct Geom {
    pub scan: Option<Rect>,
    pub close: Option<Rect>,
    pub all_users: Option<Rect>,
}

pub(crate) struct LlmFind {
    pub hosts: Vec<Option<String>>, // this machine first, then the ssh hosts of the person's configuration
    pub host_idx: usize,
    pub all_users: bool,
    pub scan: Scan,
    pub scroll: usize,
    pub req: u64,
    pub geom: Geom,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Press {
    Scan,
    AllUsers,
    Prev,
    Next,
    Up,
    Down,
    Close,
}

/// A line per server (openai and ollama), most useful first as the host sorted them.
pub(super) fn server_rows(reply: &Value) -> Vec<String> {
    reply["endpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| matches!(e["kind"].as_str(), Some("openai" | "ollama")))
        .map(|e| {
            let models: Vec<&str> = e["models"].as_array().into_iter().flatten().filter_map(Value::as_str).take(3).collect();
            let total = e["models_total"].as_u64().unwrap_or(0) as usize;
            let list = if models.is_empty() { "no model listed".to_string() } else { format!("{}{}", models.join(", "), if total > models.len() { format!(" +{}", total - models.len()) } else { String::new() }) };
            let auth = match e["auth"].as_str() { Some("key_needed") => "key needed", Some("none_needed") => "open", _ => "?" };
            format!(
                "{}:{}  {}  {}  {}  {}{}",
                e["address"].as_str().unwrap_or("?"), e["port"], e["kind"].as_str().unwrap_or("?"), e["bind"].as_str().unwrap_or("?"), auth, list,
                e["process"].as_str().map_or(String::new(), |p| format!("  ({p})"))
            )
        })
        .collect()
}

/// What was found, in one line, and how many other ports there were.
pub(super) fn summary(reply: &Value) -> String {
    if reply["supported"] != json!(true) {
        return format!("This machine's listening ports cannot be read here ({}).", reply["reason"].as_str().unwrap_or("unknown"));
    }
    let servers = server_rows(reply).len();
    let others: Vec<&Value> = reply["endpoints"].as_array().into_iter().flatten().filter(|e| !matches!(e["kind"].as_str(), Some("openai" | "ollama"))).collect();
    let skipped = reply["skipped"].as_u64().unwrap_or(0);
    format!(
        "{servers} server{} found; {} other port{} did not answer like one{}. {} listening, {} looked at, {} ms.",
        if servers == 1 { "" } else { "s" }, others.len(), if others.len() == 1 { "" } else { "s" }, if skipped > 0 { format!(" ({skipped} not looked at: too many)") } else { String::new() },
        reply["listening"], reply["probed"], reply["elapsed_ms"]
    )
}

fn press_is_prev(key: Press) -> bool {
    key == Press::Prev
}

impl LlmFind {
    fn host(&self) -> Option<&str> {
        self.hosts.get(self.host_idx).and_then(|h| h.as_deref())
    }

    pub(super) fn press(&mut self, key: Press, rows: usize) -> bool {
        match key {
            Press::AllUsers => {
                self.all_users = !self.all_users;
                false
            }
            Press::Prev | Press::Next => {
                let n = self.hosts.len().max(1);
                self.host_idx = if press_is_prev(key) { (self.host_idx + n - 1) % n } else { (self.host_idx + 1) % n };
                self.scan = Scan::Idle; // the answer was about another machine
                self.scroll = 0;
                false
            }
            Press::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                false
            }
            Press::Down => {
                self.scroll = (self.scroll + 1).min(rows.saturating_sub(VISIBLE_ROWS));
                false
            }
            Press::Scan | Press::Close => true,
        }
    }
}

impl App {
    pub(crate) fn llm_find_open(&mut self) {
        let listed = self.host_call_within(json!({"op": "ssh.hosts"}), 3);
        let mut hosts: Vec<Option<String>> = vec![None];
        hosts.extend(listed["hosts"].as_array().into_iter().flatten().filter_map(|h| h["host"].as_str()).map(|h| Some(super::clean(h))));
        self.ui.sidebar.llm = Some(LlmFind { hosts, host_idx: 0, all_users: false, scan: Scan::Idle, scroll: 0, req: 0, geom: Geom::default() });
        self.dirty = true;
    }

    fn llm_find_close(&mut self) {
        self.ui.sidebar.llm = None;
        let ours = |a: &Action| matches!(a, Action::ModalBackdrop | Action::LlmScan | Action::LlmClose | Action::LlmAllUsers);
        self.ui.controls.hits.retain(|h| !ours(&h.action));
        self.dirty = true;
    }

    /// Starts the look: the one request, on a worker thread, with room for the host's own 8 seconds.
    fn llm_find_scan(&mut self) {
        let Some(f) = self.ui.sidebar.llm.as_mut() else { return };
        if f.scan == Scan::Running {
            return;
        }
        let mut request = json!({"op": "llm.discover", "all_users": f.all_users});
        let seconds = if let Some(host) = f.host() {
            request["ssh_host"] = json!(host);
            30 // the host's own budget for an ssh host is 20 seconds
        } else {
            14
        };
        f.scan = Scan::Running;
        f.scroll = 0;
        let req_id = self.spawn_host_call(request, seconds, |req_id| AsyncJob::LlmFind { req_id });
        if let Some(f) = self.ui.sidebar.llm.as_mut() {
            f.req = req_id;
        }
        self.dirty = true;
    }

    pub(crate) fn on_llm_async(&mut self, job: AsyncJob, reply: Value) {
        let has_proxy = self.ui.proxy.is_some();
        let AsyncJob::LlmFind { req_id } = job else { return };
        let Some(f) = self.ui.sidebar.llm.as_mut() else { return };
        if has_proxy && f.req != req_id {
            return;
        }
        f.scan = if reply["ok"] == json!(1) {
            Scan::Done(reply)
        } else {
            Scan::Failed(match reply["error"].as_str().unwrap_or("unknown") {
                "option_unavailable" => "That is not available for this machine.".to_string(),
                "ssh_unreachable" => "That host could not be reached over ssh without a question (is it in the ssh configuration, and does key login work?).".to_string(),
                "remote_python_missing" => "That host has no python3, which the look needs.".to_string(),
                "remote_timeout" => "That host did not answer in time.".to_string(),
                "remote_output_too_large" => "That host sent more text than the host reads; nothing was read.".to_string(),
                "remote_failed" => "That host's answer could not be used.".to_string(),
                other => chats_error_text(other),
            })
        };
        self.dirty = true;
    }

    fn llm_rows(&self) -> usize {
        match self.ui.sidebar.llm.as_ref().map(|f| &f.scan) {
            Some(Scan::Done(reply)) => server_rows(reply).len(),
            _ => 0,
        }
    }

    pub(crate) fn llm_key(&mut self, event: &KeyEvent) -> bool {
        if self.ui.sidebar.llm.is_none() || self.ui.mods.control_key() || self.ui.mods.alt_key() {
            return false;
        }
        let press = match &event.logical_key {
            Key::Named(NamedKey::Enter) => Some(Press::Scan),
            Key::Named(NamedKey::Escape) => Some(Press::Close),
            Key::Named(NamedKey::ArrowUp) => Some(Press::Up),
            Key::Named(NamedKey::ArrowDown) => Some(Press::Down),
            Key::Named(NamedKey::ArrowLeft) => Some(Press::Prev),
            Key::Named(NamedKey::ArrowRight) => Some(Press::Next),
            Key::Character(c) if c.as_str().eq_ignore_ascii_case("s") => Some(Press::Scan),
            Key::Character(c) if c.as_str().eq_ignore_ascii_case("a") => Some(Press::AllUsers),
            _ => None,
        };
        if let Some(press) = press.filter(|p| !(event.repeat && matches!(p, Press::Scan | Press::Close))) {
            self.llm_apply(press);
        }
        true
    }

    fn llm_apply(&mut self, press: Press) {
        let rows = self.llm_rows();
        let done = self.ui.sidebar.llm.as_mut().is_some_and(|f| f.press(press, rows));
        match (done, press) {
            (true, Press::Scan) => self.llm_find_scan(),
            (true, _) => self.llm_find_close(),
            _ => {}
        }
        self.dirty = true;
    }

    pub(crate) fn llm_click(&mut self, action: &Action) -> bool {
        match action {
            Action::LlmScan => self.llm_apply(Press::Scan),
            Action::LlmClose => self.llm_apply(Press::Close),
            Action::LlmAllUsers => self.llm_apply(Press::AllUsers),
            _ => return false,
        }
        true
    }

    pub(super) fn llm_json(&self) -> Value {
        let Some(f) = &self.ui.sidebar.llm else { return Value::Null };
        let r = |r: &Option<Rect>| r.map_or(Value::Null, |r| json!(r));
        let (state, message, rows) = match &f.scan {
            Scan::Idle => ("idle", String::new(), Vec::new()),
            Scan::Running => ("running", String::new(), Vec::new()),
            Scan::Done(reply) => ("done", summary(reply), server_rows(reply)),
            Scan::Failed(why) => ("failed", why.clone(), Vec::new()),
        };
        let endpoints = match &f.scan {
            Scan::Done(reply) => reply["endpoints"].clone(),
            _ => json!([]),
        };
        json!({"open": true, "host": f.host(), "hosts": f.hosts, "all_users": f.all_users, "state": state, "message": message, "rows": rows, "endpoints": endpoints,
               "rects": {"scan": r(&f.geom.scan), "close": r(&f.geom.close), "all_users": r(&f.geom.all_users)}})
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_llm_find(&mut self, buf: &mut [u32], width: usize, height: usize, top: f32, chat_top: f32, hint_y: f32) {
        let Some(f) = self.ui.sidebar.llm.as_ref() else { return };
        let (all_users, scroll) = (f.all_users, f.scroll);
        let place = f.host().map_or("this machine".to_string(), |h| format!("ssh host {h}"));
        let waits = if f.host().is_some() { "up to 20 seconds" } else { "up to 8 seconds" };
        let (status, rows): (String, Vec<String>) = match &f.scan {
            Scan::Idle => (format!("Press Enter to look for Local LLM servers on the listening ports of {place}. Read-only; nothing is stored."), Vec::new()),
            Scan::Running => (format!("Looking... ({waits})"), Vec::new()),
            Scan::Done(reply) => (summary(reply), server_rows(reply)),
            Scan::Failed(why) => (why.clone(), Vec::new()),
        };
        let busy = f.scan == Scan::Running;
        let s = self.ui.scale.max(1.0);
        let lh = (self.line_h() * s).floor().max(18.0);
        let char_w = self.char_w() * s;
        darken_region(buf, width, height, 0, top.floor().max(0.0) as usize, width, hint_y.floor().max(0.0) as usize, 102);
        let modal_w = (760.0 * s).min(width as f32 - (32.0 * s).floor()).max(320.0 * s).floor();
        let pad = (20.0 * s).floor();
        let cols = (((modal_w - 2.0 * pad).max(120.0 * s)) / char_w).max(20.0) as usize;
        let status_lines: Vec<String> = controls::wrap_cols(&clean(&status), cols).into_iter().take(3).collect();
        let shown: Vec<&String> = rows.iter().skip(scroll).take(VISIBLE_ROWS).collect();
        let (header_h, btn_h, gap) = ((lh + 14.0 * s).floor(), (lh + 10.0 * s).floor(), (12.0 * s).floor());
        let modal_h = header_h + gap + status_lines.len() as f32 * lh + gap + (shown.len().max(1) as f32) * lh + gap + lh + gap + btn_h + (16.0 * s).floor();
        let x0 = (((width as f32) - modal_w) * 0.5).floor().max(8.0);
        let x1 = (x0 + modal_w).min(width as f32 - 4.0).floor();
        let (ay0, ay1) = (top.floor(), hint_y.floor());
        let y0 = (ay0 + ((ay1 - ay0 - modal_h) * 0.35).floor()).max(ay0 + 4.0);
        let y1 = (y0 + modal_h).min(ay1 - 2.0).floor();
        let (mw, mh) = ((x1 - x0) as i32, (y1 - y0) as i32);
        let sh = (6.0 * s).floor() as i32;
        rect(buf, width, height, x0 as i32 + sh, y0 as i32 + sh, mw, mh, SHADOW_BG);
        rect(buf, width, height, x0 as i32, y0 as i32, mw, mh, MODAL_BG);
        rect(buf, width, height, x0 as i32, y0 as i32, mw, header_h as i32, HEADER_BG);
        let bw = (2.0 * s).round().max(2.0) as i32;
        for (bx, by, w, h) in [(x0 as i32, y0 as i32, mw, bw), (x0 as i32, y1 as i32 - bw, mw, bw), (x0 as i32, y0 as i32, bw, mh), (x1 as i32 - bw, y0 as i32, bw, mh)] {
            rect(buf, width, height, bx, by, w, h, BORDER);
        }
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        controls.target([0.0, chat_top.min(top), width as f32, hint_y], Action::ModalBackdrop);
        controls.target([x0, y0, x1, y1], Action::ModalBackdrop);
        let (ink, muted, accent) = (Color::rgb(0xEE, 0xF2, 0xF8), Color::rgb(0x98, 0xAC, 0xC2), Color::rgb(0x6E, 0xE8, 0xC0));
        let clip = (x1 - pad) as i32;
        let title = format!("Local LLM servers  \u{00b7}  {place}  \u{00b7}  {}", if all_users { "all users' ports" } else { "your own processes' ports" });
        controls.text(fs, swash, buf, width, height, x0 + pad, (y0 + ((header_h - lh) * 0.5).floor()).max(y0), clip, &title, accent);
        let mut y = (y0 + header_h + gap).floor();
        for line in &status_lines {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, line, ink);
            y += lh;
        }
        y += gap;
        if shown.is_empty() {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, if rows.is_empty() && !busy { "(nothing listed)" } else { "" }, muted);
            y += lh;
        }
        for row in &shown {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, &clip_cols(&clean(row), cols), ink);
            y += lh;
        }
        y += gap;
        controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, "Enter scan   Left/Right host   A all users' ports   Up/Down scroll   Esc close", muted);
        let by = (y1 - (16.0 * s).floor() - btn_h).floor();
        rect(buf, width, height, (x0 + pad) as i32, (by - (7.0 * s).floor()) as i32, (modal_w - 2.0 * pad) as i32, 1, DIVIDER);
        let (bwd, g) = ((120.0 * s).floor(), (12.0 * s).floor());
        let close_r = [(x1 - pad - bwd).floor(), by, (x1 - pad).floor(), by + btn_h];
        let scan_r = [close_r[0] - g - bwd, by, close_r[0] - g, by + btn_h];
        let users_r = [x0 + pad, by, (x0 + pad + 190.0 * s).min(scan_r[0] - g).floor(), by + btn_h];
        let on = Color::rgb(0xF4, 0xF7, 0xFB);
        draw_centered_btn(controls, fs, swash, buf, width, height, scan_r, if busy { "Looking..." } else { "Scan" }, if busy { 0xFF24_3629 } else { ALLOW_LIVE }, on, lh, s, Action::LlmScan);
        draw_centered_btn(controls, fs, swash, buf, width, height, close_r, "Close", READ_LIVE, on, lh, s, Action::LlmClose);
        draw_centered_btn(controls, fs, swash, buf, width, height, users_r, if all_users { "All users: on" } else { "All users: off" }, READ_LIVE, on, lh, s, Action::LlmAllUsers);
        if let Some(f) = self.ui.sidebar.llm.as_mut() {
            f.geom = Geom { scan: Some(scan_r), close: Some(close_r), all_users: Some(users_r) };
        }
    }
}

fn clip_cols(text: &str, cols: usize) -> String {
    super::sidebar_render::clip_cols(text, cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply() -> Value {
        json!({"supported": true, "listening": 5, "probed": 4, "skipped": 1, "elapsed_ms": 321, "endpoints": [
            {"kind": "ollama", "address": "127.0.0.1", "port": 11434, "bind": "loopback", "auth": "none_needed", "models": ["llama3:8b", "mistral:7b", "phi3", "qwen2"], "models_total": 6, "process": "ollama"},
            {"kind": "openai", "address": "0.0.0.0", "port": 8000, "bind": "all", "auth": "key_needed", "models": [], "models_total": 0, "process": null},
            {"kind": "not-http", "address": "127.0.0.1", "port": 22, "bind": "loopback", "auth": "unknown", "models": [], "models_total": 0, "process": null}]})
    }

    #[test]
    fn the_rows_are_the_servers_with_their_models_and_the_other_ports_are_counted() {
        let rows = server_rows(&reply());
        assert_eq!(rows, ["127.0.0.1:11434  ollama  loopback  open  llama3:8b, mistral:7b, phi3 +3  (ollama)", "0.0.0.0:8000  openai  all  key needed  no model listed"]);
        let s = summary(&reply());
        assert_eq!(s, "2 servers found; 1 other port did not answer like one (1 not looked at: too many). 5 listening, 4 looked at, 321 ms.");
        assert!(summary(&json!({"supported": false, "reason": "proc_unreadable"})).contains("cannot be read here (proc_unreadable)"));
    }

    #[test]
    fn the_keys_scroll_toggle_and_end_or_start_the_look() {
        let mut f = LlmFind { hosts: vec![None, Some("gpu-box".into()), Some("b".into())], host_idx: 0, all_users: false, scan: Scan::Idle, scroll: 0, req: 0, geom: Geom::default() };
        assert!(!f.press(Press::AllUsers, 0) && f.all_users);
        assert!(!f.press(Press::Down, 15) && f.scroll == 1);
        for _ in 0..20 {
            f.press(Press::Down, 15);
        }
        assert_eq!(f.scroll, 5, "no further than the last row shows");
        assert!(!f.press(Press::Up, 15) && f.scroll == 4);
        assert!(f.press(Press::Scan, 0) && f.press(Press::Close, 0));
        f.scan = Scan::Failed("x".into());
        assert!(!f.press(Press::Next, 0) && f.host() == Some("gpu-box") && f.scan == Scan::Idle, "another host: the old answer is gone");
        f.press(Press::Next, 0);
        f.press(Press::Next, 0);
        assert_eq!(f.host(), None, "the list wraps to this machine");
        f.press(Press::Prev, 0);
        assert_eq!(f.host(), Some("b"));
    }
}
