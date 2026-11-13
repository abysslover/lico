//! `Native` in the window (`M11-P01`): the name of the provider's own default home beside the input target, the Settings menu entries, and the one-time first-run
//! question `Use your existing Claude/Codex login and its chats?` (Yes, No, Ask later). The question appears when a default home has chats, the person has not
//! answered, and managed accounts exist (the host says `ask`); before the answer only the counts of the transcripts were read. `Ask later` asks again at the next
//! window start, nothing is stored for it. Keys: Y, N, L or Esc, Enter is Yes.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::approval_modal::{darken_region, draw_centered_btn, ALLOW_LIVE, DIVIDER, MODAL_BG, READ_LIVE, SHADOW_BG};
use super::controls::{self, Action, Rect};
use super::ui::{rect, Row};
use super::{clean, App};

const BORDER: u32 = 0xFF56_B4FF;
const HEADER_BG: u32 = 0xFF16_2638;
const DENY: u32 = 0xFF9E_3636;

/// One Native entry as `account.list` reports it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeEntry {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub state: String,
    pub chosen: String, // `on`, `off`, or empty while not answered
    pub chats: Option<u64>,
}

#[derive(Clone, Default)]
pub(crate) struct PromptGeom {
    pub yes: Option<Rect>,
    pub no: Option<Rect>,
    pub later: Option<Rect>,
}

pub(crate) struct Prompt {
    pub entries: Vec<NativeEntry>, // the providers to ask about: not answered, with chats in their default home
    pub geom: PromptGeom,
}

/// The Native entries of an `account.list` answer.
pub(super) fn parse_entries(list: &Value) -> Vec<NativeEntry> {
    list["native"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| NativeEntry {
            id: n["account"].as_str().unwrap_or("").to_string(),
            provider: n["provider"].as_str().unwrap_or("").to_string(),
            label: clean(n["label"].as_str().unwrap_or("")),
            state: n["state"].as_str().unwrap_or("").to_string(),
            chosen: n["chosen"].as_str().unwrap_or("").to_string(),
            chats: n["chats"].as_u64(),
        })
        .collect()
}

/// The providers the first-run question is about: not answered (`ask`) and with chats to show.
pub(super) fn wanted(entries: &[NativeEntry]) -> Vec<NativeEntry> {
    entries.iter().filter(|e| e.state == "ask" && e.chats.unwrap_or(0) > 0).cloned().collect()
}

fn provider_name(provider: &str) -> &'static str {
    if provider == "codex" { "Codex" } else { "Claude Code" }
}

impl App {
    /// The account of a row as the person reads it: the managed account's label, `Native (~/.claude)` for a session that runs in the provider's own default home
    /// (AUTH-06: never `no account` for a session that does run somewhere), else `no account`.
    pub(crate) fn row_label(&self, row: &Row) -> String {
        if row.account.is_empty() && !row.native.is_empty() {
            row.native.clone()
        } else {
            self.label_of(&row.account)
        }
    }

    /// An `account.list` answer arrived: keep the Native entries and open the first-run question once per window start when it is due.
    pub(crate) fn native_seen(&mut self, list: &Value) {
        let entries = parse_entries(list);
        let due = wanted(&entries);
        self.ui.sidebar.native_entries = entries;
        if !due.is_empty() && !self.ui.sidebar.native_asked && self.ui.sidebar.native_prompt.is_none() && self.ui.sidebar.newchat.is_none() {
            self.ui.sidebar.native_asked = true;
            self.ui.sidebar.native_prompt = Some(Prompt { entries: due, geom: PromptGeom::default() });
            self.dirty = true;
        }
    }

    fn native_answer(&mut self, answer: &str) {
        let Some(prompt) = self.ui.sidebar.native_prompt.take() else { return };
        self.dirty = true;
        self.ui.controls.hits.retain(|h| !matches!(h.action, Action::NativeYes | Action::NativeNo | Action::NativeLater | Action::ModalBackdrop));
        if answer == "later" {
            self.ui.flash = Some("Asking again the next time the window starts.".into());
            return;
        }
        let word = if answer == "yes" { "on" } else { "off" };
        for entry in &prompt.entries {
            let reply = self.host_call(json!({"op": "settings.set", format!("native_{}", entry.provider): word}));
            if reply["ok"] != json!(1) {
                self.ui.flash = Some("Could not keep the answer; use the Settings menu.".into());
                return;
            }
        }
        self.send("{\"op\":\"account.list\"}");
        self.chats_sync_if_open(true);
        self.ui.flash = Some(if answer == "yes" { "Your existing login and chats are in the account list (Native).".into() } else { "Native is off; the Settings menu turns it on.".into() });
    }

