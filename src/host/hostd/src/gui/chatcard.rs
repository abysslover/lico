//! A card of the Chats tab (`M22-P04c`): the provider as a picture over both lines, the title on the first line with the time of the last access at its right end (in a colour that says
//! how old it is), where the chat is on the second line with the state at its right end as a picture (a disc while the chat is open, green, orange or red by its memory; a ring when its
//! tab is closed; a square when it ended). No letters and no marks in words: the hint row says everything of the card under the pointer. The list, its fetch and its paging stay in
//! `sidebar_chats.rs`; this file is what a card says and how it is laid out and drawn.

use cosmic_text::{Color, FontSystem, SwashCache};
use serde_json::{json, Value};

use super::controls::{fill, Controls, Rect};
use super::sidebar::HOME_PATH;
use super::sidebar_chats::ChatEntry;
use super::sidebar_render::{clip_cols, tail_path};
use super::ui::icons::{self, Icon};
use super::ui::{memrow, Row};
use super::App;

const GREEN: Color = Color::rgb(0x68, 0xE0, 0xA8);
const GOLD: Color = Color::rgb(0xFA, 0xD2, 0x6E);
const GREY: Color = Color::rgb(0x98, 0xA6, 0xB8);

/// How old a chat is, in four steps: the colour of its time. One colour beyond a week: nobody follows the age of an old chat any finer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum AgeClass {
    Fresh, // under an hour
    Today, // under a day
    Week,  // under a week
    Old,
}

impl AgeClass {
    pub(super) fn name(self) -> &'static str {
        match self {
            AgeClass::Fresh => "fresh",
            AgeClass::Today => "today",
            AgeClass::Week => "week",
            AgeClass::Old => "old",
        }
    }

    pub(super) fn ink(self) -> Color {
        match self {
            AgeClass::Fresh => Color::rgb(0x5C, 0xC8, 0xFF),
            AgeClass::Today => Color::rgb(0x8F, 0xB4, 0xD8),
            AgeClass::Week => Color::rgb(0x8C, 0x98, 0xA8),
            AgeClass::Old => Color::rgb(0x5E, 0x6C, 0x7E),
        }
    }
}

/// The time of the last access in at most four characters: `now`, `5m`, `3h`, `4d`, `3w`, `5mo`, `1y`. Nothing is finer than a minute; beyond a week it is weeks, then months, then years.
pub(super) fn age_label(now: u64, updated_at: u64) -> String {
    if updated_at == 0 || now < updated_at + 60 {
        return "now".into();
    }
    let d = now - updated_at;
    match d {
        d if d < 3_600 => format!("{}m", d / 60),
        d if d < 86_400 => format!("{}h", d / 3_600),
        d if d < 7 * 86_400 => format!("{}d", d / 86_400),
        d if d < 30 * 86_400 => format!("{}w", d / (7 * 86_400)),
        d if d < 365 * 86_400 => format!("{}mo", d / (30 * 86_400)),
        d => format!("{}y", d / (365 * 86_400)),
    }
}

pub(super) fn age_class(now: u64, updated_at: u64) -> AgeClass {
    let d = now.saturating_sub(updated_at);
    match d {
        _ if updated_at == 0 => AgeClass::Old,
        d if d < 3_600 => AgeClass::Fresh,
        d if d < 86_400 => AgeClass::Today,
        d if d < 7 * 86_400 => AgeClass::Week,
        _ => AgeClass::Old,
    }
}

/// The age in a sentence: `just now`, `10m ago`.
fn age_words(now: u64, updated_at: u64) -> String {
    match age_label(now, updated_at).as_str() {
        "now" => "just now".to_string(),
        short => format!("{short} ago"),
    }
}

/// What a card says of its chat's session: open in a tab of this window (and how its memory is), its tab closed, or ended; nothing for a chat that is not open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum State {
    None,
    Open(lico_memguard::Level),
    Closed,
    Ended,
}

impl State {
    pub(super) fn name(self) -> &'static str {
        match self {
            State::None => "none",
            State::Open(_) => "open",
            State::Closed => "closed",
            State::Ended => "ended",
        }
    }

    /// The memory level of an open chat, as the word of the window state.
    pub(super) fn memory_name(self) -> Option<&'static str> {
        match self {
            State::Open(lico_memguard::Level::Ok) => Some("ok"),
            State::Open(lico_memguard::Level::Orange) => Some("orange"),
            State::Open(lico_memguard::Level::Red) => Some("red"),
            _ => None,
        }
    }

    /// The colour of the picture: the memory colours of the tab list for an open chat (green while it is fine).
    pub(super) fn ink(self) -> Color {
        match self {
            State::Open(level) => memrow::ink(level, GREEN),
            State::Closed => GOLD,
            State::Ended | State::None => GREY,
        }
    }
}

