//! Floating `@AgentName` autocomplete popup above the prompt editor (`M08-P02`).
//!
//! Activates when typing `@<prefix>` at the start of the prompt or after whitespace in any open
//! session tab or Call Room tab. Supports `ArrowUp` / `ArrowDown` navigation, `Tab` or mouse-click
//! insertion (`@Name ` or `@"Name With Spaces" `), and `Esc` dismissal.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::controls::{self, Action, Rect};
use super::sidebar_render::clip_cols;
use super::{emit_ime, App};

const MAX_MENTION_VIS: usize = 6;

#[derive(Clone, Debug)]
pub struct MentionCandidate {
    pub insert_token: String,
    pub display: String,
    pub subtitle: String,
    pub session: Option<String>,
}

#[derive(Clone, Default)]
pub struct MentionItemGeom {
    pub index: usize,
    pub insert_token: String,
    pub display: String,
    pub subtitle: String,
    pub session: Option<String>,
    pub rect: Rect,
}

#[derive(Default)]
pub struct MentionPopupState {
    pub sel: usize,
    pub dismissed_sig: Option<(String, usize, String)>,
    pub popup_rect: Option<Rect>,
    pub items_geom: Vec<MentionItemGeom>,
}

fn format_insert_token(raw_name: &str, session_id: &str) -> String {
    let trimmed = raw_name.trim();
    if trimmed.is_empty() {
        format!("@{}", &session_id[..session_id.len().min(7)])
    } else if trimmed.chars().any(char::is_whitespace) {
        format!("@\"{}\"", trimmed.replace('"', ""))
    } else {
        format!("@{trimmed}")
    }
}

/// Extract `(at_byte, cursor_byte, query)` when the cursor sits inside an unfinished `@<query>`
/// token at the start of the draft or immediately after whitespace.
pub(crate) fn extract_at_query(text: &str, cursor: usize) -> Option<(usize, usize, String)> {
    if cursor == 0 || cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let before = &text[..cursor];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line_before = &before[line_start..];
    let rel_at = line_before.rfind('@')?;
    let at_byte = line_start + rel_at;
    if at_byte > 0 {
        let prev = text[..at_byte].chars().next_back()?;
        if !prev.is_whitespace() {
            return None;
        }
    }
    let after_at = &text[at_byte + 1..cursor];
    if let Some(quoted) = after_at.strip_prefix('"') {
        if quoted.contains('"') {
            return None;
        }
        return Some((at_byte, cursor, quoted.to_string()));
    }
    if after_at.chars().any(char::is_whitespace) {
        return None;
    }
    Some((at_byte, cursor, after_at.to_string()))
}

impl App {
    pub(crate) fn mention_candidates(&self) -> Option<(usize, usize, String, Vec<MentionCandidate>)> {
        if !self.ui.enabled
            || self.ui.panel.is_some()
            || self.ui.search.open
            || !self.preedit.is_empty()
            || !self.has_prompt_target()
        {
            return None;
        }
        let (at_byte, cur_byte, query) = extract_at_query(self.ed.text(), self.ed.cursor())?;
        let target_id = self.prompt_target_id();
        if self.ui.mention.dismissed_sig.as_ref() == Some(&(target_id, at_byte, query.clone())) {
            return None;
        }

        let q_low = query.to_lowercase();
        let mut list = Vec::new();

        if let Some(ref cid) = self.ui.calls.active_call {
            if let Some(sum) = self.ui.sidebar.calls.summaries.iter().find(|s| &s.call_id == cid) {
                for (sid, lbl, prov) in [
                    (&sum.session_a, &sum.label_a, &sum.provider_a),
                    (&sum.session_b, &sum.label_b, &sum.provider_b),
                ] {
                    let state = self
                        .ui
                        .rows
                        .iter()
                        .find(|r| &r.session == sid)
                        .map(|r| if r.holds_a_turn() { "working" } else { r.state.as_str() })
                        .unwrap_or("running");
                    let tok = format_insert_token(lbl, sid);
                    let matches = q_low.is_empty()
                        || lbl.to_lowercase().contains(&q_low)
                        || sid.to_lowercase().contains(&q_low)
                        || prov.to_lowercase().contains(&q_low);
                    if matches {
                        list.push(MentionCandidate {
                            insert_token: tok,
                            display: format!("@{lbl}"),
                            subtitle: format!("{prov} \u{00b7} {state}"),
                            session: Some(sid.clone()),
                        });
                    }
                }
                if q_low.is_empty() || "all".starts_with(&q_low) || "both".starts_with(&q_low) {
                    list.push(MentionCandidate {
                        insert_token: "@all".into(),
                        display: "@all".into(),
                        subtitle: "send to both participants in this Call Room".into(),
                        session: None,
                    });
                }
            }
        } else {
            let mut peers: Vec<_> = self
                .ui
                .rows
                .iter()
                .filter(|r| r.session != self.session && r.state != "exited")
                .collect();
            peers.sort_by_key(|r| (u8::from(r.state != "running"), std::cmp::Reverse(r.last_active_at)));
            let running_count = peers.iter().filter(|r| r.state == "running").count();
            for r in peers {
                let raw_name = r.name.trim();
                let display_name = r.name();
                let tok = format_insert_token(if raw_name.is_empty() { "" } else { raw_name }, &r.session);
                let matches = q_low.is_empty()
                    || display_name.to_lowercase().contains(&q_low)
                    || r.session.to_lowercase().contains(&q_low)
                    || r.provider.to_lowercase().contains(&q_low);
                if matches {
                    let act = if r.holds_a_turn() {
                        "working"
                    } else if r.state == "running" {
                        "idle"
                    } else {
                        r.state.as_str()
                    };
                    list.push(MentionCandidate {
                        insert_token: tok.clone(),
                        display: format!("{tok}  ({display_name})"),
                        subtitle: format!("{} \u{00b7} {act} \u{00b7} {}", r.provider, &r.session[..r.session.len().min(7)]),
                        session: Some(r.session.clone()),
                    });
                }
            }
            if running_count >= 2 && (q_low.is_empty() || "all".starts_with(&q_low)) {
                list.push(MentionCandidate {
                    insert_token: "@all".into(),
                    display: "@all".into(),
                    subtitle: format!("mention all {running_count} running peer sessions"),
                    session: None,
                });
            }
        }

        if list.is_empty() {
            None
        } else {
            Some((at_byte, cur_byte, query, list))
        }
    }

