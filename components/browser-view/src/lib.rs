//! `lico-browser-view` (SPEC.md): the display-free model of one browser tab. It is driven with the person's intents (an address, a size, a pointer, a key) and the engine's events
//! (a frame, a navigation, a title) and answers with data to draw and effects for the host adapter to carry out. It opens no socket, draws nothing and reads no clock (the time is
//! given to `show` and `tick`). Depends on `lico-frame` only.

use lico_frame::{Image, Limits};

/// The addresses of the history of a tab: a person's back and forward, not an archive.
pub const MAX_HISTORY: usize = 100;
/// The longest address taken or shown: a real address is a few hundred characters; browsers stop near 2,000.
pub const MAX_URL: usize = 2048;
/// The longest title shown (characters): a tab label is short; the rest of a long title is not shown.
pub const MAX_TITLE: usize = 200;
/// The largest compressed frame kept: the cap of `lico-ws` and `lico-frame` (the largest frame the engine sent was 69,612 bytes).
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
/// The largest page viewport: it bounds one decoded picture to 9.2 MB of memory in Lico's process.
pub const MAX_VIEWPORT_W: u32 = 1920;
pub const MAX_VIEWPORT_H: u32 = 1200;
/// The smallest page viewport (a page cannot be laid out in less).
pub const MIN_VIEWPORT: u32 = 64;
/// How long a shown tab waits for a frame before it asks for one screenshot: a still page sends one frame at once, so half a second is a slow moment.
pub const FALLBACK_AFTER_MS: u64 = 500;
/// The most characters of committed text sent to the page at once.
pub const MAX_INSERT: usize = 4096;
/// The longest notice of a tab, in characters: a line of text, not a page (the host in it is cleaned and cut first).
pub const MAX_NOTICE: usize = 300;
/// The longest host name shown in a notice (a DNS name is at most 253 characters).
const MAX_NOTICE_HOST: usize = 253;
/// What every tab of an engine that is not the pinned release shows.
/// The line a tab shows when the person opened it for this machine's own services (it can reach those and no other site).
pub const LOCAL_NOTE: &str = "this tab can reach services on this machine and no other site";
pub const UNPINNED_NOTICE: &str = "running an engine that is not the pinned release";

// ---- text from outside -----------------------------------------------------------------------------------------------------------------------------

fn hidden_format_char(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2028}' | '\u{2029}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{061C}' | '\u{FEFF}')
}

/// Text that a page or an engine gave, made safe to show: control, bidirectional-override and zero-width characters removed, cut at `max_chars` (the last of them `…`).
pub fn clean(text: &str, max_chars: usize) -> String {
    let kept: Vec<char> = text.chars().filter(|c| !c.is_control() && !hidden_format_char(*c)).collect();
    if kept.len() <= max_chars {
        kept.into_iter().collect()
    } else {
        kept[..max_chars.saturating_sub(1)].iter().collect::<String>() + "…"
    }
}

// ---- the address ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlError {
    Empty,
    TooLong,
    /// Not `http` or `https`.
    Scheme,
    /// A user name or password in the address.
    Credentials,
    Invalid,
}

fn scheme_like(text: &str) -> Option<&str> {
    let end = text.find(':')?;
    let head = &text[..end];
    let mut chars = head.chars();
    let first = chars.next()?;
    (first.is_ascii_alphabetic() && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))).then_some(head)
}