/// The state of a card. A chat that is open in a tab takes it from the tab list (the window has the figures; a stalled session is at least orange); a chat whose tab the list does not
/// know falls back to what the fetch said.
pub(super) fn state_of(it: &ChatEntry, row: Option<&Row>, now: u64) -> State {
    let state = row.map(|r| r.state.as_str()).or(it.open_state.as_deref());
    match state {
        Some("running") => State::Open(row.map_or(lico_memguard::Level::Ok, |r| {
            let level = memrow::level(r);
            if memrow::stalled(r, now) { level.max(lico_memguard::Level::Orange) } else { level }
        })),
        Some("disconnected") => State::Closed,
        Some("exited") => State::Ended,
        _ => State::None,
    }
}

/// What tells the list that a card has to be drawn again: the session, its state and its level, for every row.
pub(super) fn rows_sig(rows: &[Row], now: u64) -> String {
    rows.iter().map(|r| format!("{}:{}:{}:{}", r.session, r.state, memrow::level(r) as u8, u8::from(memrow::stalled(r, now)))).collect::<Vec<_>>().join(",")
}

/// Who a card belongs to, when that is worth a word: another account of the provider, or an ssh host. The provider's own default home says nothing.
fn who_of(it: &ChatEntry) -> Option<&str> {
    it.account_label.as_deref().or(it.ssh_host.as_deref()).filter(|l| !l.starts_with("Native ("))
}

/// Where a session is, short: `lico://home` for a Home session, else the last two components of its folder (`work/api`), whole when it has only one (`/srv`).
pub(super) fn folder_of(it: &ChatEntry) -> Option<String> {
    if it.global {
        return Some(HOME_PATH.to_string());
    }
    let cwd = it.cwd.as_deref()?;
    let mut tail: Vec<&str> = cwd.rsplit('/').filter(|c| !c.is_empty()).take(2).collect();
    tail.reverse();
    (!tail.is_empty()).then(|| tail.join("/"))
}

/// The second line of a card: `lico://home`, or the end of the folder as long as `cols` columns allow (the whole path when it fits); empty for a chat without a folder.
pub(super) fn folder_line(it: &ChatEntry, cols: usize) -> String {
    if it.provider == "browser" {
        return lico_browser_view::display_address(&it.provider_session, cols); // the page's address, cut in the middle as the bar cuts it
    }
    if it.global {
        return HOME_PATH.to_string();
    }
    it.cwd.as_deref().filter(|c| !c.is_empty() && *c != "/").map_or_else(String::new, |c| tail_path(c.trim_end_matches('/'), cols))
}

/// The title over the two lines of a card: line one up to `cols1` columns, and what does not fit on it (cut to `cols2` with an ellipsis) on line two. A wide character counts two columns, and a
/// line breaks after a space when there is one in its last 12 columns, so that a word is not cut when it need not be. A title that fits has no second line.
pub(super) fn title_lines(title: &str, cols1: usize, cols2: usize) -> (String, String) {
    let width = |c: char| if super::editor::wide(c) { 2 } else { 1 };
    let chars: Vec<char> = title.trim().chars().collect();
    let (mut used, mut cut) = (0, 0);
    while cut < chars.len() && used + width(chars[cut]) <= cols1 {
        used += width(chars[cut]);
        cut += 1;
    }
    if cut == chars.len() {
        return (title.trim().to_string(), String::new());
    }
    if chars[cut] != ' ' {
        let mut back = cut;
        let mut room = 12;
        while back > 0 && room > 0 && chars[back - 1] != ' ' {
            back -= 1;
            room -= width(chars[back]).min(room);
        }
        if back > 0 && chars[back - 1] == ' ' && room > 0 {
            cut = back;
        }
    }
    let first: String = chars[..cut].iter().collect();
    let rest: String = chars[cut..].iter().collect();
    (first.trim_end().to_string(), clip_cols(rest.trim_start(), cols2))
}

/// The sentence of the hint row for a card under the pointer: the title, the time, the state in words with its memory figure, the folder, who, the id; cut to `cols` columns with the title
/// giving way first.
pub(super) fn hint_of(it: &ChatEntry, state: State, row: Option<&Row>, now: u64, cols: usize) -> String {
    let rest = if it.provider == "browser" {
        format!("{} \u{00b7} {}", age_words(now, it.updated_at), it.provider_session) // a remembered page: the whole address, and a click opens it
    } else {
        session_rest(it, state, row, now)
    };
    let room = cols.saturating_sub(rest.chars().count() + 3).max(12);
    format!("{} \u{00b7} {rest}", clip_cols(&it.title, room)).chars().take(cols).collect()
}

