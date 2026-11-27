//! The small shared control set of the hand-drawn window: text labels (cached once shaped), buttons and list rows, each drawn and hit-tested
//! here and nowhere else. The menu bar and its entries, the Allow / Deny buttons, the session list and the hint line all go through it, so
//! a box that is drawn is exactly the box a click is tested against.

use std::collections::HashMap;

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap};

use super::ui::{blit, rect};
use super::FONT_PX;

pub type Rect = [f32; 4]; // x0, y0, x1, y1 in window pixels

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Menu(usize),
    Item(usize, usize),
    Toolbar(usize),
    HostItem(usize),
    Session(String),
    Tab(String),
    CloseTab(String),
    CallTab(String),
    CloseCallTab(String),
    CallHangup,
    CallJumpSession(String),
    TabScroll(isize),
    BrowserTab(String),
    CloseBrowserTab(String),
    TerminalTab(String),
    CloseTerminalTab(String),
    FocusSessionDir,
    SidebarTab(u8),
    SidebarHost,
    SidebarHostItem(usize),
    SidebarDotfiles,
    SidebarRefresh,
    SidebarNewHere,
    SidebarNewChat,
    NewChatAccount(usize),
    NewChatMode(bool),
    NewChatOk,
    NewChatCancel,
    NewChatOptions,
    LlmScan,
    LlmClose,
    LlmAllUsers,
    LlmMore,
    PathCopy,
    WindowMax,
    LlmPick(usize),
    WizardPick(usize),
    /// A click on one of the five pictures of an account's page, which is the key `.0` (`M24-P03`).
    AccountAct(char),
    /// A click on picture `.0` of the providers (Claude, agy, Codex) on the screen of a new account (`M25-P11`).
    ProviderPick(usize),
    /// A click on a button of the sign-in screen, which is the key `.0` (`o` open the address, `c` copy it, `v` paste, `g` send the code, `r` try again; `M25-P13`).
    LoginAct(char),
    /// A click on box `.1` of the row at line `.0` of the wizard's second or third step (`M22-P07`).
    WizardChoose(usize, usize),
    SidebarFilter,
    SidebarFilterClear,
    SidebarGlobal,
    SidebarActiveDir,
    SidebarJumpActive,
    SidebarParent,
    SidebarEntry(usize),
    SidebarChatEntry(usize),
    SidebarPage(isize),
    SidebarNavBack,
    SidebarNavForward,
    SidebarBookmarkToggle,
    SidebarBookmarksBtn,
    SidebarBookmarkItem(usize),
    SidebarBookmarkRemove(usize),
    DirMenuItem(usize),
    MentionItem(usize),
    Allow,
    Deny,
    ReadCard,
    ModalBackdrop,
    StopRealScreen,
}

pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}

#[derive(Default)]
pub struct Controls {
    pub hits: Vec<Hit>,
    labels: HashMap<String, (Buffer, f32)>,
    pub scale: f32,
    pub font_px: f32,
    /// How many labels were shaped since the start (a label whose text and scale are unchanged is shaped once and reused).
    pub shapes: u64,
    /// The Allow and Deny buttons of the last frame were drawn dimmed (a card that may not be answered yet).
    pub dimmed: bool,
}

impl Controls {
    /// Start a frame: the boxes of the last one are forgotten, and shaped labels are dropped when the scale or text size changed or too many piled up.
    pub fn begin(&mut self, scale: f32, font_px: f32) {
        self.hits.clear();
        self.dimmed = false;
        let fp = if font_px <= 0.0 { FONT_PX } else { font_px };
        if (self.scale - scale).abs() > f32::EPSILON || (self.font_px - fp).abs() > f32::EPSILON || self.labels.len() > 600 {
            self.labels.clear();
        }
        self.scale = scale;
        self.font_px = fp;
    }

    fn label(&mut self, fs: &mut FontSystem, text: &str) -> &mut (Buffer, f32) {
        let s = self.scale;
        let fp = if self.font_px <= 0.0 { FONT_PX } else { self.font_px };
        let lh = fp + 4.0;
        if !self.labels.contains_key(text) {
            let mut b = Buffer::new(fs, Metrics::new(fp * s, lh * s));
            b.set_wrap(Wrap::None);
            b.set_size(Some(4000.0), Some(lh * s));
            b.set_text(text, &Attrs::new().family(Family::Monospace), Shaping::Advanced, None);
            b.shape_until_scroll(fs, true);
            let width = b.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
            self.shapes += 1;
            self.labels.insert(text.to_string(), (b, width));
        }
        self.labels.get_mut(text).unwrap()
    }