/// What a person typed as an `http` or `https` address, or why not. Without a scheme the text is taken as `https://` plus the text (`host:port` is not a scheme).
pub fn parse_address(typed: &str) -> Result<String, UrlError> {
    let text = typed.trim();
    if text.is_empty() {
        return Err(UrlError::Empty);
    }
    if text.chars().count() > MAX_URL {
        return Err(UrlError::TooLong);
    }
    if text.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(UrlError::Invalid);
    }
    let (scheme, rest): (String, &str) = match scheme_like(text) {
        Some(head) => {
            let after = &text[head.len() + 1..];
            if let Some(rest) = after.strip_prefix("//") {
                (head.to_ascii_lowercase(), rest)
            } else {
                let port_part: &str = after.split(['/', '?', '#']).next().unwrap_or("");
                if !port_part.is_empty() && port_part.bytes().all(|b| b.is_ascii_digit()) {
                    ("https".to_string(), text) // host:port, not a scheme
                } else {
                    return Err(UrlError::Scheme);
                }
            }
        }
        None => ("https".to_string(), text),
    };
    if scheme != "http" && scheme != "https" {
        return Err(UrlError::Scheme);
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@') {
        return Err(UrlError::Credentials);
    }
    let (host, port): (&str, Option<&str>) = if let Some(inner) = authority.strip_prefix('[') {
        let close = inner.find(']').ok_or(UrlError::Invalid)?;
        let after = &inner[close + 1..];
        match after {
            "" => (&inner[..close], None),
            a if a.starts_with(':') => (&inner[..close], Some(&a[1..])),
            _ => return Err(UrlError::Invalid),
        }
    } else {
        match authority.rfind(':') {
            Some(i) => (&authority[..i], Some(&authority[i + 1..])),
            None => (authority, None),
        }
    };
    if host.is_empty() || host.chars().any(|c| matches!(c, '\\' | '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`')) {
        return Err(UrlError::Invalid);
    }
    if let Some(p) = port {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) || p.parse::<u32>().map_or(true, |n| n > 65535) {
            return Err(UrlError::Invalid);
        }
    }
    Ok(format!("{scheme}://{rest}"))
}

/// The text field of the address row: characters, a cursor, and the pre-edit of an input method (shown at the cursor, not yet part of the text).
#[derive(Clone, Debug, Default)]
pub struct AddressField {
    text: String,
    cursor: usize,
    preedit: String,
}

fn no_control(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

impl AddressField {
    pub fn new() -> AddressField {
        AddressField::default()
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    /// The cursor, in characters.
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn preedit(&self) -> &str {
        &self.preedit
    }
    fn byte_at(&self, chars: usize) -> usize {
        self.text.char_indices().nth(chars).map_or(self.text.len(), |(i, _)| i)
    }
    fn len(&self) -> usize {
        self.text.chars().count()
    }
    pub fn set_text(&mut self, text: &str) {
        self.text = no_control(text).chars().take(MAX_URL).collect();
        self.cursor = self.len();
        self.preedit.clear();
    }
    pub fn insert(&mut self, s: &str) {
        let room = MAX_URL.saturating_sub(self.len());
        let add: String = no_control(s).chars().take(room).collect();
        let at = self.byte_at(self.cursor);
        self.text.insert_str(at, &add);
        self.cursor += add.chars().count();
    }
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let (from, to) = (self.byte_at(self.cursor - 1), self.byte_at(self.cursor));
            self.text.replace_range(from..to, "");
            self.cursor -= 1;
        }
    }
    pub fn delete(&mut self) {
        if self.cursor < self.len() {
            let (from, to) = (self.byte_at(self.cursor), self.byte_at(self.cursor + 1));
            self.text.replace_range(from..to, "");
        }
    }
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.len());
    }
    pub fn home(&mut self) {
        self.cursor = 0;
    }
    pub fn end(&mut self) {
        self.cursor = self.len();
    }
    pub fn set_preedit(&mut self, s: &str) {
        self.preedit = no_control(s);
    }
    pub fn clear_preedit(&mut self) {
        self.preedit.clear();
    }
    /// An input method committed `s`: the pre-edit goes and `s` is inserted.
    pub fn commit(&mut self, s: &str) {
        self.preedit.clear();
        self.insert(s);
    }
    /// The text with the pre-edit at the cursor, as it is drawn.
    pub fn display(&self) -> String {
        let at = self.byte_at(self.cursor);
        format!("{}{}{}", &self.text[..at], self.preedit, &self.text[at..])
    }
}

// ---- the tab ---------------------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Ready,
    Loading,
    /// The engine is not installed: what the tab shows is the expected hash and the pinned release.
    Absent { expected_sha256: String, release: String },
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Page,
    Address,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Pressed,
    Released,
    Moved,
    Wheel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    None,
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Down,
    Up,
    Char,
}

