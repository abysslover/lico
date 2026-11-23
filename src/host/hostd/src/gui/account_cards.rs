//! The account screen as pictures (`M24-P03`). The list of accounts is a card for each account (the provider's picture, its label, a star on the default one, a disc for what is left of its
//! quota) and a card with a plus for a new one; the page of one account is five pictures (a key to sign in, a pencil to rename, a star to make it the default, a bin to remove it and a bin
//! with a cross to remove it and end its sessions), each a target of its own. The text lines stay the model (the keys `1`..`9`, `A`, `S`, `R`, `F`, `D` and `E`, the dump and the notices read
//! them): a veil of the window's own ground covers them and the pictures are painted over it, as the wizard's boxes are (`wizard_cards.rs`). Nothing here reads the host.

use cosmic_text::{Color, SwashCache};

use super::super::super::controls::{self, Action};
use super::super::super::panel::Step;
use super::super::super::{App, BACKGROUND};
use super::icons::{self, Icon};
use super::picker::{draw_tiles, tile_rects, Look, FILL, INK};

const DIM: u32 = 0xFF8A_94A0;
const OK: u32 = 0xFF68_E0A8;
const LOW: u32 = 0xFFE0_A040;
const CRITICAL: u32 = 0xFFE0_5A5A;
/// How many accounts the list shows as cards: the digit keys name them (`1`..`9`).
pub(crate) const MAX_CARDS: usize = 9;

/// What is left of an account's quota, as a colour: none when the host does not say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    None,
    Ok,
    Low,
    Critical,
}

/// One card of the list: the line of the text it covers, the picture, what it says, and whether it is the default and how much is left.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AcctCard {
    pub(crate) line: usize,
    pub(crate) icon: Icon,
    pub(crate) text: String,
    pub(crate) default: bool,
    pub(crate) tone: Tone,
    /// The card with the plus: a new account.
    pub(crate) plus: bool,
}

/// One of the five pictures of an account's page: the key that does the same, its word, and whether a second click is wanted (a removal that was asked for once).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AcctButton {
    pub(crate) icon: Icon,
    pub(crate) key: char,
    pub(crate) word: &'static str,
    pub(crate) pending: bool,
}

/// The tone of what is left: the host's own words for it are `CRITICAL` under 5 percent and `LOW` under 20 (`format_quota_detail`), of the weekly figure and the five-hour one.
pub(crate) fn tone_of(weekly: Option<u8>, short: Option<u8>) -> Tone {
    match [weekly, short].into_iter().flatten().min() {
        None => Tone::None,
        Some(left) if left < 5 => Tone::Critical,
        Some(left) if left < 20 => Tone::Low,
        Some(_) => Tone::Ok,
    }
}

fn icon_of(provider: &str) -> Icon {
    match provider {
        "claude" => Icon::Claude,
        "codex" => Icon::Codex,
        "local" => Icon::Local,
        _ => Icon::Account,
    }
}

impl App {
    /// The cards of the list (`Step::Accounts`), in the order of the lines they cover: an account is the line of its number, and the card of a new account is the line of the key `A` (two lines
    /// under the last account, or under the sentence that says there is none). Empty on any other screen.
    pub(crate) fn account_cards(&self) -> Vec<AcctCard> {
        let Some(p) = self.ui.panel.as_ref().filter(|p| p.step == Step::Accounts) else { return Vec::new() };
        let mut cards: Vec<AcctCard> = p
            .accounts
            .iter()
            .take(MAX_CARDS)
            .enumerate()
            .map(|(line, a)| {
                let (weekly, short) = self.ui.quotas.get(&a.0).copied().unwrap_or((None, None));
                AcctCard { line, icon: icon_of(&a.1), text: self.acct_display(a), default: a.3, tone: tone_of(weekly, short), plus: false }
            })
            .collect();
        cards.push(AcctCard { line: p.accounts.len().clamp(1, MAX_CARDS) + 1, icon: Icon::Plus, text: "New account".into(), default: false, tone: Tone::None, plus: true });
        cards
    }

    /// The five pictures of an account's page (`Step::Actions`): the same five things as the keys `S`, `R`, `F`, `D` and `E`. Empty on any other screen.
    pub(crate) fn account_buttons(&self) -> Vec<AcctButton> {
        let Some(p) = self.ui.panel.as_ref().filter(|p| p.step == Step::Actions) else { return Vec::new() };
        [(Icon::Key, 's', "Sign in"), (Icon::Pencil, 'r', "Rename"), (Icon::Star, 'f', "Default"), (Icon::Trash, 'd', "Remove"), (Icon::TrashX, 'e', "Wipe")]
            .into_iter()
            .map(|(icon, key, word)| AcctButton { icon, key, word, pending: p.confirm == Some(key) })
            .collect()
    }

