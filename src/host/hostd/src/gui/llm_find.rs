//! "Find Local LLM servers..." (`M12`, `M13`): a centred dialog, opened from the Session menu, that asks the host which listening ports of this machine or of an ssh host answer
//! like an OpenAI-compatible or an Ollama server (`llm.discover`) and lists them with their model names. The look starts only when the person presses Enter (or clicks Scan or
//! Refresh), never by itself. The last result of each (host, all users) pair is kept in this window's memory with its time (`Kept`, at most eight, nothing on disk) and shown
//! again, with its age, when the dialog is opened or the choice is made again; Enter or Refresh looks again. Servers that answered but list no model are folded behind one line.
//! Keys: Enter or S scan or refresh, A toggles all users' ports, Left and Right choose the host, Up and Down select a row, F opens or closes the fold, Esc closes.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::approval_modal::{darken_region, draw_centered_btn, ALLOW_LIVE, DIVIDER, MODAL_BG, READ_LIVE, SHADOW_BG};
use super::controls::{self, Action, Rect};
use super::jobs::AsyncJob;
use super::llm_rows::{self as rows, Entry, Item};
use super::sidebar_chats::chats_error_text;
use super::ui::rect;
use super::{clean, App};

const BORDER: u32 = 0xFF4A_D6A8;
const HEADER_BG: u32 = 0xFF14_2B26;
const SELECTED_BG: u32 = 0xFF1E_3B34;
/// Lines of the list that are shown at once (a server takes two, the fold line one).
const LIST_LINES: usize = 10;
const KEPT: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Scan {
    Idle,
    Running,
    Done(Value),
    Failed(String),
}

/// The last result of one (host, all users) pair and when it was made (seconds of the window's clock).
pub(crate) struct Kept {
    host: Option<String>,
    all_users: bool,
    at: u64,
    reply: Value,
}

#[derive(Clone, Default)]
pub(crate) struct Geom {
    pub scan: Option<Rect>,
    pub close: Option<Rect>,
    pub all_users: Option<Rect>,
    pub more: Option<Rect>,
    pub rows: Vec<(usize, Rect)>,
    pub cols: usize,
}

pub(crate) struct LlmFind {
    pub hosts: Vec<Option<String>>, // this machine first, then the ssh hosts of the person's configuration
    pub host_idx: usize,
    pub all_users: bool,
    pub scan: Scan,
    pub at: Option<u64>,  // when the shown result was made
    pub cached: bool,     // it was kept from an earlier look, not made while this dialog was open
    pub asked: Option<(Option<String>, bool)>, // the pair the look in flight is about
    pub scroll: usize,    // the first entry of the list that is shown
    pub sel: usize,       // the selected server (its place among the shown servers)
    pub fold_open: bool,
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
    Fold,
    Close,
}

/// What was found, in one line, and how many other ports there were.
pub(super) fn summary(reply: &Value) -> String {
    if reply["supported"] != json!(true) {
        return format!("This machine's listening ports cannot be read here ({}).", reply["reason"].as_str().unwrap_or("unknown"));
    }
    let servers = rows::items(reply).len();
    let others = reply["endpoints"].as_array().into_iter().flatten().filter(|e| !rows::is_server(e)).count();
    let skipped = reply["skipped"].as_u64().unwrap_or(0);
    format!(
        "{servers} server{} found; {others} other port{} did not answer like one{}. {} listening, {} looked at, {} ms.",
        if servers == 1 { "" } else { "s" }, if others == 1 { "" } else { "s" }, if skipped > 0 { format!(" ({skipped} not looked at: too many)") } else { String::new() },
        reply["listening"], reply["probed"], reply["elapsed_ms"]
    )
}

impl LlmFind {
    fn host(&self) -> Option<&str> {
        self.hosts.get(self.host_idx).and_then(|h| h.as_deref())
    }

    fn items(&self) -> Vec<Item> {
        match &self.scan {
            Scan::Done(reply) => rows::items(reply),
            _ => Vec::new(),
        }
    }

    /// The list as shown: the servers that are not folded, the fold line, and the folded servers when it is open.
    fn list(&self, items: &[Item]) -> Vec<Entry> {
        rows::entries(items, self.fold_open)
    }