/// What the host adapter is asked to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    SetViewport { width: u32, height: u32 },
    StartScreencast,
    StopScreencast,
    /// The number of the frame to acknowledge.
    Ack(u64),
    CaptureScreenshot,
    Navigate(String),
    Reload,
    Mouse { kind: MouseKind, x: f64, y: f64, button: Button, clicks: u32, delta: f64 },
    Key { kind: KeyKind, key: String, code: String, text: String },
    InsertText(String),
}

/// Why the fetch guard refused a destination (0.4.0), as the guard says it (the adapter maps its own reason to this one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Other,
    NotPublic,
    LoopbackOnly,
    Port,
    Denied,
}

/// What the host adapter tells the model.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    Started,
    Navigated(String),
    LoadStarted,
    LoadFinished,
    Title(String),
    Frame { id: u64, png: Vec<u8> },
    /// A screenshot taken because no frame came (it has no number to acknowledge).
    Screenshot(Vec<u8>),
    TargetGone,
    Crashed,
    Missing { expected_sha256: String, release: String },
    NoFontFound,
    /// The fetch guard refused or could not complete a fetch for this page (a status of its own: 403, 502, 504) of `host`. The engine shows a blank page and no error.
    FetchRefused { status: u16, host: String, port: u16, why: Refusal },
    /// The engine refused the address before any fetch because it is on this computer or a private network (a number, or the name `localhost`: MEASURED, P06).
    LocalAddressRefused,
    /// The engine is not the pinned release (the owner's setting says so).
    UnpinnedEngine,
    /// The person opened this tab for this machine's own services (an explicit act; never from a page or a link).
    LocalTab,
}

pub struct Tab {
    phase: Phase,
    focus: Focus,
    url: String,
    title: String,
    history: Vec<String>,
    index: usize,
    shown: bool,
    screencasting: bool,
    viewport: (u32, u32),
    capped: bool,
    pending_url: Option<String>,
    address: AddressField,
    no_font: bool,
    newest: Option<Vec<u8>>,
    newest_seq: u64,
    decoded: Option<Image>,
    decoded_seq: u64,
    decodes: u64,
    dropped: u64,
    too_big: u64,
    decode_errors: u64,
    shown_at_ms: u64,
    frame_since_show: bool,
    fallback_sent: bool,
    notice: Option<String>,
    unpinned: bool,
    local: bool,
}

fn clamp_viewport(area: (u32, u32)) -> ((u32, u32), bool) {
    let v = (area.0.clamp(MIN_VIEWPORT, MAX_VIEWPORT_W), area.1.clamp(MIN_VIEWPORT, MAX_VIEWPORT_H));
    (v, area.0 > MAX_VIEWPORT_W || area.1 > MAX_VIEWPORT_H)
}

impl Default for Tab {
    fn default() -> Tab {
        Tab::new()
    }
}

impl Tab {
    pub fn new() -> Tab {
        Tab {
            phase: Phase::Starting,
            focus: Focus::Page,
            url: String::new(),
            title: String::new(),
            history: Vec::new(),
            index: 0,
            shown: false,
            screencasting: false,
            viewport: (MIN_VIEWPORT, MIN_VIEWPORT),
            capped: false,
            pending_url: None,
            address: AddressField::new(),
            no_font: false,
            newest: None,
            newest_seq: 0,
            decoded: None,
            decoded_seq: 0,
            decodes: 0,
            dropped: 0,
            too_big: 0,
            decode_errors: 0,
            shown_at_ms: 0,
            frame_since_show: false,
            fallback_sent: false,
            notice: None,
            unpinned: false,
            local: false,
        }
    }