    /// Where each glyph of `text` is when it is drawn at x = 0: (first byte, end byte, left, right). The address bar's caret and mouse boxes are made from them (M22-P05a).
    pub fn glyph_spans(&mut self, fs: &mut FontSystem, text: &str) -> Vec<(usize, usize, f32, f32)> {
        self.label(fs, text).0.layout_runs().flat_map(|r| r.glyphs.iter().map(|g| (g.start, g.end, g.x, g.x + g.w)).collect::<Vec<_>>()).collect()
    }

    /// Gives the editor the geometry of `text` drawn as one row at `at` (x of the text's start, y of its top, its width, the scale): its caret, selection and boxes for the mouse.
    pub fn place_editor(&mut self, fs: &mut FontSystem, ed: &mut super::editor::Editor, text: &str, caret: Option<(usize, usize)>, at: [f32; 4]) {
        let entry = self.label(fs, text);
        ed.place(&entry.0, (0, 1, caret), at);
    }

    /// The width a text takes when drawn.
    pub fn width(&mut self, fs: &mut FontSystem, text: &str) -> f32 {
        self.label(fs, text).1
    }

    /// A text with its top left at (x, y), cut at `clip_x`; returns its width.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, x: f32, y: f32, clip_x: i32, text: &str, ink: Color) -> f32 {
        let entry = self.label(fs, text);
        let w = entry.1;
        blit(&mut entry.0, fs, swash, buf, width, height, x as i32, y as i32, clip_x, ink);
        w
    }

    /// A filled box with its label inside, and the action a click on it means.
    #[allow(clippy::too_many_arguments)]
    pub fn button(&mut self, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, r: Rect, label: &str, fill: u32, ink: Color, action: Action) {
        rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, fill);
        let pad = 6.0 * self.scale;
        self.text(fs, swash, buf, width, height, r[0] + pad, r[1], r[2] as i32, label, ink);
        self.hits.push(Hit { rect: r, action });
    }

    /// A row of a list: highlighted when selected, its text cut at the row's right edge.
    #[allow(clippy::too_many_arguments)]
    pub fn row(&mut self, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, r: Rect, label: &str, selected: bool, ink: Color, action: Action) {
        if selected {
            rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, 0xFF22_3A58);
        }
        let pad = 4.0 * self.scale;
        self.text(fs, swash, buf, width, height, r[0] + pad, r[1], (r[2] - 2.0) as i32, label, ink);
        self.hits.push(Hit { rect: r, action });
    }

    /// Only a hit box; for a thing that was drawn some other way.
    pub fn target(&mut self, r: Rect, action: Action) {
        self.hits.push(Hit { rect: r, action });
    }

    /// What a click at (x, y) means: the box drawn last (on top) wins.
    pub fn hit_at(&self, x: f32, y: f32) -> Option<Action> {
        self.hits.iter().rev().find(|h| x >= h.rect[0] && x < h.rect[2] && y >= h.rect[1] && y < h.rect[3]).map(|h| h.action.clone())
    }
}

/// Fill a box (used for backgrounds that belong to a control but carry no text).
pub fn fill(buf: &mut [u32], width: usize, height: usize, r: Rect, color: u32) {
    rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, color);
}

/// Wrap `text` into lines of at most `max_cols` display columns (`2` for wide CJK/emoji, `1` otherwise),
/// breaking at spaces when possible so approval prompts never run under the right-aligned Allow/Deny buttons.
pub fn wrap_cols(text: &str, max_cols: usize) -> Vec<String> {
    let cap = max_cols.max(12);
    let col_w = |c: char| if super::editor::wide(c) { 2 } else { 1 };
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cur_cols = 0usize;
    let mut last_space: Option<(usize, usize)> = None; // (byte_pos_of_space, cols_before_space)
    for ch in text.chars() {
        let w = col_w(ch);
        if cur_cols + w > cap && !cur.is_empty() {
            if ch == ' ' {
                out.push(std::mem::take(&mut cur));
                cur_cols = 0;
                last_space = None;
                continue;
            }
            if let Some((sp_byte, _)) = last_space.take() {
                let tail = cur[sp_byte + 1..].to_string();
                cur.truncate(sp_byte);
                out.push(std::mem::take(&mut cur));
                cur_cols = tail.chars().map(col_w).sum();
                cur = tail;
            } else {
                out.push(std::mem::take(&mut cur));
                cur_cols = 0;
            }
        }
        if ch == ' ' && !cur.is_empty() {
            last_space = Some((cur.len(), cur_cols));
        }
        cur.push(ch);
        cur_cols += w;
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}
