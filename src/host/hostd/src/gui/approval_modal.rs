//! Centered window-level approval modal notification dialog (`M07-P05`).
//!
//! Renders an unmistakable centered modal dialog over a dimmed window backdrop whenever a pending
//! approval request exists on the active session (or any open workbench tab in `lico gui`), with
//! centered `Allow` and `Deny` action buttons, optional `Read Full (V)` inspection, and clear
//! session-persistence messaging.

use cosmic_text::{Color, FontSystem, SwashCache};

use super::controls::{self, Action, Controls, Rect};
use super::ui::rect;
use super::{clean, App};

pub(super) const SHADOW_BG: u32 = 0xFF05_070B;
pub(super) const MODAL_BG: u32 = 0xFF16_1C26;
const HEADER_BG: u32 = 0xFF23_1E16;
const BORDER_AMBER: u32 = 0xFFE5_A93C;
pub(super) const DIVIDER: u32 = 0xFF2C_3748;

pub(super) const ALLOW_LIVE: u32 = 0xFF2E_8548;
const ALLOW_DIM: u32 = 0xFF24_3629;
const DENY_LIVE: u32 = 0xFF9E_3636;
const DENY_DIM: u32 = 0xFF38_2424;
pub(super) const READ_LIVE: u32 = 0xFF24_3042;
pub(super) const READ_DIM: u32 = 0xFF1B_2230;