    // ---- what the painter and the host read ----
    pub fn phase(&self) -> &Phase {
        &self.phase
    }
    /// A line the person should read about the page that is loading or loaded (cleared when the tab leaves it); never page content.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
    pub fn unpinned(&self) -> bool {
        self.unpinned
    }
    /// The lines the tab shows as its help (a new tab, and F1): the identity, the ports that load, what is refused, how tabs opened elsewhere appear, Korean input and the command
    /// for a first look at a real site. A tab for this machine's own services says so first.
    pub fn help_lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.local {
            v.push(LOCAL_NOTE.to_string());
        }
        for line in [
            "Pages are told this is Lico with the Obscura engine.",
            "A page may still see a Windows platform and vendor: the engine decides those.",
            "Only http and https load, on ports 80, 443, 8080 and 8443.",
            "Addresses on this computer or a private network are refused, by number and as localhost.",
            "To reach this machine's own services, open a tab for them from the + menu.",
            "A tab opened with the command line appears here at the next list (the + menu, or window focus).",
            "Korean is typed with the input method: the address row shows the composition.",
            "The page gets the finished text only.",
            "To look at a real site yourself: lico browser open https://SITE",
        ] {
            v.push(line.to_string());
        }
        v
    }

    /// Whether the person opened this tab for this machine's own services (`LOCAL_NOTE` is what the tab shows).
    pub fn local(&self) -> bool {
        self.local
    }
    pub fn loading(&self) -> bool {
        self.phase == Phase::Loading
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn history(&self) -> &[String] {
        &self.history
    }
    pub fn can_back(&self) -> bool {
        self.index > 0 && !self.history.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.history.len()
    }
    pub fn shown(&self) -> bool {
        self.shown
    }
    pub fn viewport(&self) -> (u32, u32) {
        self.viewport
    }
    pub fn viewport_capped(&self) -> bool {
        self.capped
    }
    pub fn no_font(&self) -> bool {
        self.no_font
    }
    pub fn focus(&self) -> Focus {
        self.focus
    }
    pub fn focus_address(&mut self) {
        self.focus = Focus::Address;
    }
    pub fn focus_page(&mut self) {
        self.focus = Focus::Page;
    }
    pub fn address(&self) -> &AddressField {
        &self.address
    }
    pub fn address_mut(&mut self) -> &mut AddressField {
        &mut self.address
    }
    pub fn dropped_frames(&self) -> u64 {
        self.dropped
    }
    pub fn too_big_frames(&self) -> u64 {
        self.too_big
    }
    pub fn decode_errors(&self) -> u64 {
        self.decode_errors
    }
    /// How many frames were decoded.
    pub fn decodes(&self) -> u64 {
        self.decodes
    }
    pub fn decoded_bytes(&self) -> usize {
        self.decoded.as_ref().map_or(0, |i| i.pixels.len() * 4)
    }
    pub fn kept_bytes(&self) -> usize {
        self.newest.as_ref().map_or(0, Vec::len)
    }

    fn active(&self) -> bool {
        matches!(self.phase, Phase::Ready | Phase::Loading)
    }

    // ---- intents ----

    fn start_effects(&mut self) -> Vec<Effect> {
        self.screencasting = true;
        vec![Effect::SetViewport { width: self.viewport.0, height: self.viewport.1 }, Effect::StartScreencast]
    }

    /// The tab is shown in an area of this size at time `now_ms`.
    pub fn show(&mut self, area: (u32, u32), now_ms: u64) -> Vec<Effect> {
        (self.viewport, self.capped) = clamp_viewport(area);
        self.shown = true;
        self.shown_at_ms = now_ms;
        self.frame_since_show = false;
        self.fallback_sent = false;
        if self.active() {
            self.start_effects()
        } else {
            Vec::new()
        }
    }

    /// The tab is hidden: its screencast stops, its decoded picture is freed and its newest compressed frame is kept.
    pub fn hide(&mut self) -> Vec<Effect> {
        let was = self.screencasting;
        self.shown = false;
        self.screencasting = false;
        self.decoded = None;
        self.decoded_seq = 0;
        if was && self.active() {
            vec![Effect::StopScreencast]
        } else {
            Vec::new()
        }
    }

    pub fn resize(&mut self, area: (u32, u32)) -> Vec<Effect> {
        if !self.shown {
            return Vec::new();
        }
        let (v, capped) = clamp_viewport(area);
        self.capped = capped;
        if v == self.viewport {
            return Vec::new();
        }
        self.viewport = v;
        if self.active() {
            vec![Effect::SetViewport { width: v.0, height: v.1 }]
        } else {
            Vec::new()
        }
    }

    fn push_history(&mut self, url: &str) {
        if self.history.get(self.index).is_some_and(|u| u == url) {
            return;
        }
        if !self.history.is_empty() {
            self.history.truncate(self.index + 1);
        }
        self.history.push(url.to_string());
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
        self.index = self.history.len() - 1;
    }

    /// The person submitted an address.
    pub fn submit_address(&mut self, typed: &str) -> Result<Vec<Effect>, UrlError> {
        let url = parse_address(typed)?;
        if matches!(self.phase, Phase::Absent { .. } | Phase::Failed(_)) {
            return Ok(Vec::new());
        }
        self.notice = None;
        self.push_history(&url);
        self.url = url.clone();
        self.address.set_text(&url);
        if self.phase == Phase::Starting {
            self.pending_url = Some(url);
            return Ok(Vec::new());
        }
        self.phase = Phase::Loading;
        Ok(vec![Effect::Navigate(url)])
    }

    pub fn back(&mut self) -> Vec<Effect> {
        if !self.can_back() || !self.active() {
            return Vec::new();
        }
        self.index -= 1;
        self.go_to_index()
    }

    pub fn forward(&mut self) -> Vec<Effect> {
        if !self.can_forward() || !self.active() {
            return Vec::new();
        }
        self.index += 1;
        self.go_to_index()
    }

    fn go_to_index(&mut self) -> Vec<Effect> {
        let url = self.history[self.index].clone();
        self.notice = None;
        self.url = url.clone();
        self.phase = Phase::Loading;
        vec![Effect::Navigate(url)]
    }

    pub fn reload(&mut self) -> Vec<Effect> {
        if self.active() {
            self.notice = None;
            self.phase = Phase::Loading;
            vec![Effect::Reload]
        } else {
            Vec::new()
        }
    }

    // ---- what the engine reports ----

    fn keep_frame(&mut self, png: Vec<u8>) {
        if png.len() > MAX_FRAME_BYTES {
            self.too_big += 1;
            return;
        }
        if self.newest.is_some() && self.newest_seq != self.decoded_seq {
            self.dropped += 1; // the frame it replaces was never painted
        }
        self.newest = Some(png);
        self.newest_seq += 1;
        self.frame_since_show = true;
    }

    pub fn engine_event(&mut self, event: EngineEvent) -> Vec<Effect> {
        match event {
            EngineEvent::Started => {
                self.phase = Phase::Ready;
                let mut fx = if self.shown { self.start_effects() } else { Vec::new() };
                if let Some(url) = self.pending_url.take() {
                    self.phase = Phase::Loading;
                    fx.push(Effect::Navigate(url));
                }
                fx
            }
            EngineEvent::Navigated(url) => {
                let url = clean(&url, MAX_URL);
                self.push_history(&url);
                self.url = url.clone();
                if self.focus != Focus::Address {
                    self.address.set_text(&url);
                }
                Vec::new()
            }
            EngineEvent::LoadStarted => {
                if self.active() {
                    self.phase = Phase::Loading;
                }
                Vec::new()
            }
            EngineEvent::LoadFinished => {
                if self.active() {
                    self.phase = Phase::Ready;
                }
                Vec::new()
            }
            EngineEvent::Title(t) => {
                self.title = clean(&t, MAX_TITLE);
                Vec::new()
            }
            EngineEvent::Frame { id, png } => {
                self.keep_frame(png);
                vec![Effect::Ack(id)]
            }
            EngineEvent::Screenshot(png) => {
                self.keep_frame(png);
                Vec::new()
            }
            EngineEvent::TargetGone => {
                self.phase = Phase::Failed("the page was closed by the engine".into());
                self.screencasting = false;
                Vec::new()
            }
            EngineEvent::Crashed => {
                self.phase = Phase::Failed("the browser engine stopped".into());
                self.screencasting = false;
                Vec::new()
            }
            EngineEvent::Missing { expected_sha256, release } => {
                self.phase = Phase::Absent { expected_sha256, release };
                Vec::new()
            }
            EngineEvent::NoFontFound => {
                self.no_font = true;
                Vec::new()
            }
            EngineEvent::FetchRefused { status, host, port, why } => {
                let host = clean(&host, MAX_NOTICE_HOST);
                let line = match (status, why) {
                    (403, Refusal::Port) => format!("{host}: the port {port} is not one of 80, 443, 8080 and 8443"),
                    (403, Refusal::NotPublic) => format!("{host}: this address is on this computer or a private network, which this tab does not open"),
                    (403, Refusal::LoopbackOnly) => format!("{host}: this tab opens only this machine's own services"),
                    (403, Refusal::Denied) => format!("{host}: this port belongs to Lico and no page may reach it"),
                    (502, _) => format!("{host}: the address could not be found or reached"),
                    (504, _) => format!("{host}: the address took too long"),
                    (other, _) => format!("{host}: the page could not be fetched (status {other})"),
                };
                self.notice = Some(clean(&line, MAX_NOTICE));
                Vec::new()
            }
            EngineEvent::LocalAddressRefused => {
                self.notice = Some("the engine refuses addresses on this computer or a private network, by number and as localhost".into());
                Vec::new()
            }
            EngineEvent::UnpinnedEngine => {
                self.unpinned = true;
                Vec::new()
            }
            EngineEvent::LocalTab => {
                self.local = true;
                Vec::new()
            }
        }
    }

    /// The time passes: a shown tab that has had no frame since it was shown asks once for a screenshot.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Effect> {
        if self.shown && self.active() && !self.frame_since_show && !self.fallback_sent && now_ms.saturating_sub(self.shown_at_ms) >= FALLBACK_AFTER_MS {
            self.fallback_sent = true;
            return vec![Effect::CaptureScreenshot];
        }
        Vec::new()
    }

    /// The picture to paint: the newest frame, decoded now if it has not been (and only for a shown tab).
    pub fn paint(&mut self) -> Option<&Image> {
        if !self.shown {
            return None;
        }
        if self.newest_seq != self.decoded_seq {
            if let Some(png) = &self.newest {
                let limits = Limits { max_input: MAX_FRAME_BYTES, max_pixels: (MAX_VIEWPORT_W * MAX_VIEWPORT_H) as usize };
                match lico_frame::decode(png, &limits) {
                    Ok(img) => {
                        self.decoded = Some(img);
                        self.decodes += 1;
                    }
                    Err(_) => self.decode_errors += 1,
                }
            }
            self.decoded_seq = self.newest_seq;
        }
        self.decoded.as_ref()
    }

    // ---- the person's input ----

    /// A point of the shown area (from its top-left corner) as a point of the page, if it is inside the viewport.
    pub fn map_point(&self, x: i32, y: i32) -> Option<(f64, f64)> {
        (x >= 0 && y >= 0 && (x as u32) < self.viewport.0 && (y as u32) < self.viewport.1).then_some((x as f64, y as f64))
    }

    pub fn mouse(&mut self, kind: MouseKind, x: i32, y: i32, button: Button, clicks: u32, delta: f64) -> Option<Effect> {
        if !self.shown || !self.active() {
            return None;
        }
        let (px, py) = self.map_point(x, y)?;
        Some(Effect::Mouse { kind, x: px, y: py, button, clicks, delta })
    }

    pub fn key(&mut self, kind: KeyKind, key: &str, code: &str, text: &str) -> Option<Effect> {
        if !self.shown || !self.active() || self.focus != Focus::Page {
            return None;
        }
        Some(Effect::Key { kind, key: key.to_string(), code: code.to_string(), text: text.to_string() })
    }

    /// Text an input method committed, for the page.
    pub fn text(&mut self, s: &str) -> Option<Effect> {
        if !self.shown || !self.active() || self.focus != Focus::Page {
            return None;
        }
        let t: String = no_control(s).chars().take(MAX_INSERT).collect();
        (!t.is_empty()).then_some(Effect::InsertText(t))
    }
}