    fn shown(&self, items: &[Item]) -> usize {
        self.list(items).iter().filter(|e| matches!(e, Entry::Item(_))).count()
    }

    /// Moves what a key moves; true when the key ends the dialog or asks for a look (the caller does that).
    pub(super) fn press(&mut self, key: Press) -> bool {
        let items = self.items();
        let shown = self.shown(&items);
        match key {
            Press::AllUsers => self.all_users = !self.all_users,
            Press::Prev | Press::Next => {
                let n = self.hosts.len().max(1);
                self.host_idx = if key == Press::Prev { (self.host_idx + n - 1) % n } else { (self.host_idx + 1) % n };
            }
            Press::Up => self.sel = self.sel.saturating_sub(1),
            Press::Down => self.sel = (self.sel + 1).min(shown.saturating_sub(1)),
            Press::Fold => {
                self.fold_open = !self.fold_open;
                self.sel = if self.fold_open { items.iter().take_while(|i| !i.folded).count() } else { self.sel }; // opened: the first folded server, so that the list shows them
                self.sel = self.sel.min(self.shown(&items).saturating_sub(1));
            }
            Press::Scan | Press::Close => return true,
        }
        let list = self.list(&items);
        self.scroll = rows::scroll_for(&list, (shown > 0).then_some(self.sel), self.scroll, LIST_LINES);
        false
    }

    /// Shows what is kept for the host and the users' choice that are chosen now, or nothing yet.
    fn restore(&mut self, kept: &[Kept]) {
        let found = kept.iter().find(|k| k.host.as_deref() == self.host() && k.all_users == self.all_users);
        (self.scan, self.at, self.cached) = match found {
            Some(k) => (Scan::Done(k.reply.clone()), Some(k.at), true),
            None => (Scan::Idle, None, false),
        };
        (self.scroll, self.sel, self.fold_open) = (0, 0, false);
    }
}

impl App {
    pub(crate) fn llm_find_open(&mut self) {
        let listed = self.host_call_within(json!({"op": "ssh.hosts"}), 3);
        let mut hosts: Vec<Option<String>> = vec![None];
        hosts.extend(listed["hosts"].as_array().into_iter().flatten().filter_map(|h| h["host"].as_str()).map(|h| Some(super::clean(h))));
        let mut f = LlmFind { hosts, host_idx: 0, all_users: false, scan: Scan::Idle, at: None, cached: false, asked: None, scroll: 0, sel: 0, fold_open: false, req: 0, geom: Geom::default() };
        f.restore(&self.ui.sidebar.llm_cache);
        self.ui.sidebar.llm = Some(f);
        self.dirty = true;
    }

    fn llm_find_close(&mut self) {
        self.ui.sidebar.llm = None;
        let ours = |a: &Action| matches!(a, Action::ModalBackdrop | Action::LlmScan | Action::LlmClose | Action::LlmAllUsers | Action::LlmMore | Action::LlmPick(_));
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
        f.asked = Some((f.host().map(str::to_string), f.all_users));
        f.scan = Scan::Running;
        (f.scroll, f.sel, f.fold_open) = (0, 0, false);
        let req_id = self.spawn_host_call(request, seconds, |req_id| AsyncJob::LlmFind { req_id });
        if let Some(f) = self.ui.sidebar.llm.as_mut() {
            f.req = req_id;
        }
        self.dirty = true;
    }

