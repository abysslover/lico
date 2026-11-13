//! The look of the "+ New chat" modal (`M10-P03`): a centered dialog over a dimmed window, in the style of the approval dialog, with the account list,
//! the `Resume` and `New` choice, the line about the last session, and the buttons. Nothing but drawing and the places to click.

use cosmic_text::{Color, SwashCache};

use super::approval_modal::{darken_region, draw_centered_btn, ALLOW_LIVE, DIVIDER, MODAL_BG, READ_LIVE, SHADOW_BG};
use super::controls::{self, Action};
use super::newchat::{Geom, MAX_ROWS};
use super::ui::rect;
use super::{clean, App};

const BORDER_BLUE: u32 = 0xFF56_B4FF;
const HEADER_BG: u32 = 0xFF16_2638;
const ROW_ON: u32 = 0xFF21_3147;
const CHOICE_OFF: u32 = 0xFF1B_2230;

impl App {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_new_chat(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, top: f32, chat_top: f32, hint_y: f32) {
        let Some(nc) = self.ui.sidebar.newchat.as_ref() else { return };
        let (folder, host, sel, resume, busy, info) = (nc.folder.clone(), nc.ssh_host.clone(), nc.sel, nc.resume, nc.busy, nc.info_line());
        let accounts: Vec<(String, String, bool)> = nc.accounts.iter().map(|a| (a.label.clone(), a.provider.clone(), a.default)).collect();
        let s = self.ui.scale.max(1.0);
        let lh = (self.line_h() * s).floor().max(18.0);
        let char_w = self.char_w() * s;

        let dim_y0 = top.max(if self.ui.markers { 16.0 } else { 0.0 }).floor().max(0.0) as usize;
        darken_region(buf, width, height, 0, dim_y0, width, hint_y.floor().max(0.0) as usize, 102);

        let modal_w = (560.0 * s).min(width as f32 - (32.0 * s).floor()).max(300.0 * s).floor();
        let pad = (20.0 * s).floor();
        let body_cols = (((modal_w - 2.0 * pad).max(120.0 * s)) / char_w).max(20.0) as usize;
        let info_lines: Vec<String> = controls::wrap_cols(&clean(&info), body_cols).into_iter().take(3).collect();
        let shown_rows = accounts.len().clamp(1, MAX_ROWS);
        let first_row = if sel >= MAX_ROWS { sel + 1 - MAX_ROWS } else { 0 };

        let header_h = (lh + 14.0 * s).floor();
        let btn_h = (lh + 10.0 * s).floor();
        let row_h = (lh + 4.0 * s).floor();
        let (gap, small) = ((12.0 * s).floor(), (8.0 * s).floor());
        let modal_h = header_h + gap + lh + shown_rows as f32 * row_h + gap + btn_h + small + info_lines.len().max(1) as f32 * lh + small + lh + gap + btn_h + (16.0 * s).floor();

        let modal_x0 = (((width as f32) - modal_w) * 0.5).floor().max(8.0);
        let modal_x1 = (modal_x0 + modal_w).min(width as f32 - 4.0).floor();
        let (avail_y0, avail_y1) = (top.floor(), hint_y.floor());
        let modal_y0 = (avail_y0 + ((avail_y1 - avail_y0 - modal_h) * 0.4).floor()).max(avail_y0 + 4.0);
        let modal_y1 = (modal_y0 + modal_h).min(avail_y1 - 2.0).floor();

        let sh = (6.0 * s).floor() as i32;
        let (mw, mh) = ((modal_x1 - modal_x0) as i32, (modal_y1 - modal_y0) as i32);
        rect(buf, width, height, modal_x0 as i32 + sh, modal_y0 as i32 + sh, mw, mh, SHADOW_BG);
        rect(buf, width, height, modal_x0 as i32, modal_y0 as i32, mw, mh, MODAL_BG);
        rect(buf, width, height, modal_x0 as i32, modal_y0 as i32, mw, header_h as i32, HEADER_BG);
        let bw = (2.0 * s).round().max(2.0) as i32;
        rect(buf, width, height, modal_x0 as i32, modal_y0 as i32, mw, bw, BORDER_BLUE);
        rect(buf, width, height, modal_x0 as i32, modal_y1 as i32 - bw, mw, bw, BORDER_BLUE);
        rect(buf, width, height, modal_x0 as i32, modal_y0 as i32, bw, mh, BORDER_BLUE);
        rect(buf, width, height, modal_x1 as i32 - bw, modal_y0 as i32, bw, mh, BORDER_BLUE);
        rect(buf, width, height, modal_x0 as i32 + bw, (modal_y0 + header_h) as i32, mw - 2 * bw, 1, DIVIDER);

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        // everything behind the dialog is absorbed; the dialog's own parts are registered after this, so they win
        controls.target([0.0, chat_top.min(top), width as f32, hint_y], Action::ModalBackdrop);
        if (list_w as f32) < width as f32 {
            controls.target([list_w as f32, chat_top, width as f32, hint_y], Action::ModalBackdrop);
        }
        controls.target([modal_x0, modal_y0, modal_x1, modal_y1], Action::ModalBackdrop);