/// The hint row of a session after its title: the time, the state in words with its memory figure, the folder, who, the id.
fn session_rest(it: &ChatEntry, state: State, row: Option<&Row>, now: u64) -> String {
    let mut rest = vec![age_words(now, it.updated_at)];
    let figure = row.map(|r| if r.rss_kb > 0 { r.rss_kb } else { r.memory_kb }).filter(|kb| *kb > 0).map(memrow::long);
    match state {
        State::Open(level) => {
            let tail = match level {
                lico_memguard::Level::Ok => "",
                lico_memguard::Level::Orange => ", memory high",
                lico_memguard::Level::Red => ", memory over the limit",
            };
            rest.push(format!("open{}{tail}", figure.map(|f| format!(", {f}")).unwrap_or_default()));
        }
        State::Closed => rest.push("tab closed, a click opens it again".into()),
        State::Ended => rest.push("ended".into()),
        State::None if it.running_elsewhere => rest.push("written by another program, cannot be resumed here".into()),
        State::None => {}
    }
    rest.extend(folder_of(it));
    rest.extend(who_of(it).map(str::to_string));
    rest.push(format!("#{}", &it.provider_session[..it.provider_session.len().min(8)]));
    rest.join(" \u{00b7} ")
}

/// Where the parts of a card are.
#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Layout {
    pub icon: Rect,
    pub title: Rect,
    pub time: Rect,
    pub folder: Rect,
    pub glyph: Rect,
}

/// The parts of a card `card` at scale `s`, lines `lh` high, with a time column `time_w` wide: the icon over both lines at the left, the title and the folder in the lines beside it, the time
/// at the right of the first line and the state's picture at the right of the second.
pub(super) fn layout(card: Rect, s: f32, lh: f32, time_w: f32) -> Layout {
    let h = card[3] - card[1];
    let isz = (h - 4.0 * s).max(8.0);
    let (ix, iy) = (card[0] + 5.0 * s, card[1] + (h - isz) * 0.5);
    let icon = [ix, iy, ix + isz, iy + isz];
    let text_x = icon[2] + 5.0 * s;
    let right = card[2] - 6.0 * s;
    let (l1, l2) = (card[1] + s, card[1] + lh + s);
    let time = [right - time_w, l1, right, l1 + lh];
    let gs = (lh * 0.6).round().max(6.0);
    let glyph = [right - gs, l2 + (lh - gs) * 0.5, right, l2 + (lh - gs) * 0.5 + gs];
    Layout {
        icon,
        title: [text_x, l1, (time[0] - 6.0 * s).max(text_x), l1 + lh],
        time,
        folder: [text_x, l2, (glyph[0] - 6.0 * s).max(text_x), l2 + lh],
        glyph,
    }
}

fn argb(c: Color) -> u32 {
    0xFF00_0000 | (u32::from(c.r()) << 16) | (u32::from(c.g()) << 8) | u32::from(c.b())
}