    /// A click on one of the five pictures of an account's page: the same as the key that does it. True when the action was one of these. (A click on a card of the list needs no target of its
    /// own: a card covers the line of its text, and the click on that line already opens the account or adds one, `accounts_or_label_click`.)
    pub(crate) fn account_click(&mut self, action: &Action) -> bool {
        let Action::AccountAct(key) = action else { return false };
        self.panel_choice(Step::Actions, *key);
        self.dirty = true;
        true
    }

    /// The cards of the list, or the pictures of an account's page, over the lines of the text they stand for.
    pub(super) fn account_paint(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, s: f32) {
        self.ui.toolbar_geom.acct.clear();
        let step = self.ui.panel.as_ref().map(|p| p.step);
        match step {
            Some(Step::Accounts) => self.account_list_paint(buf, width, height, list_w, s),
            Some(Step::Actions) => self.account_page_paint(buf, width, height, list_w, s),
            _ => {}
        }
    }

    fn line_range(&self, line: usize) -> Option<(f32, f32)> {
        self.ui.hit_lines.iter().find(|l| l.2 == line).map(|l| (l.0, l.1))
    }

    fn account_list_paint(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, s: f32) {
        let cards = self.account_cards();
        let none_yet = self.ui.panel.as_ref().is_some_and(|p| p.accounts.is_empty());
        let (x_text, char_w) = (list_w as f32 + 4.0 * s, self.char_w() * s);
        let sentence = if none_yet { self.line_range(0) } else { None };
        let lines: Vec<Option<(f32, f32)>> = cards.iter().map(|c| self.line_range(c.line)).collect();
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        if let Some((y0, y1)) = sentence {
            controls::fill(buf, width, height, [list_w as f32 + 1.0, y0, width as f32, y1], BACKGROUND); // "No account yet: ..." is said by the empty list and the plus
        }
        let mut rects = Vec::new();
        for (card, range) in cards.iter().zip(&lines) {
            let Some(&(y0, y1)) = range.as_ref() else { continue };
            controls::fill(buf, width, height, [list_w as f32 + 1.0, y0, width as f32, y1], BACKGROUND); // the veil over the text of the line
            let air = s.round().max(1.0);
            let h = (y1 - y0) - 2.0 * air;
            let label_h = (controls.font_px + 4.0) * controls.scale;
            let pad = (8.0 * s).round();
            let text_w = controls.width(fs, &card.text);
            let want = pad * 3.0 + h + text_w + if card.default { h } else { 0.0 } + if card.tone != Tone::None { h } else { 0.0 };
            let right = (x_text + want.max(18.0 * char_w)).min(width as f32 - 4.0 * s);
            let r = [x_text, y0 + air, right, y1 - air];
            controls::fill(buf, width, height, r, FILL);
            let x = r[0] + pad;
            icons::draw(buf, width, height, card.icon, [x, r[1], x + h, r[3]], INK, FILL);
            let x = x + h + pad;
            let x = x + controls.text(fs, swash, buf, width, height, x, r[1] + (h - label_h) * 0.5, (r[2] - 2.0 * pad) as i32, &card.text, Color(INK));
            if card.default {
                icons::draw(buf, width, height, Icon::Star, [x + pad * 0.5, r[1], x + pad * 0.5 + h, r[3]], LOW, FILL);
            }
            if card.tone != Tone::None {
                let colour = match card.tone {
                    Tone::Critical => CRITICAL,
                    Tone::Low => LOW,
                    _ => OK,
                };
                let (cx, cy, radius) = (r[2] - pad - h * 0.25, (r[1] + r[3]) * 0.5, h * 0.2);
                let rows = (2.0 * radius).ceil() as i32;
                for k in 0..rows {
                    let dy = k as f32 + 0.5 - radius;
                    let half = (radius * radius - dy * dy).max(0.0).sqrt();
                    controls::fill(buf, width, height, [cx - half, cy - radius + k as f32, cx + half, cy - radius + k as f32 + 1.0], colour);
                }
            }
            rects.push(r);
        }
        self.ui.toolbar_geom.acct = rects;
    }