    pub(crate) fn on_llm_async(&mut self, job: AsyncJob, reply: Value) {
        let has_proxy = self.ui.proxy.is_some();
        let AsyncJob::LlmFind { req_id } = job else { return };
        let now = self.clock_now();
        let Some(f) = self.ui.sidebar.llm.as_mut() else { return };
        if has_proxy && f.req != req_id {
            return;
        }
        let asked = f.asked.take();
        let current = asked.as_ref().is_some_and(|(h, u)| h.as_deref() == f.host() && *u == f.all_users);
        if reply["ok"] == json!(1) {
            if let Some((host, all_users)) = asked {
                let cache = &mut self.ui.sidebar.llm_cache;
                cache.retain(|k| !(k.host == host && k.all_users == all_users));
                cache.push(Kept { host, all_users, at: now, reply: reply.clone() });
                if cache.len() > KEPT {
                    cache.remove(0);
                }
            }
        }
        let Some(f) = self.ui.sidebar.llm.as_mut() else { return };
        if reply["ok"] == json!(1) {
            if current {
                (f.scan, f.at, f.cached) = (Scan::Done(reply), Some(now), false);
            } else if f.scan == Scan::Running {
                f.scan = Scan::Idle; // the answer was about another choice: it is kept, not shown
            }
        } else if f.scan == Scan::Running {
            f.scan = Scan::Failed(match reply["error"].as_str().unwrap_or("unknown") {
                "option_unavailable" => "That is not available for this machine.".to_string(),
                "ssh_unreachable" => "That host could not be reached over ssh without a question (is it in the ssh configuration, and does key login work?).".to_string(),
                "remote_python_missing" => "That host has no python3, which the look needs.".to_string(),
                "remote_timeout" => "That host did not answer in time.".to_string(),
                "remote_output_too_large" => "That host sent more text than the host reads; nothing was read.".to_string(),
                "remote_failed" => "That host's answer could not be used.".to_string(),
                other => chats_error_text(other),
            });
        }
        self.dirty = true;
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
            Key::Character(c) if c.as_str().eq_ignore_ascii_case("f") => Some(Press::Fold),
            _ => None,
        };
        if let Some(press) = press.filter(|p| !(event.repeat && matches!(p, Press::Scan | Press::Close))) {
            self.llm_apply(press);
        }
        true
    }

    fn llm_apply(&mut self, press: Press) {
        let done = self.ui.sidebar.llm.as_mut().is_some_and(|f| f.press(press));
        match (done, press) {
            (true, Press::Scan) => self.llm_find_scan(),
            (true, _) => self.llm_find_close(),
            (false, Press::Prev | Press::Next | Press::AllUsers) => {
                let kept = std::mem::take(&mut self.ui.sidebar.llm_cache);
                if let Some(f) = self.ui.sidebar.llm.as_mut() {
                    f.restore(&kept); // the answer was about another choice: this one's own, or none
                }
                self.ui.sidebar.llm_cache = kept;
            }
            _ => {}
        }
        self.dirty = true;
    }

    pub(crate) fn llm_click(&mut self, action: &Action) -> bool {
        match action {
            Action::LlmScan => self.llm_apply(Press::Scan),
            Action::LlmClose => self.llm_apply(Press::Close),
            Action::LlmAllUsers => self.llm_apply(Press::AllUsers),
            Action::LlmMore => self.llm_apply(Press::Fold),
            Action::LlmPick(i) => {
                if let Some(f) = self.ui.sidebar.llm.as_mut() {
                    f.sel = *i;
                }
                self.dirty = true;
            }
            _ => return false,
        }
        true
    }

    pub(super) fn llm_json(&self) -> Value {
        let Some(f) = &self.ui.sidebar.llm else { return Value::Null };
        let r = |r: &Option<Rect>| r.map_or(Value::Null, |r| json!(r));
        let (state, message) = match &f.scan {
            Scan::Idle => ("idle", String::new()),
            Scan::Running => ("running", String::new()),
            Scan::Done(reply) => ("done", summary(reply)),
            Scan::Failed(why) => ("failed", why.clone()),
        };
        let endpoints = match &f.scan {
            Scan::Done(reply) => reply["endpoints"].clone(),
            _ => json!([]),
        };
        let cols = f.geom.cols.max(20);
        let all = f.items();
        let list = f.list(&all);
        let shown: Vec<usize> = list.iter().filter_map(|e| if let Entry::Item(i) = e { Some(*i) } else { None }).collect();
        let folded = all.iter().filter(|i| i.folded).count();
        let items: Vec<Value> = shown.iter().map(|&i| {
            let (lines, clipped) = rows::row_lines(&all[i], cols);
            json!({"lines": lines, "clipped": clipped, "selected": i == f.sel, "folded": all[i].folded, "via_ssh": all[i].via_ssh,
                   "full": format!("{}  {}", all[i].head(), if all[i].ids.is_empty() { "no model listed".to_string() } else { all[i].ids.join(", ") })})
        }).collect();
        let row_rect = |i: usize| f.geom.rows.iter().find(|(k, _)| *k == i).map_or(Value::Null, |(_, r)| json!(r));
        let age = f.at.map(|at| self.clock_now().saturating_sub(at));
        let detail = shown.contains(&f.sel).then(|| rows::detail(&all[f.sel], cols)).unwrap_or_default();
        let note = if all.iter().any(|i| i.ids.is_empty()) { rows::NO_MODEL_NOTE } else { "" };
        let place = f.host().map_or("this machine".to_string(), |h| format!("ssh host {h}"));
        let scan_label = if f.scan == Scan::Running { "Looking..." } else if matches!(f.scan, Scan::Done(_)) { "Refresh" } else { "Scan" };
        json!({"open": true, "host": f.host(), "hosts": f.hosts, "all_users": f.all_users, "state": state, "message": message,
               "rows": shown.iter().map(|&i| rows::row_text(&all[i])).collect::<Vec<_>>(), "items": items, "selected": shown.contains(&f.sel).then_some(f.sel), "endpoints": endpoints,
               "folded": {"count": folded, "open": f.fold_open, "label": if folded > 0 { rows::fold_label(folded, f.fold_open) } else { String::new() }},
               "detail": detail, "note": note, "place_lines": controls::wrap_cols(&clean(&place), cols), "cols": cols, "cached": f.cached, "age_s": age,
               "found": age.map_or(String::new(), |a| format!("found {}", rows::age_words(a))), "scan_label": scan_label,
               "rects": {"scan": r(&f.geom.scan), "close": r(&f.geom.close), "all_users": r(&f.geom.all_users), "more": r(&f.geom.more), "items": shown.iter().map(|&i| row_rect(i)).collect::<Vec<_>>()}})
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_llm_find(&mut self, buf: &mut [u32], width: usize, height: usize, top: f32, chat_top: f32, hint_y: f32) {
        let now = self.clock_now();
        let Some(f) = self.ui.sidebar.llm.as_ref() else { return };
        let (all_users, scroll, sel, at, fold_open, cached) = (f.all_users, f.scroll, f.sel, f.at, f.fold_open, f.cached);
        let place = f.host().map_or("this machine".to_string(), |h| format!("ssh host {h}"));
        let ssh = f.host().is_some();
        let waits = if ssh { "up to 20 seconds" } else { "up to 8 seconds" };
        let all = f.items();
        let list = f.list(&all);
        let kept_n = all.iter().filter(|i| !i.folded).count();
        let folded = all.len() - kept_n;
        let status = match &f.scan {
            Scan::Idle => format!("Press Enter to look for Local LLM servers on the listening ports of {place}. Read-only; nothing is stored on disk."),
            Scan::Running => format!("Looking... ({waits})"),
            Scan::Done(reply) => format!("{} Found {}{}", summary(reply), rows::age_words(at.map_or(0, |a| now.saturating_sub(a))), if cached { " (kept from the earlier look; Refresh looks again)." } else { "." }),
            Scan::Failed(why) => why.clone(),
        };
        let (busy, done) = (f.scan == Scan::Running, matches!(f.scan, Scan::Done(_)));
        let s = self.ui.scale.max(1.0);
        let lh = (self.line_h() * s).floor().max(18.0);
        let char_w = self.char_w() * s;
        darken_region(buf, width, height, 0, top.floor().max(0.0) as usize, width, hint_y.floor().max(0.0) as usize, 102);
        let modal_w = (760.0 * s).min(width as f32 - (32.0 * s).floor()).max(320.0 * s).floor();
        let pad = (20.0 * s).floor();
        let cols = (((modal_w - 2.0 * pad).max(120.0 * s)) / char_w).max(20.0) as usize;
        let place_lines: Vec<String> = if ssh { controls::wrap_cols(&clean(&place), cols).into_iter().take(4).collect() } else { Vec::new() };
        let status_lines: Vec<String> = controls::wrap_cols(&clean(&status), cols).into_iter().take(3).collect();
        let first = scroll.min(list.len().saturating_sub(1));
        let count = rows::fit(&list, first, LIST_LINES);
        let shown: Vec<Entry> = list.iter().skip(first).take(count).copied().collect();
        let list_lines: usize = shown.iter().map(|e| e.lines()).sum();
        let detail: Vec<String> = if done && sel < kept_n + if fold_open { folded } else { 0 } { all.get(sel).map(|i| rows::detail(i, cols)).unwrap_or_default() } else { Vec::new() };
        let has_detail = !detail.is_empty();
        let note: Vec<String> = if done && all.iter().any(|i| i.ids.is_empty()) { controls::wrap_cols(rows::NO_MODEL_NOTE, cols).into_iter().take(3).collect() } else { Vec::new() };
        let (header_h, btn_h, gap) = ((lh + 14.0 * s).floor(), (lh + 10.0 * s).floor(), (12.0 * s).floor());
        let blocks = [place_lines.len(), status_lines.len()].iter().sum::<usize>() + list_lines.max(1) + detail.len() + usize::from(has_detail) + note.len() + 1;
        let modal_h = header_h + blocks as f32 * lh + gap * 5.0 + btn_h + (16.0 * s).floor();
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
        let title = format!("Local LLM servers  \u{00b7}  {}  \u{00b7}  {}", if ssh { "ssh host" } else { "this machine" }, if all_users { "all users' ports" } else { "your own processes' ports" });
        controls.text(fs, swash, buf, width, height, x0 + pad, (y0 + ((header_h - lh) * 0.5).floor()).max(y0), clip, &title, accent);
        let mut y = (y0 + header_h + gap).floor();
        for line in &place_lines {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, line, muted); // the whole host name, wrapped: the title above is short
            y += lh;
        }
        for line in &status_lines {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, line, ink);
            y += lh;
        }
        y += gap;
        let mut geom = Geom { cols, ..Geom::default() };
        if shown.is_empty() {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, if done && all.is_empty() { "(nothing listed)" } else { "" }, muted);
            y += lh;
        }
        for entry in &shown {
            match *entry {
                Entry::More => {
                    let r = [x0 + pad, y, x1 - pad, y + lh];
                    controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, &rows::fold_label(folded, fold_open), accent);
                    controls.target(r, Action::LlmMore);
                    geom.more = Some(r);
                    y += lh;
                }
                Entry::Item(i) => {
                    let r = [x0 + pad, y, x1 - pad, y + 2.0 * lh];
                    if i == sel {
                        rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, SELECTED_BG);
                    }
                    let (lines, _) = rows::row_lines(&all[i], cols);
                    controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, &clean(&lines[0]), ink);
                    controls.text(fs, swash, buf, width, height, x0 + pad, y + lh, clip, &clean(&lines[1]), muted);
                    controls.target(r, Action::LlmPick(i));
                    geom.rows.push((i, r));
                    y += 2.0 * lh;
                }
            }
        }
        y += gap;
        if has_detail {
            rect(buf, width, height, (x0 + pad) as i32, (y - (gap * 0.5).floor()) as i32, (modal_w - 2.0 * pad) as i32, 1, DIVIDER);
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, "Selected row, whole:", muted);
            y += lh;
        }
        for line in &detail {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, line, ink); // the selected row, whole
            y += lh;
        }
        for line in &note {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, line, muted);
            y += lh;
        }
        y += gap;
        controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, "Enter scan   Up/Down select   F more   Left/Right host   A all users   Esc close", muted);
        let by = (y1 - (16.0 * s).floor() - btn_h).floor();
        rect(buf, width, height, (x0 + pad) as i32, (by - (7.0 * s).floor()) as i32, (modal_w - 2.0 * pad) as i32, 1, DIVIDER);
        let (bwd, g) = ((120.0 * s).floor(), (12.0 * s).floor());
        let close_r = [(x1 - pad - bwd).floor(), by, (x1 - pad).floor(), by + btn_h];
        let scan_r = [close_r[0] - g - bwd, by, close_r[0] - g, by + btn_h];
        let users_r = [x0 + pad, by, (x0 + pad + 190.0 * s).min(scan_r[0] - g).floor(), by + btn_h];
        let on = Color::rgb(0xF4, 0xF7, 0xFB);
        let label = if busy { "Looking..." } else if done { "Refresh" } else { "Scan" };
        draw_centered_btn(controls, fs, swash, buf, width, height, scan_r, label, if busy { 0xFF24_3629 } else { ALLOW_LIVE }, on, lh, s, Action::LlmScan);
        draw_centered_btn(controls, fs, swash, buf, width, height, close_r, "Close", READ_LIVE, on, lh, s, Action::LlmClose);
        draw_centered_btn(controls, fs, swash, buf, width, height, users_r, if all_users { "All users: on" } else { "All users: off" }, READ_LIVE, on, lh, s, Action::LlmAllUsers);
        geom.scan = Some(scan_r);
        geom.close = Some(close_r);
        geom.all_users = Some(users_r);
        if let Some(f) = self.ui.sidebar.llm.as_mut() {
            f.geom = geom;
        }
    }
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

    fn find(hosts: Vec<Option<String>>) -> LlmFind {
        LlmFind { hosts, host_idx: 0, all_users: false, scan: Scan::Idle, at: None, cached: false, asked: None, scroll: 0, sel: 0, fold_open: false, req: 0, geom: Geom::default() }
    }

    #[test]
    fn the_summary_counts_the_servers_and_the_other_ports() {
        let s = summary(&reply());
        assert_eq!(s, "2 servers found; 1 other port did not answer like one (1 not looked at: too many). 5 listening, 4 looked at, 321 ms.");
        assert!(summary(&json!({"supported": false, "reason": "proc_unreadable"})).contains("cannot be read here (proc_unreadable)"));
    }

    #[test]
    fn the_keys_select_fold_choose_and_end_or_start_the_look() {
        let mut f = find(vec![None, Some("gpu-box".into()), Some("b".into())]);
        let many: Vec<Value> = (0..4).map(|n| json!({"kind": "openai", "address": "127.0.0.1", "port": 8000 + n, "bind": "loopback", "auth": "none_needed", "models": ["m"], "models_total": 1, "process": null}))
            .chain([json!({"kind": "ollama", "address": "127.0.0.1", "port": 9, "bind": "loopback", "auth": "none_needed", "models": [], "models_total": 0, "process": null})]).collect();
        f.scan = Scan::Done(json!({"supported": true, "endpoints": many}));
        assert!(!f.press(Press::AllUsers) && f.all_users);
        for _ in 0..9 {
            f.press(Press::Down);
        }
        assert_eq!(f.sel, 3, "no further than the last shown server: the folded one is not shown");
        assert!(!f.press(Press::Fold) && f.fold_open);
        assert_eq!(f.sel, 4, "opened: the first folded server is selected");
        assert!(f.scroll > 0, "the list follows the selection");
        assert!(!f.press(Press::Fold) && f.sel == 3, "closing the fold takes the selection back to a shown server");
        assert!(!f.press(Press::Up) && f.sel == 2);
        assert!(f.press(Press::Scan) && f.press(Press::Close));
        assert!(!f.press(Press::Next) && f.host() == Some("gpu-box"));
        f.press(Press::Next);
        f.press(Press::Next);
        assert_eq!(f.host(), None, "the list wraps to this machine");
        f.press(Press::Prev);
        assert_eq!(f.host(), Some("b"));
    }

    #[test]
    fn a_kept_result_is_shown_again_for_its_own_pair_and_for_no_other() {
        let kept = vec![Kept { host: None, all_users: false, at: 1000, reply: reply() }, Kept { host: Some("gpu-box".into()), all_users: true, at: 1200, reply: json!({"supported": true, "endpoints": []}) }];
        let mut f = find(vec![None, Some("gpu-box".into())]);
        f.restore(&kept);
        assert!(matches!(f.scan, Scan::Done(_)) && f.at == Some(1000) && f.cached);
        f.all_users = true;
        f.restore(&kept);
        assert!(f.scan == Scan::Idle && f.at.is_none() && !f.cached, "all users' ports of this machine were never looked at");
        f.host_idx = 1;
        f.restore(&kept);
        assert!(matches!(f.scan, Scan::Done(_)) && f.at == Some(1200));
    }
}