        let (ink, muted, accent) = (Color::rgb(0xEE, 0xF2, 0xF8), Color::rgb(0x98, 0xAC, 0xC2), Color::rgb(0x88, 0xC8, 0xFF));
        let clip_x = (modal_x1 - pad) as i32;
        let place = match &host {
            Some(h) => format!("ssh:{h} {folder}"),
            None => folder.clone(),
        };
        let title = format!("New chat  ·  {}", super::sidebar_render::tail_path(&clean(&place), (body_cols.saturating_sub(12)).max(10)));
        controls.text(fs, swash, buf, width, height, modal_x0 + pad, (modal_y0 + ((header_h - lh) * 0.5).floor()).max(modal_y0), clip_x, &title, accent);

        let mut geom = Geom::default();
        let mut y = (modal_y0 + header_h + gap).floor();
        controls.text(fs, swash, buf, width, height, modal_x0 + pad, y, clip_x, "Account", muted);
        y += lh;
        if accounts.is_empty() {
            controls.text(fs, swash, buf, width, height, modal_x0 + pad, y + 2.0 * s, clip_x, "(none for this location)", muted);
            y += row_h;
        }
        for (i, (label, provider, default)) in accounts.iter().enumerate().skip(first_row).take(MAX_ROWS) {
            let r = [modal_x0 + pad, y, modal_x1 - pad, y + row_h];
            if i == sel {
                rect(buf, width, height, r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32, ROW_ON);
            }
            let mark = if i == sel { "\u{25cf}" } else { "\u{25cb}" };
            let text = format!("{mark} {}  ·  {provider}{}", clean(label), if *default { "  (default)" } else { "" });
            controls.text(fs, swash, buf, width, height, r[0] + 6.0 * s, y + 2.0 * s, r[2] as i32, &text, if i == sel { ink } else { muted });
            controls.target(r, Action::NewChatAccount(i));
            geom.rows.push((i, r));
            y += row_h;
        }
        y += gap;

        let half = ((modal_x1 - modal_x0 - 2.0 * pad - small) * 0.5).floor();
        let resume_r = [modal_x0 + pad, y, modal_x0 + pad + half, y + btn_h];
        let new_r = [resume_r[2] + small, y, modal_x1 - pad, y + btn_h];
        let on = Color::rgb(0xF4, 0xF7, 0xFB);
        draw_centered_btn(controls, fs, swash, buf, width, height, resume_r, "Resume", if resume { ALLOW_LIVE } else { CHOICE_OFF }, if resume { on } else { muted }, lh, s, Action::NewChatMode(true));
        draw_centered_btn(controls, fs, swash, buf, width, height, new_r, "New", if resume { CHOICE_OFF } else { ALLOW_LIVE }, if resume { muted } else { on }, lh, s, Action::NewChatMode(false));
        (geom.resume, geom.new) = (Some(resume_r), Some(new_r));
        y += btn_h + small;

        for line in &info_lines {
            controls.text(fs, swash, buf, width, height, modal_x0 + pad, y, clip_x, line, ink);
            y += lh;
        }
        if info_lines.is_empty() {
            y += lh;
        }
        y += small;
        controls.text(fs, swash, buf, width, height, modal_x0 + pad, y, clip_x, "Up/Down account  Left/Right Resume or New  Enter  Esc", muted);

        let btn_y = (modal_y1 - (16.0 * s).floor() - btn_h).floor();
        rect(buf, width, height, (modal_x0 + pad) as i32, (btn_y - (7.0 * s).floor()) as i32, (modal_w - 2.0 * pad) as i32, 1, DIVIDER);
        let (bw_btn, gap_btn) = ((112.0 * s).floor(), (12.0 * s).floor());
        let cancel_r = [(modal_x1 - pad - bw_btn).floor(), btn_y, (modal_x1 - pad).floor(), btn_y + btn_h];
        let ok_r = [cancel_r[0] - gap_btn - bw_btn, btn_y, cancel_r[0] - gap_btn, btn_y + btn_h];
        let options_r = [modal_x0 + pad, btn_y, (modal_x0 + pad + (132.0 * s).floor()).min(ok_r[0] - gap_btn), btn_y + btn_h];
        let ok_fill = if busy || accounts.is_empty() { 0xFF24_3629 } else { ALLOW_LIVE };
        draw_centered_btn(controls, fs, swash, buf, width, height, ok_r, if busy { "Opening..." } else if resume { "Resume" } else { "Start" }, ok_fill, on, lh, s, Action::NewChatOk);
        draw_centered_btn(controls, fs, swash, buf, width, height, cancel_r, "Cancel", READ_LIVE, on, lh, s, Action::NewChatCancel);
        if options_r[2] - options_r[0] >= 80.0 * s {
            draw_centered_btn(controls, fs, swash, buf, width, height, options_r, "Options...", READ_LIVE, on, lh, s, Action::NewChatOptions);
            geom.options = Some(options_r);
        }
        (geom.ok, geom.cancel) = (Some(ok_r), Some(cancel_r));
        if let Some(nc) = self.ui.sidebar.newchat.as_mut() {
            nc.geom = geom;
        }
    }
}