    pub(crate) fn native_key(&mut self, event: &KeyEvent) -> bool {
        if self.ui.sidebar.native_prompt.is_none() || self.ui.mods.control_key() || self.ui.mods.alt_key() {
            return false;
        }
        let answer = match &event.logical_key {
            Key::Named(NamedKey::Enter) => Some("yes"),
            Key::Named(NamedKey::Escape) => Some("later"),
            Key::Character(c) if c.as_str().eq_ignore_ascii_case("y") => Some("yes"),
            Key::Character(c) if c.as_str().eq_ignore_ascii_case("n") => Some("no"),
            Key::Character(c) if c.as_str().eq_ignore_ascii_case("l") => Some("later"),
            _ => None,
        };
        if let (Some(answer), false) = (answer, event.repeat) {
            self.native_answer(answer);
        }
        true
    }

    pub(crate) fn native_click(&mut self, action: &Action) -> bool {
        match action {
            Action::NativeYes => self.native_answer("yes"),
            Action::NativeNo => self.native_answer("no"),
            Action::NativeLater => self.native_answer("later"),
            _ => return false,
        }
        true
    }

    /// The Settings menu entry of a provider: `Native Claude Code login (~/.claude): on`.
    pub(crate) fn native_menu_label(&self, provider: &str) -> String {
        match self.ui.sidebar.native_entries.iter().find(|e| e.provider == provider) {
            Some(e) => format!("Use existing {} login ({}): {}", provider_name(provider), e.label.trim_start_matches("Native (").trim_end_matches(')'), if e.state == "on" { "on" } else if e.chosen == "off" { "off" } else { "not chosen" }),
            None => format!("Use existing {} login: ?", provider_name(provider)),
        }
    }

    pub(crate) fn toggle_native(&mut self, provider: &str) {
        let on = self.ui.sidebar.native_entries.iter().find(|e| e.provider == provider).is_some_and(|e| e.chosen == "on" || (e.chosen.is_empty() && e.state == "on"));
        let reply = self.host_call(json!({"op": "settings.set", format!("native_{provider}"): if on { "off" } else { "on" }}));
        self.ui.flash = Some(if reply["ok"] == json!(1) {
            self.send("{\"op\":\"account.list\"}");
            self.chats_sync_if_open(true);
            format!("{} login and chats: {}.", provider_name(provider), if on { "off" } else { "on" })
        } else {
            "Could not change that setting.".into()
        });
        self.dirty = true;
    }