/// The picture of a state in `r`: a disc, a ring (`ground` is the colour of the card under it) or a small square; nothing for no state. Nothing is drawn outside `r`.
pub(super) fn draw_glyph(buf: &mut [u32], width: usize, height: usize, state: State, r: Rect, ground: u32) {
    let ink = argb(state.ink());
    let d = (r[2] - r[0]).min(r[3] - r[1]);
    let (cx, cy) = ((r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5);
    let disc = |buf: &mut [u32], radius: f32, col: u32| {
        for i in 0..(2.0 * radius).ceil() as usize {
            let y = (cy - radius).round() + i as f32;
            let half = (radius * radius - (y + 0.5 - cy).powi(2)).max(0.0).sqrt();
            fill(buf, width, height, [(cx - half).round().max(r[0]), y.max(r[1]), (cx + half).round().min(r[2]), (y + 1.0).min(r[3])], col);
        }
    };
    match state {
        State::None => {}
        State::Open(_) => disc(buf, d * 0.5, ink),
        State::Closed => {
            disc(buf, d * 0.5, ink);
            disc(buf, d * 0.5 - (d * 0.22).max(1.5), ground);
        }
        State::Ended => {
            let q = (d * 0.16).round();
            fill(buf, width, height, [(cx - d * 0.5 + q).round(), (cy - d * 0.5 + q).round(), (cx + d * 0.5 - q).round(), (cy + d * 0.5 - q).round()], ink);
        }
    }
}

/// The provider's picture: the ones of the New session wizard.
pub(super) fn icon_of(provider: &str) -> Icon {
    match provider {
        "claude" => Icon::Claude,
        "local" => Icon::Local,
        "browser" => Icon::Browser,
        _ => Icon::Codex,
    }
}

fn icon_ink(provider: &str) -> Color {
    match provider {
        "claude" => Color::rgb(0xF2, 0xAA, 0x6B),
        "local" => Color::rgb(0xA8, 0xB8, 0xCC),
        "browser" => Color::rgb(0x7F, 0xB4, 0xF2),
        _ => Color::rgb(0x7A, 0xE0, 0xB8),
    }
}

/// What was drawn for a card: what the window state and the tests read.
#[derive(Clone, Default)]
pub(super) struct Shown {
    pub layout: Layout,
    pub icon: &'static str,
    pub title_shown: String,
    pub title_line2: String, // what of a long title did not fit on the first line (then the folder is not on the card: the hint row and the path row have it)
    pub folder_shown: String,
    pub age: String,
    pub age_class: &'static str,
    pub state: &'static str,
    pub memory: Option<&'static str>,
}

impl Shown {
    pub(super) fn add_to(&self, v: &mut Value) {
        v["icon"] = json!(self.icon);
        v["title_shown"] = json!(self.title_shown);
        v["title_line2"] = json!(self.title_line2);
        v["folder_shown"] = json!(self.folder_shown);
        v["age"] = json!(self.age);
        v["age_class"] = json!(self.age_class);
        v["state"] = json!(self.state);
        v["memory"] = json!(self.memory);
        v["icon_rect"] = json!(self.layout.icon);
        v["title_rect"] = json!(self.layout.title);
        v["time_rect"] = json!(self.layout.time);
        v["folder_rect"] = json!(self.layout.folder);
        v["glyph_rect"] = json!(self.layout.glyph);
    }
}

/// Draws a card (`r`, on `fill`) and says what it drew. `col_w` is the width of one column at the window's scale.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_card(controls: &mut Controls, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, r: Rect, ground: u32, it: &ChatEntry, state: State, now: u64, s: f32, lh: f32, col_w: f32) -> Shown {
    let time_w = controls.width(fs, "12mo");
    let lay = layout(r, s, lh, time_w);
    let ink = Color::rgb(0xEA, 0xF0, 0xF7);
    icons::draw(buf, width, height, icon_of(&it.provider), lay.icon, argb(icon_ink(&it.provider)), ground);
    let cols = |r: Rect| ((r[2] - r[0]).max(24.0) / col_w) as usize;
    let (title_shown, title_line2) = title_lines(&it.title, cols(lay.title), cols(lay.folder));
    controls.text(fs, swash, buf, width, height, lay.title[0], lay.title[1], lay.title[2] as i32, &title_shown, ink);
    let (age, class) = (age_label(now, it.updated_at), age_class(now, it.updated_at));
    let w = controls.width(fs, &age);
    controls.text(fs, swash, buf, width, height, lay.time[2] - w, lay.time[1], r[2] as i32, &age, class.ink());
    // a title that does not fit one line goes on to the second (the person asked to see the title); one that does leaves it to the folder
    let folder_shown = if title_line2.is_empty() { folder_line(it, cols(lay.folder)) } else { String::new() };
    let (second, second_ink) = if title_line2.is_empty() { (&folder_shown, GREY) } else { (&title_line2, ink) };
    controls.text(fs, swash, buf, width, height, lay.folder[0], lay.folder[1], lay.folder[2] as i32, second, second_ink);
    draw_glyph(buf, width, height, state, lay.glyph, ground);
    Shown { layout: lay, icon: icon_of(&it.provider).name(), title_shown, title_line2, folder_shown, age, age_class: class.name(), state: state.name(), memory: state.memory_name() }
}

impl App {
    /// The hint row's sentence for the card under the pointer.
    pub(crate) fn chat_hover_hint(&self, cols: usize) -> Option<String> {
        let idx = self.ui.mem.chat_hover?;
        let it = self.ui.sidebar.chats.items.get(idx)?;
        let row = it.open_session.as_ref().and_then(|sid| self.ui.rows.iter().find(|r| &r.session == sid));
        let now = self.clock_now();
        Some(hint_of(it, state_of(it, row, now), row, now, cols))
    }
}

#[cfg(test)]
#[path = "chatcard_tests.rs"]
mod tests;