pub(super) fn darken_region(buf: &mut [u32], width: usize, height: usize, x0: usize, y0: usize, x1: usize, y1: usize, numer: u32) {
    let x1 = x1.min(width);
    let y1 = y1.min(height);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for py in y0..y1 {
        let row = &mut buf[py * width + x0..py * width + x1];
        for px in row.iter_mut() {
            let r = ((*px >> 16) & 0xFF) * numer / 256;
            let g = ((*px >> 8) & 0xFF) * numer / 256;
            let b = (*px & 0xFF) * numer / 256;
            *px = 0xFF00_0000 | (r << 16) | (g << 8) | b;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_centered_btn(
    controls: &mut Controls,
    fs: &mut FontSystem,
    swash: &mut SwashCache,
    buf: &mut [u32],
    width: usize,
    height: usize,
    r: Rect,
    label: &str,
    fill: u32,
    ink: Color,
    lh: f32,
    s: f32,
    action: Action,
) {
    rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, fill);
    let tw = controls.width(fs, label);
    let tx = (r[0] + ((r[2] - r[0] - tw) * 0.5).floor()).max(r[0] + (10.0 * s).floor());
    let ty = (r[1] + ((r[3] - r[1] - lh) * 0.5).floor()).max(r[1]);
    controls.text(fs, swash, buf, width, height, tx, ty, r[2] as i32, label, ink);
    controls.target(r, action);
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_approval_modal(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        list_w: usize,
        top: f32,
        chat_top: f32,
        hint_y: f32,
    ) {
        let Some(row) = self.modal_approval_row().cloned() else { return };
        let Some((id, prompt)) = row.approvals.first().cloned() else { return };
        let ours = self.lico_card() == Some(id);
        let live = self.answers_live();
        let no_keep = ours && self.card_asks(id).no_keep();
        let s = self.ui.scale.max(1.0);
        let lh = (self.line_h() * s).floor().max(18.0);
        let char_w = self.char_w() * s;

        let dim_y0 = top.max(if self.ui.markers { 16.0 } else { 0.0 }).floor().max(0.0) as usize;
        let dim_y1 = hint_y.floor().max(0.0) as usize;
        darken_region(buf, width, height, 0, dim_y0, width, dim_y1, 102);

        let modal_w = (580.0 * s).min(width as f32 - (32.0 * s).floor()).max(300.0 * s).floor();
        let pad = (20.0 * s).floor();
        let body_cols = (((modal_w - 2.0 * pad).max(120.0 * s)) / char_w).max(20.0) as usize;
        let prompt_lines: Vec<String> = controls::wrap_cols(&clean(&format!("approval {id}: {prompt}")), body_cols)
            .into_iter()
            .take(8)
            .collect();

        let header_h = (lh + (14.0 * s).floor()).floor();
        let btn_h = (lh + (10.0 * s).floor()).floor();
        let top_gap = (12.0 * s).floor();
        let note_gap = (8.0 * s).floor();
        let footer_gap = (14.0 * s).floor();
        let bot_pad = (16.0 * s).floor();
        let body_h = (prompt_lines.len() as f32) * lh;
        let modal_h = header_h + top_gap + body_h + note_gap + lh + footer_gap + btn_h + bot_pad;

        let modal_x0 = (((width as f32) - modal_w) * 0.5).floor().max(8.0);
        let modal_x1 = (modal_x0 + modal_w).min(width as f32 - 4.0).floor();
        let avail_y0 = top.floor();
        let avail_y1 = hint_y.floor();
        let modal_y0 = (avail_y0 + ((avail_y1 - avail_y0 - modal_h) * 0.45).floor()).max(avail_y0 + 4.0);
        let modal_y1 = (modal_y0 + modal_h).min(avail_y1 - 2.0).floor();

        let sh = (6.0 * s).floor() as i32;
        rect(buf, width, height, modal_x0 as i32 + sh, modal_y0 as i32 + sh, (modal_x1 - modal_x0) as i32, (modal_y1 - modal_y0) as i32, SHADOW_BG);
        rect(buf, width, height, modal_x0 as i32, modal_y0 as i32, (modal_x1 - modal_x0) as i32, (modal_y1 - modal_y0) as i32, MODAL_BG);
        rect(buf, width, height, modal_x0 as i32, modal_y0 as i32, (modal_x1 - modal_x0) as i32, header_h as i32, HEADER_BG);

        let bw = (2.0 * s).round().max(2.0) as i32;
        let mw = (modal_x1 - modal_x0) as i32;
        let mh = (modal_y1 - modal_y0) as i32;
        rect(buf, width, height, modal_x0 as i32, modal_y0 as i32, mw, bw, BORDER_AMBER);
        rect(buf, width, height, modal_x0 as i32, modal_y1 as i32 - bw, mw, bw, BORDER_AMBER);
        rect(buf, width, height, modal_x0 as i32, modal_y0 as i32, bw, mh, BORDER_AMBER);
        rect(buf, width, height, modal_x1 as i32 - bw, modal_y0 as i32, bw, mh, BORDER_AMBER);
        rect(buf, width, height, modal_x0 as i32 + bw, (modal_y0 + header_h) as i32, mw - 2 * bw, 1, DIVIDER);

        let sess_title = {
            let t = row.display_title();
            if t.is_empty() {
                row.session.chars().take(7).collect::<String>()
            } else {
                t.chars().take(28).collect::<String>()
            }
        };
        let bg_note = if row.session != self.session { " [background tab]" } else { "" };
        let title_str = format!("Approval Required  ·  {} ({}){}", sess_title, row.provider, bg_note);
        let policy_str = if ours && !no_keep {
            "Allow keeps this permission active for the rest of this session.  (Y / N)"
        } else if ours {
            "Applies to this single request only.  (Y / N)"
        } else {
            "Provider permission prompt — choose Allow (Y) or Deny (N)."
        };

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        controls.dimmed = !live;

        // Absorb chat-pane backdrop clicks & modal card clicks while keeping session list / top tabs interactive.
        if (list_w as f32) < width as f32 && chat_top < hint_y {
            controls.target([list_w as f32, chat_top, width as f32, hint_y], Action::ModalBackdrop);
        }
        controls.target([modal_x0, modal_y0, modal_x1, modal_y1], Action::ModalBackdrop);

        let clip_x = (modal_x1 - pad) as i32;
        let title_y = (modal_y0 + ((header_h - lh) * 0.5).floor()).max(modal_y0);
        controls.text(fs, swash, buf, width, height, modal_x0 + pad, title_y, clip_x, &title_str, Color::rgb(0xFF, 0xCC, 0x66));

        let mut cy = (modal_y0 + header_h + top_gap).floor();
        let body_ink = Color::rgb(0xEE, 0xF2, 0xF8);
        for line in &prompt_lines {
            controls.text(fs, swash, buf, width, height, modal_x0 + pad, cy, clip_x, line, body_ink);
            cy += lh;
        }
        cy = (cy + note_gap).floor();
        controls.text(fs, swash, buf, width, height, modal_x0 + pad, cy, clip_x, policy_str, Color::rgb(0x98, 0xAC, 0xC2));

        let btn_y = (modal_y1 - bot_pad - btn_h).floor();
        rect(buf, width, height, (modal_x0 + pad) as i32, (btn_y - (7.0 * s).floor()) as i32, (modal_w - 2.0 * pad) as i32, 1, DIVIDER);

        let btn_w = (112.0 * s).floor();
        let btn_gap = (12.0 * s).floor();
        let deny_x1 = (modal_x1 - pad).floor();
        let deny_x0 = (deny_x1 - btn_w).floor();
        let allow_x1 = (deny_x0 - btn_gap).floor();
        let allow_x0 = (allow_x1 - btn_w).floor();

        let (allow_fill, deny_fill, read_fill, btn_ink) = if live {
            (ALLOW_LIVE, DENY_LIVE, READ_LIVE, Color::rgb(0xF4, 0xF7, 0xFB))
        } else {
            (ALLOW_DIM, DENY_DIM, READ_DIM, Color::rgb(0x9E, 0xA8, 0xB4))
        };

        if ours {
            let read_w = (132.0 * s).floor();
            let read_r = [modal_x0 + pad, btn_y, (modal_x0 + pad + read_w).min(allow_x0 - btn_gap), btn_y + btn_h];
            if read_r[2] - read_r[0] >= 80.0 * s {
                draw_centered_btn(controls, fs, swash, buf, width, height, read_r, "Read Full (V)", read_fill, btn_ink, lh, s, Action::ReadCard);
            }
        }
        draw_centered_btn(controls, fs, swash, buf, width, height, [allow_x0, btn_y, allow_x1, btn_y + btn_h], "Allow", allow_fill, btn_ink, lh, s, Action::Allow);
        draw_centered_btn(controls, fs, swash, buf, width, height, [deny_x0, btn_y, deny_x1, btn_y + btn_h], "Deny", deny_fill, btn_ink, lh, s, Action::Deny);
    }
}