    pub(super) fn native_prompt_json(&self) -> Value {
        let Some(p) = &self.ui.sidebar.native_prompt else { return Value::Null };
        let r = |r: &Option<Rect>| r.map_or(Value::Null, |r| json!(r));
        json!({"open": true, "entries": p.entries.iter().map(|e| json!({"provider": e.provider, "label": e.label, "chats": e.chats})).collect::<Vec<_>>(),
               "rects": {"yes": r(&p.geom.yes), "no": r(&p.geom.no), "later": r(&p.geom.later)}})
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_native_prompt(&mut self, buf: &mut [u32], width: usize, height: usize, top: f32, chat_top: f32, hint_y: f32) {
        let Some(prompt) = self.ui.sidebar.native_prompt.as_ref() else { return };
        let entries = prompt.entries.clone();
        let s = self.ui.scale.max(1.0);
        let lh = (self.line_h() * s).floor().max(18.0);
        let char_w = self.char_w() * s;
        darken_region(buf, width, height, 0, top.floor().max(0.0) as usize, width, hint_y.floor().max(0.0) as usize, 102);
        let modal_w = (600.0 * s).min(width as f32 - (32.0 * s).floor()).max(320.0 * s).floor();
        let pad = (20.0 * s).floor();
        let cols = (((modal_w - 2.0 * pad).max(120.0 * s)) / char_w).max(20.0) as usize;
        let mut lines: Vec<(String, bool)> = Vec::new();
        for e in &entries {
            let home = e.label.trim_start_matches("Native (").trim_end_matches(')');
            lines.push((format!("{}: {} chats in {home}", provider_name(&e.provider), e.chats.unwrap_or(0)), true));
        }
        let note = "Yes lists them in + New chat and the Chats tab; they run with their own login, which Lico never reads or copies. No keeps them out. The Settings menu changes it later.";
        let mut body: Vec<(String, bool)> = lines;
        body.extend(controls::wrap_cols(&clean(note), cols).into_iter().map(|l| (l, false)));
        let (header_h, btn_h, gap) = ((lh + 14.0 * s).floor(), (lh + 10.0 * s).floor(), (12.0 * s).floor());
        let question_lines: Vec<String> = controls::wrap_cols("Use your existing Claude/Codex login and its chats?", cols);
        let modal_h = header_h + gap + (question_lines.len() + body.len()) as f32 * lh + gap + btn_h + (16.0 * s).floor() + gap;
        let x0 = (((width as f32) - modal_w) * 0.5).floor().max(8.0);
        let x1 = (x0 + modal_w).min(width as f32 - 4.0).floor();
        let (ay0, ay1) = (top.floor(), hint_y.floor());
        let y0 = (ay0 + ((ay1 - ay0 - modal_h) * 0.4).floor()).max(ay0 + 4.0);
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
        let (ink, muted, accent) = (Color::rgb(0xEE, 0xF2, 0xF8), Color::rgb(0x98, 0xAC, 0xC2), Color::rgb(0x88, 0xC8, 0xFF));
        let clip = (x1 - pad) as i32;
        controls.text(fs, swash, buf, width, height, x0 + pad, (y0 + ((header_h - lh) * 0.5).floor()).max(y0), clip, "Your existing Claude Code / Codex", accent);
        let mut y = (y0 + header_h + gap).floor();
        for line in &question_lines {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, line, ink);
            y += lh;
        }
        for (line, strong) in &body {
            controls.text(fs, swash, buf, width, height, x0 + pad, y, clip, line, if *strong { ink } else { muted });
            y += lh;
        }
        let by = (y1 - (16.0 * s).floor() - btn_h).floor();
        rect(buf, width, height, (x0 + pad) as i32, (by - (7.0 * s).floor()) as i32, (modal_w - 2.0 * pad) as i32, 1, DIVIDER);
        let (bwd, g) = ((120.0 * s).floor(), (12.0 * s).floor());
        let later_r = [(x1 - pad - bwd).floor(), by, (x1 - pad).floor(), by + btn_h];
        let no_r = [later_r[0] - g - bwd, by, later_r[0] - g, by + btn_h];
        let yes_r = [no_r[0] - g - bwd, by, no_r[0] - g, by + btn_h];
        let on = Color::rgb(0xF4, 0xF7, 0xFB);
        draw_centered_btn(controls, fs, swash, buf, width, height, yes_r, "Yes (Y)", ALLOW_LIVE, on, lh, s, Action::NativeYes);
        draw_centered_btn(controls, fs, swash, buf, width, height, no_r, "No (N)", DENY, on, lh, s, Action::NativeNo);
        draw_centered_btn(controls, fs, swash, buf, width, height, later_r, "Ask later (L)", READ_LIVE, on, lh, s, Action::NativeLater);
        if let Some(p) = self.ui.sidebar.native_prompt.as_mut() {
            p.geom = PromptGeom { yes: Some(yes_r), no: Some(no_r), later: Some(later_r) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(provider: &str, state: &str, chats: Option<u64>) -> NativeEntry {
        NativeEntry { id: format!("native-{provider}"), provider: provider.into(), label: "Native (~/.x)".into(), state: state.into(), chosen: String::new(), chats }
    }

    #[test]
    fn the_question_is_about_the_providers_that_are_not_answered_and_have_chats_to_show() {
        let entries = vec![entry("claude", "ask", Some(12)), entry("codex", "ask", Some(0)), entry("codex", "on", Some(5)), entry("claude", "off", Some(3)), entry("codex", "ask", None)];
        let due = wanted(&entries);
        assert_eq!(due.len(), 1);
        assert_eq!((due[0].provider.as_str(), due[0].chats), ("claude", Some(12)));
        assert!(wanted(&[entry("claude", "on", Some(9))]).is_empty(), "an open Native needs no question");
    }

    #[test]
    fn the_entries_are_read_from_the_account_list() {
        let list = json!({"native": [{"account": "native-claude", "provider": "claude", "label": "Native (~/.claude)", "state": "ask", "chosen": null, "chats": 4}]});
        let got = parse_entries(&list);
        assert_eq!(got, vec![NativeEntry { id: "native-claude".into(), provider: "claude".into(), label: "Native (~/.claude)".into(), state: "ask".into(), chosen: String::new(), chats: Some(4) }]);
        assert!(parse_entries(&json!({"accounts": []})).is_empty());
    }
}
