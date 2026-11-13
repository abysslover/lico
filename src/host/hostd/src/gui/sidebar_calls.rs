//! Left sidebar `[ Calls ]` tab (`tab == 2`, `M08-P02`): live summary of persistent Call Rooms
//! (`⇄ <Label A> · <Label B>`), quick-connect peer buttons, instant filtering, pagination,
//! 1-click Call Room tab activation, and per-room `×` close (`call.close`).

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};

use super::controls::{self, Action, Rect};
use super::panel::AsyncJob;
use super::sidebar::SidebarGeom;
use super::sidebar_render::clip_cols;
use super::ui::memrow;
use super::App;

#[derive(Clone, Debug, PartialEq)]
pub struct CallSummaryItem {
    pub call_id: String,
    pub title: String,
    pub session_a: String,
    pub session_b: String,
    pub provider_a: String,
    pub provider_b: String,
    pub label_a: String,
    pub label_b: String,
    pub status: String,
    pub active: bool,
    pub paused: bool,
    pub owner_authorized: bool,
    pub round: u32,
    pub max_rounds: u32,
    pub entry_count: usize,
    pub updated_at: u64,
    pub last_speaker: String,
    pub last_preview: String,
}

#[derive(Clone, Default)]
pub struct CallSidebarEntryGeom {
    pub index: usize,
    pub call_id: String,
    pub title: String,
    pub status: String,
    pub round: u32,
    pub entry_count: usize,
    pub rect: Rect,
    pub close_rect: Rect,
}

#[derive(Clone, Default)]
pub struct CallPeerStartGeom {
    pub session: String,
    pub label: String,
    pub provider: String,
    pub rect: Rect,
}

#[derive(Default)]
pub struct CallsSidebarState {
    pub summaries: Vec<CallSummaryItem>,
    pub page: usize,
    pub page_size: usize,
    pub entries_geom: Vec<CallSidebarEntryGeom>,
    pub peers_geom: Vec<CallPeerStartGeom>,
}