    pub(crate) fn mention_popup_shown(&self) -> bool {
        self.mention_candidates().is_some()
    }

    pub(crate) fn complete_mention_index(&mut self, idx: usize) -> bool {
        let Some((at_byte, cur_byte, _, list)) = self.mention_candidates() else {
            return false;
        };
        let Some(cand) = list.get(idx) else {
            return false;
        };
        let needs_space = !self.ed.text()[cur_byte..].starts_with(' ');
        let replacement = if needs_space {
            format!("{} ", cand.insert_token)
        } else {
            cand.insert_token.clone()
        };
        if self.ed.replace_span(at_byte, cur_byte, &replacement) {
            self.ui.mention.sel = 0;
            self.ui.mention.dismissed_sig = None;
            emit_ime("text", self.ed.text());
            self.dirty = true;
            return true;
        }
        false
    }

    pub(crate) fn mention_key(&mut self, event: &KeyEvent) -> bool {
        let Some((at_byte, _, query, list)) = self.mention_candidates() else {
            return false;
        };
        if self.ui.mods.control_key() || self.ui.mods.alt_key() {
            return false;
        }
        let max_i = list.len().min(MAX_MENTION_VIS).saturating_sub(1);
        let cur_sel = self.ui.mention.sel.min(max_i);
        match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) if !self.ui.mods.shift_key() => {
                self.ui.mention.sel = cur_sel.saturating_sub(1);
                self.dirty = true;
                true
            }
            Key::Named(NamedKey::ArrowDown) if !self.ui.mods.shift_key() => {
                self.ui.mention.sel = (cur_sel + 1).min(max_i);
                self.dirty = true;
                true
            }
            Key::Named(NamedKey::Tab) if !self.ui.mods.shift_key() => {
                if !event.repeat {
                    let _ = self.complete_mention_index(cur_sel);
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                if !event.repeat {
                    self.ui.mention.dismissed_sig = Some((self.prompt_target_id(), at_byte, query));
                    self.dirty = true;
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn draw_mention_popup(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize) {
        self.ui.mention.popup_rect = None;
        self.ui.mention.items_geom.clear();
        let Some((_, _, _, list)) = self.mention_candidates() else {
            return;
        };
        let Some(ed_rect) = self.ed.rect else {
            return;
        };
        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let char_w = self.char_w();
        let vis = list.len().min(MAX_MENTION_VIS);
        let sel = self.ui.mention.sel.min(vis.saturating_sub(1));
        self.ui.mention.sel = sel;

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let x0 = (list_w as f32 + 8.0 * s).min(width as f32 - 40.0 * s);
        let pop_w = (460.0 * s).min((width as f32 - x0 - 12.0 * s).max(160.0 * s));
        let pop_h = (vis as f32 + 1.0) * lh + 8.0 * s;
        let below_y = ed_rect[3] + 4.0 * s;
        let max_y = (height as f32 - 2.0 * lh - 4.0 * s).max(80.0 * s);
        let y0 = if below_y + pop_h <= max_y {
            below_y
        } else {
            (ed_rect[1] - pop_h - 4.0 * s).max(76.0 * s)
        };
        let pop_rect = [x0, y0, x0 + pop_w, y0 + pop_h];

        controls::fill(buf, width, height, [x0 - 1.0, y0 - 1.0, x0 + pop_w + 1.0, y0 + pop_h + 1.0], 0xFF38_B2AC);
        controls::fill(buf, width, height, pop_rect, 0xFF13_1E2A);
        controls::fill(buf, width, height, [x0, y0, x0 + pop_w, y0 + lh + 2.0 * s], 0xFF18_2938);

        let ink = Color::rgb(0xEC, 0xF3, 0xFA);
        let teal = Color::rgb(0x64, 0xE2, 0xCB);
        let muted = Color::rgb(0x98, 0xA8, 0xBA);
        let max_cols = ((pop_w - 16.0 * s) / (char_w * s)).max(16.0) as usize;

        controls.text(
            fs,
            swash,
            buf,
            width,
            height,
            x0 + 8.0 * s,
            y0 + s,
            (x0 + pop_w - 4.0 * s) as i32,
            &clip_cols("Mention agent (@)  \u{00b7}  Tab / click to insert  \u{00b7}  Esc hide", max_cols),
            teal,
        );

        let mut items_out = Vec::new();
        for (idx, cand) in list.iter().take(vis).enumerate() {
            let ry0 = y0 + lh + 4.0 * s + idx as f32 * lh;
            let r = [x0 + 2.0 * s, ry0, x0 + pop_w - 2.0 * s, ry0 + lh];
            if idx == sel {
                controls::fill(buf, width, height, r, 0xFF21_3B52);
            }
            let sub_w = controls.width(fs, &cand.subtitle) + 12.0 * s;
            let disp_clip = (r[2] - sub_w).max(r[0] + 60.0 * s) as i32;
            controls.text(fs, swash, buf, width, height, r[0] + 6.0 * s, ry0, disp_clip, &cand.display, ink);
            if r[2] - sub_w > r[0] + 70.0 * s {
                controls.text(fs, swash, buf, width, height, r[2] - sub_w + 4.0 * s, ry0, r[2] as i32, &cand.subtitle, muted);
            }
            controls.target(r, Action::MentionItem(idx));
            items_out.push(MentionItemGeom {
                index: idx,
                insert_token: cand.insert_token.clone(),
                display: cand.display.clone(),
                subtitle: cand.subtitle.clone(),
                session: cand.session.clone(),
                rect: r,
            });
        }

        self.ui.mention.popup_rect = Some(pop_rect);
        self.ui.mention.items_geom = items_out;
    }

    pub(crate) fn mention_popup_json(&self) -> Value {
        let items: Vec<Value> = self
            .ui
            .mention
            .items_geom
            .iter()
            .map(|it| {
                json!({
                    "index": it.index,
                    "insert_token": it.insert_token,
                    "display": it.display,
                    "subtitle": it.subtitle,
                    "session": it.session,
                    "rect": it.rect,
                })
            })
            .collect();
        json!({
            "shown": self.ui.mention.popup_rect.is_some(),
            "selected": self.ui.mention.sel,
            "rect": self.ui.mention.popup_rect,
            "items": items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_at_query, format_insert_token};

    #[test]
    fn extract_at_query_handles_leading_inline_and_quoted_prefixes() {
        assert_eq!(extract_at_query("@Pri", 4), Some((0, 4, "Pri".into())));
        assert_eq!(extract_at_query("hello @Pri-Avatar", 17), Some((6, 17, "Pri-Avatar".into())));
        assert_eq!(extract_at_query("@\"Pri Ava", 9), Some((0, 9, "Pri Ava".into())));
        assert_eq!(extract_at_query("email@domain.com", 16), None);
        assert_eq!(extract_at_query("@Pri-Avatar hello", 17), None);
    }

    #[test]
    fn format_insert_token_quotes_whitespace_names() {
        assert_eq!(format_insert_token("Pri-Avatar", "sf87442cc739"), "@Pri-Avatar");
        assert_eq!(format_insert_token("Pri Avatar", "sf87442cc739"), "@\"Pri Avatar\"");
        assert_eq!(format_insert_token("", "sf87442cc739"), "@sf87442");
    }
}