    fn account_page_paint(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, s: f32) {
        let buttons = self.account_buttons();
        let band = [self.line_range(0), self.line_range(buttons.len().saturating_sub(1))];
        let (Some(first), Some(last)) = (band[0], band[1]) else { return };
        controls::fill(buf, width, height, [list_w as f32 + 1.0, first.0, width as f32, last.1], BACKGROUND); // the veil over the five lines of keys
        let rects = tile_rects([list_w as f32 + 4.0 * s, first.0, width as f32 - 4.0 * s, last.1], s, buttons.len());
        let (mouse, char_w) = ((self.ui.mouse.0 as f32, self.ui.mouse.1 as f32), self.char_w() * s);
        let hover = rects.iter().position(|r| mouse.0 >= r[0] && mouse.0 < r[2] && mouse.1 >= r[1] && mouse.1 < r[3]);
        let looks: Vec<Look> = buttons.iter().enumerate().map(|(i, b)| Look { icon: b.icon, enabled: true, focused: b.pending, hovered: hover == Some(i) }).collect();
        draw_tiles(buf, width, height, s, &rects, &looks);
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let label_h = (controls.font_px + 4.0) * controls.scale;
        for (b, r) in buttons.iter().zip(&rects) {
            let word = if b.pending { "Sure?" } else { b.word };
            let w = controls.width(fs, word);
            let x = (r[0] + (r[2] - r[0] - w) * 0.5).max(r[0] + char_w * 0.25);
            controls.text(fs, swash, buf, width, height, x, r[3] - label_h - 2.0 * s, r[2] as i32, word, Color(if b.pending { INK } else { DIM }));
            controls.target(*r, Action::AccountAct(b.key));
        }
        self.ui.toolbar_geom.tiles = rects;
        self.ui.toolbar_geom.tile_hover = hover;
    }