fn parse_call_summaries(value: &Value) -> Vec<CallSummaryItem> {
    value["calls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|it| {
            let call_id = it["call_id"].as_str()?.to_string();
            let session_a = it["session_a"].as_str()?.to_string();
            let session_b = it["session_b"].as_str()?.to_string();
            let label_a = it["label_a"].as_str().unwrap_or(&session_a).to_string();
            let label_b = it["label_b"].as_str().unwrap_or(&session_b).to_string();
            let title = it["title"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{label_a} \u{00b7} {label_b}"));
            Some(CallSummaryItem {
                call_id,
                title,
                session_a,
                session_b,
                provider_a: it["provider_a"].as_str().unwrap_or("claude").to_string(),
                provider_b: it["provider_b"].as_str().unwrap_or("codex").to_string(),
                label_a,
                label_b,
                status: it["status"].as_str().unwrap_or("idle").to_string(),
                active: it["active"] == json!(true),
                paused: it["paused"] == json!(true),
                owner_authorized: it["owner_authorized"] == json!(true),
                round: it["round"].as_u64().unwrap_or(0) as u32,
                max_rounds: it["max_rounds"].as_u64().unwrap_or(4) as u32,
                entry_count: it["entry_count"].as_u64().unwrap_or(0) as usize,
                updated_at: it["updated_at"].as_u64().unwrap_or(0),
                last_speaker: it["last_speaker"].as_str().unwrap_or("").to_string(),
                last_preview: it["last_preview"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

impl CallsSidebarState {
    pub fn filtered_indices(&self, query: &str) -> Vec<usize> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return (0..self.summaries.len()).collect();
        }
        self.summaries
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.title.to_lowercase().contains(&q)
                    || c.call_id.to_lowercase().contains(&q)
                    || c.label_a.to_lowercase().contains(&q)
                    || c.label_b.to_lowercase().contains(&q)
                    || c.last_preview.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn active_count(&self) -> usize {
        self.summaries.iter().filter(|c| c.active).count()
    }
}

impl App {
    pub(crate) fn sync_calls_from_status(&mut self, value: &Value) {
        let next = parse_call_summaries(value);
        let changed = next != self.ui.sidebar.calls.summaries;
        self.ui.sidebar.calls.summaries = next;
        if changed {
            self.dirty = true;
        }
        self.sync_open_calls_after_status();
    }

    pub(crate) fn calls_step_page(&mut self, delta: isize) {
        if delta == 0 {
            return;
        }
        let filtered_len = self.ui.sidebar.calls.filtered_indices(&self.ui.sidebar.filter).len();
        let ps = self.ui.sidebar.calls.page_size.max(1);
        let total_pages = filtered_len.div_ceil(ps).max(1);
        let cur = self.ui.sidebar.calls.page.min(total_pages.saturating_sub(1));
        let next = (cur as isize + delta).clamp(0, total_pages.saturating_sub(1) as isize) as usize;
        if next != cur {
            self.ui.sidebar.calls.page = next;
            self.dirty = true;
        }
    }

    pub(crate) fn sidebar_calls_click(&mut self, action: &Action) -> bool {
        match action {
            Action::SidebarCallEntry(idx) => {
                self.ui.sidebar.host_popup_open = false;
                self.ui.sidebar.nav.popup_open = false;
                self.ui.sidebar.filter_focused = false;
                if let Some(cid) = self.ui.sidebar.calls.summaries.get(*idx).map(|c| c.call_id.clone()) {
                    self.open_call_tab(&cid);
                }
                true
            }
            Action::SidebarCallClose(idx) => {
                if let Some(cid) = self.ui.sidebar.calls.summaries.get(*idx).map(|c| c.call_id.clone()) {
                    self.close_and_remove_call(&cid);
                }
                true
            }
            Action::SidebarCallStartPeer(peer_sid) => {
                let caller = if !self.session.is_empty() {
                    Some(self.session.clone())
                } else {
                    self.ui
                        .rows
                        .iter()
                        .find(|r| r.state == "running" && r.session != *peer_sid)
                        .map(|r| r.session.clone())
                };
                if let Some(from_sid) = caller {
                    self.start_call_between(&from_sid, peer_sid);
                }
                true
            }
            _ => false,
        }
    }

    fn close_and_remove_call(&mut self, call_id: &str) {
        self.close_call_tab(call_id);
        if self.socket.is_empty() {
            return;
        }
        let cid = call_id.to_string();
        let _ = self.spawn_host_call(json!({"op": "call.close", "call": cid}), 6, |_| {
            AsyncJob::CallAction
        });
        self.ui.flash = Some("Closed Call Room.".into());
        self.dirty = true;
    }

    pub(super) fn insert_calls_json(&self, out: &mut Value) {
        let cs = &self.ui.sidebar.calls;
        let entries: Vec<Value> = cs
            .entries_geom
            .iter()
            .map(|e| {
                json!({
                    "index": e.index,
                    "call_id": e.call_id,
                    "title": e.title,
                    "status": e.status,
                    "round": e.round,
                    "entry_count": e.entry_count,
                    "rect": e.rect,
                    "close_rect": e.close_rect,
                })
            })
            .collect();
        let peers: Vec<Value> = cs
            .peers_geom
            .iter()
            .map(|p| {
                json!({
                    "session": p.session,
                    "label": p.label,
                    "provider": p.provider,
                    "rect": p.rect,
                })
            })
            .collect();
        out["calls_tab_rect"] = json!(self.ui.sidebar.geom.calls_tab_rect);
        out["calls_total"] = json!(cs.summaries.len());
        out["calls_active_count"] = json!(cs.active_count());
        out["call_entries"] = json!(entries);
        out["call_peer_starters"] = json!(peers);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_calls_sidebar(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        x_right: f32,
        tab_y1: f32,
        hint_y: f32,
        total_mem: &str,
        mut geom: SidebarGeom,
    ) {
        self.ui.sidebar.calls.entries_geom.clear();
        self.ui.sidebar.calls.peers_geom.clear();

        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let char_w = self.char_w();
        let max_cols = ((x_right - 12.0 * s) / (char_w * s)).max(12.0) as usize;
        let mem_y = hint_y - lh;
        let mut y = tab_y1 + 4.0 * s;

        let filter = self.ui.sidebar.filter.clone();
        let filter_focused = self.ui.sidebar.filter_focused;
        let filtered = self.ui.sidebar.calls.filtered_indices(&filter);
        let active_call = self.ui.calls.active_call.clone();

        let caller_sid = if !self.session.is_empty() {
            Some(self.session.clone())
        } else {
            self.ui.rows.iter().find(|r| r.state == "running").map(|r| r.session.clone())
        };
        let caller_name = caller_sid
            .as_ref()
            .and_then(|sid| self.ui.rows.iter().find(|r| &r.session == sid))
            .map(|r| r.name());
        let peer_choices: Vec<(String, String, String)> = caller_sid
            .as_ref()
            .map(|csid| {
                self.ui
                    .rows
                    .iter()
                    .filter(|r| r.state == "running" && &r.session != csid)
                    .take(3)
                    .map(|r| (r.session.clone(), r.name(), r.provider.clone()))
                    .collect()
            })
            .unwrap_or_default();

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let ink = Color::rgb(0xEA, 0xF0, 0xF7);
        let muted = Color::rgb(0x98, 0xA6, 0xB8);
        let dim = Color::rgb(0x5E, 0x6C, 0x7E);
        let teal_ink = Color::rgb(0x64, 0xE2, 0xCB);
        let gold_ink = Color::rgb(0xFA, 0xD2, 0x6E);
        let row_h = lh + 2.0 * s;

        // 1. Quick-connect peer strip (`+ ⇄ <Peer>`) when >= 2 running sessions exist
        if let Some(ref cname) = caller_name.filter(|_| !peer_choices.is_empty()) {
            let hdr = clip_cols(&format!("Start call from {cname}:"), max_cols);
            controls.text(fs, swash, buf, width, height, 6.0 * s, y, x_right as i32, &hdr, muted);
            y += lh + 2.0 * s;
            let mut out_peers = Vec::new();
            for (psid, plabel, pprov) in &peer_choices {
                let btn_lbl = clip_cols(&format!("+ \u{21c4} {plabel} ({pprov})"), max_cols.saturating_sub(2));
                let brect = [4.0 * s, y, x_right - 4.0 * s, y + row_h];
                controls.button(
                    fs,
                    swash,
                    buf,
                    width,
                    height,
                    brect,
                    &btn_lbl,
                    0xFF18_3136,
                    teal_ink,
                    Action::SidebarCallStartPeer(psid.clone()),
                );
                out_peers.push(CallPeerStartGeom {
                    session: psid.clone(),
                    label: plabel.clone(),
                    provider: pprov.clone(),
                    rect: brect,
                });
                y += row_h + 2.0 * s;
            }
            self.ui.sidebar.calls.peers_geom = out_peers;
            y += 2.0 * s;
        }

        // 2. Call Room search/filter box
        let f_rect = [4.0 * s, y, x_right - 4.0 * s, y + row_h];
        controls::fill(buf, width, height, f_rect, if filter_focused { 0xFF1E_2D42 } else { 0xFF16_1E29 });
        if filter_focused {
            controls::fill(buf, width, height, [f_rect[0], f_rect[3] - 1.0, f_rect[2], f_rect[3]], 0xFF56_B4FF);
        }
        let has_clear = filter_focused || !filter.is_empty();
        let clr_w = if has_clear { controls.width(fs, "\u{00d7}") + 10.0 * s } else { 0.0 };
        let f_text = if filter.is_empty() && !filter_focused {
            "Filter calls...".to_string()
        } else if filter_focused {
            format!("Filter: {filter}|")
        } else {
            format!("Filter: {filter}")
        };
        let f_col = if filter.is_empty() && !filter_focused { dim } else { ink };
        controls.text(fs, swash, buf, width, height, f_rect[0] + 6.0 * s, y + s, (f_rect[2] - clr_w - 2.0 * s) as i32, &f_text, f_col);
        let main_f_rect = [f_rect[0], f_rect[1], f_rect[2] - clr_w, f_rect[3]];
        controls.target(main_f_rect, Action::SidebarFilter);
        geom.filter_rect = Some(main_f_rect);
        if has_clear {
            let c_rect = [f_rect[2] - clr_w, f_rect[1], f_rect[2], f_rect[3]];
            controls.text(fs, swash, buf, width, height, c_rect[0] + 4.0 * s, y + s, c_rect[2] as i32, "\u{00d7}", muted);
            controls.target(c_rect, Action::SidebarFilterClear);
            geom.filter_clear_rect = Some(c_rect);
        }
        y += row_h + 5.0 * s;

        // 3. Call Room cards
        let nav_y = mem_y - lh - 4.0 * s;
        let card_h = 2.0 * lh + 5.0 * s;
        let ps = (((nav_y - y).max(0.0) / card_h).floor() as usize).clamp(1, 20);
        self.ui.sidebar.calls.page_size = ps;
        let total = filtered.len();
        let total_pages = total.div_ceil(ps).max(1);
        let page_idx = self.ui.sidebar.calls.page.min(total_pages - 1);
        self.ui.sidebar.calls.page = page_idx;
        let start = page_idx * ps;
        let end = (start + ps).min(total);

        if filtered.is_empty() {
            let msg = if filter.trim().is_empty() {
                "No Call Rooms yet."
            } else {
                "(no matching Call Rooms)"
            };
            controls.text(fs, swash, buf, width, height, 6.0 * s, y, x_right as i32, msg, dim);
            if filter.trim().is_empty() {
                controls.text(
                    fs,
                    swash,
                    buf,
                    width,
                    height,
                    6.0 * s,
                    y + lh,
                    x_right as i32,
                    &clip_cols("Type @AgentName in chat to connect.", max_cols),
                    muted,
                );
            }
        } else {
            let close_w = controls.width(fs, "\u{00d7}") + 8.0 * s;
            let summaries = self.ui.sidebar.calls.summaries.clone();
            let mut out_geom = Vec::new();
            for &sum_idx in &filtered[start..end] {
                if y + card_h > nav_y + 2.0 * s {
                    break;
                }
                let item = &summaries[sum_idx];
                let r = [3.0 * s, y, x_right - 3.0 * s, y + card_h - 2.0 * s];
                let is_sel = active_call.as_deref() == Some(item.call_id.as_str());
                let fill = if is_sel {
                    0xFF19_3338
                } else if sum_idx % 2 == 0 {
                    0xFF15_1E29
                } else {
                    0xFF13_1B25
                };
                controls::fill(buf, width, height, r, fill);
                if is_sel || item.active {
                    let bar_col = if item.active { 0xFF38_B2AC } else { 0xFF56_B4FF };
                    controls::fill(buf, width, height, [r[0], r[1], r[0] + 3.0 * s, r[3]], bar_col);
                }

                let (pill, pill_col) = if item.paused {
                    ("[paused]".to_string(), gold_ink)
                } else if item.active {
                    (format!("[{}/{}]", item.round.max(1), item.max_rounds), teal_ink)
                } else {
                    ("[idle]".to_string(), muted)
                };
                let pill_w = controls.width(fs, &pill) + 6.0 * s;
                let crect = [r[2] - close_w - 2.0 * s, y + s, r[2] - 2.0 * s, y + lh + s];
                let title_clip = (crect[0] - pill_w - 2.0 * s) as i32;
                let title_cols = (((title_clip as f32 - (r[0] + 7.0 * s)).max(16.0)) / (char_w * s)) as usize;
                let title_lbl = format!("\u{21c4} {}", clip_cols(&item.title, title_cols.saturating_sub(2)));
                controls.text(fs, swash, buf, width, height, r[0] + 7.0 * s, y + s, title_clip, &title_lbl, ink);
                controls.text(fs, swash, buf, width, height, crect[0] - pill_w, y + s, crect[0] as i32, &pill, pill_col);

                let sub_raw = if item.last_preview.is_empty() {
                    format!("{} \u{2194} {} \u{00b7} {} msgs", item.provider_a, item.provider_b, item.entry_count)
                } else {
                    format!("{} msgs \u{00b7} {}: {}", item.entry_count, item.last_speaker, item.last_preview)
                };
                controls.text(
                    fs,
                    swash,
                    buf,
                    width,
                    height,
                    r[0] + 8.0 * s,
                    y + lh + 1.5 * s,
                    (r[2] - 4.0 * s) as i32,
                    &clip_cols(&sub_raw, max_cols.saturating_sub(2)),
                    muted,
                );

                // Register card body target first, then `×` close button on top (`hit_at` picks last drawn).
                controls.target(r, Action::SidebarCallEntry(sum_idx));
                controls.text(fs, swash, buf, width, height, crect[0] + 3.0 * s, y + s, crect[2] as i32, "\u{00d7}", muted);
                controls.target(crect, Action::SidebarCallClose(sum_idx));

                out_geom.push(CallSidebarEntryGeom {
                    index: sum_idx,
                    call_id: item.call_id.clone(),
                    title: item.title.clone(),
                    status: item.status.clone(),
                    round: item.round,
                    entry_count: item.entry_count,
                    rect: r,
                    close_rect: crect,
                });
                y += card_h;
            }
            self.ui.sidebar.calls.entries_geom = out_geom;
        }

        // 4. Pagination Footer + Memory Total Line
        if nav_y > tab_y1 + lh * 3.0 {
            let has_prev = page_idx > 0;
            let has_next = page_idx + 1 < total_pages;
            let btn_w = controls.width(fs, "<") + 12.0 * s;
            let pv_rect = [4.0 * s, nav_y, 4.0 * s + btn_w, nav_y + lh];
            let nx_rect = [x_right - 4.0 * s - btn_w, nav_y, x_right - 4.0 * s, nav_y + lh];
            controls.button(
                fs,
                swash,
                buf,
                width,
                height,
                pv_rect,
                "<",
                if has_prev { 0xFF20_2E40 } else { 0xFF16_1E28 },
                if has_prev { ink } else { dim },
                Action::SidebarPage(-1),
            );
            controls.button(
                fs,
                swash,
                buf,
                width,
                height,
                nx_rect,
                ">",
                if has_next { 0xFF20_2E40 } else { 0xFF16_1E28 },
                if has_next { ink } else { dim },
                Action::SidebarPage(1),
            );
            let pg_lbl = format!("Page {}/{} ({total})", page_idx + 1, total_pages);
            let pg_w = controls.width(fs, &pg_lbl);
            let pg_x = ((x_right - pg_w) * 0.5).max(pv_rect[2] + 4.0 * s);
            controls.text(fs, swash, buf, width, height, pg_x, nav_y, (nx_rect[0] - 2.0 * s) as i32, &pg_lbl, muted);
            geom.prev_rect = Some(pv_rect);
            geom.next_rect = Some(nx_rect);
        }

        memrow::draw_total(controls, fs, swash, buf, width, height, [4.0 * s, mem_y, x_right - 2.0, nav_y], total_mem, muted);
        self.ui.sidebar.geom = geom;
    }
}