    /// The account screen for the window dump: the cards of the list and the pictures of a page with the rectangles they were painted at (none before the first frame).
    pub(crate) fn acct_json(&self) -> serde_json::Value {
        use serde_json::json;
        let rect = |r: Option<&[f32; 4]>| r.map(|r| json!(r));
        let cards: Vec<_> = self
            .account_cards()
            .iter()
            .enumerate()
            .map(|(i, c)| json!({"line": c.line, "icon": c.icon.name(), "text": c.text, "default": c.default, "tone": format!("{:?}", c.tone).to_lowercase(), "plus": c.plus, "rect": rect(self.ui.toolbar_geom.acct.get(i))}))
            .collect();
        let buttons: Vec<_> = self
            .account_buttons()
            .iter()
            .enumerate()
            .map(|(i, b)| json!({"icon": b.icon.name(), "key": b.key.to_string(), "word": b.word, "pending": b.pending, "rect": rect(self.ui.toolbar_geom.tiles.get(i))}))
            .collect();
        json!({"cards": cards, "buttons": buttons})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::scroll_oracle::app;

    fn acct(id: &str, provider: &str, label: &str, default: bool) -> (String, String, String, bool) {
        (id.into(), provider.into(), label.into(), default)
    }

    /// A window with the accounts screen open on these accounts (no host: they are put in by hand).
    fn list(accounts: Vec<(String, String, String, bool)>) -> App {
        let mut app = app(1.0, false);
        app.panel_open(Step::Accounts);
        app.ui.panel.as_mut().unwrap().accounts = accounts;
        app
    }

    fn three() -> App {
        list(vec![acct("a1", "claude", "Work", true), acct("a2", "codex", "Home", true), acct("a3", "claude", "Other", false)])
    }

    #[test]
    fn what_is_left_of_a_quota_is_a_tone_by_the_least_of_the_two_figures() {
        assert_eq!(tone_of(None, None), Tone::None);
        assert_eq!(tone_of(Some(3), None), Tone::Critical);
        assert_eq!(tone_of(Some(100), Some(4)), Tone::Critical, "the least counts, the short figure too");
        assert_eq!(tone_of(Some(5), None), Tone::Low, "under 5 is critical, 5 is not");
        assert_eq!(tone_of(Some(19), Some(80)), Tone::Low);
        assert_eq!(tone_of(None, Some(20)), Tone::Ok);
        assert_eq!(tone_of(Some(90), Some(60)), Tone::Ok);
    }

    #[test]
    fn the_list_is_a_card_for_each_account_in_the_line_of_its_number_and_a_plus_two_lines_under_the_last() {
        let mut app = three();
        app.ui.quotas.insert("a2".into(), (Some(3), Some(50)));
        app.ui.ssh_hosts.insert("a3".into(), "far".into());
        let cards = app.account_cards();
        assert_eq!(cards.iter().map(|c| (c.line, c.icon, c.plus)).collect::<Vec<_>>(), [(0, Icon::Claude, false), (1, Icon::Codex, false), (2, Icon::Claude, false), (4, Icon::Plus, true)]);
        assert_eq!(cards.iter().map(|c| c.default).collect::<Vec<_>>(), [true, true, false, false]);
        assert_eq!(cards.iter().map(|c| c.tone).collect::<Vec<_>>(), [Tone::None, Tone::Critical, Tone::None, Tone::None]);
        assert_eq!(cards[2].text, "Other [ssh: far]", "an account on an ssh host says so, as the line does");
        assert_eq!(cards[3].text, "New account");
    }

    #[test]
    fn with_no_account_there_is_only_the_plus_under_the_sentence_and_with_more_than_nine_only_nine_are_cards() {
        let none = list(Vec::new());
        assert_eq!(none.account_cards().iter().map(|c| (c.line, c.plus)).collect::<Vec<_>>(), [(2, true)], "the sentence is line 0, a blank line 1, the key A line 2");
        let many = list((1..=11).map(|i| acct(&format!("a{i}"), "claude", &format!("L{i}"), i == 1)).collect());
        let cards = many.account_cards();
        assert_eq!(cards.len(), 10, "nine accounts and the plus: the digit keys name nine");
        assert_eq!(cards[9].line, 10);
    }

    #[test]
    fn there_are_no_cards_and_no_pictures_on_any_other_screen() {
        let mut app = app(1.0, false);
        assert!(app.account_cards().is_empty() && app.account_buttons().is_empty());
        app.panel_open(Step::Accounts);
        assert!(app.account_buttons().is_empty(), "the list has cards, not the five pictures");
        app.ui.panel.as_mut().unwrap().step = Step::Actions;
        assert!(app.account_cards().is_empty(), "the page has the five pictures, not cards");
    }

    #[test]
    fn an_accounts_page_has_five_pictures_for_the_five_keys_and_the_one_asked_for_once_is_pending() {
        let mut app = three();
        app.ui.panel.as_mut().unwrap().step = Step::Actions;
        let b = app.account_buttons();
        assert_eq!(b.iter().map(|b| (b.icon, b.key)).collect::<Vec<_>>(), [(Icon::Key, 's'), (Icon::Pencil, 'r'), (Icon::Star, 'f'), (Icon::Trash, 'd'), (Icon::TrashX, 'e')]);
        assert!(b.iter().all(|b| !b.pending));
        assert_eq!(b.iter().map(|b| b.word).collect::<Vec<_>>(), ["Sign in", "Rename", "Default", "Remove", "Wipe"]);
        app.ui.panel.as_mut().unwrap().confirm = Some('e');
        assert_eq!(app.account_buttons().iter().map(|b| b.pending).collect::<Vec<_>>(), [false, false, false, false, true]);
    }

    #[test]
    fn a_card_covers_the_line_whose_click_does_what_the_card_says() {
        let n = three().account_cards().len();
        for (i, card) in three().account_cards().iter().enumerate() {
            let mut app = three();
            assert!(app.accounts_or_label_click(card.line), "the line {} of card {i} is clickable", card.line);
            let p = app.ui.panel.as_ref().unwrap();
            if card.plus {
                assert_ne!(p.step, Step::Accounts, "the plus leaves the list for the new account's screen");
            } else {
                assert_eq!((p.step, p.account.as_str()), (Step::Actions, ["a1", "a2", "a3"][i]), "the card {i} opens its own account");
            }
        }
        assert_eq!(n, 4);
        let mut none = list(Vec::new());
        assert!(none.accounts_or_label_click(none.account_cards()[0].line), "with no account the plus is the line of the key A under the sentence");
        assert_ne!(none.ui.panel.as_ref().unwrap().step, Step::Accounts);
    }

    #[test]
    fn a_click_on_a_picture_does_what_its_key_does() {
        let mut app = three();
        app.ui.panel.as_mut().unwrap().step = Step::Actions;
        app.ui.panel.as_mut().unwrap().account = "a2".into();
        assert!(app.account_click(&Action::AccountAct('d')));
        assert!(app.ui.panel.as_ref().unwrap().notice.starts_with("Could not list the sessions"), "the bin ran the key D, which asks the host who is bound first (it has no host here): {}", app.ui.panel.as_ref().unwrap().notice);
        assert!(app.account_click(&Action::AccountAct('f')));
        assert!(app.ui.panel.as_ref().unwrap().notice.starts_with("Not changed"), "the star ran the key F (the window has no host here): {}", app.ui.panel.as_ref().unwrap().notice);
    }

    #[test]
    fn an_action_that_is_not_one_of_the_five_pictures_is_not_taken() {
        let mut app = three();
        assert!(!app.account_click(&Action::WizardPick(0)));
        assert_eq!(app.ui.panel.as_ref().unwrap().step, Step::Accounts);
    }
}
