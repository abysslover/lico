//! Rendering, layout helpers, and UI-dump serialization for the `Files` panel (`M06-P01`..`M06-P03`).

use serde_json::{json, Value};

use super::super::editor::{hidden, wide};
use super::super::App;
use super::files_ui::{Explorer, FileItem, HOST_POPUP_MAX_VIS};
use super::Step;

pub(super) const DEFAULT_PAGE_SIZE: usize = 18;
const WIDE: usize = 96;

pub(super) fn calc_page_size(panel_rows: usize, in_preview: bool) -> usize {
    if panel_rows == 0 {
        return DEFAULT_PAGE_SIZE;
    }
    let reserve = if in_preview { 9 } else { 7 };
    panel_rows.saturating_sub(reserve).clamp(5, 36)
}

pub(super) fn clip_cols(text: &str, max_cols: usize) -> String {
    if max_cols < 20 {
        return text.to_string();
    }
    let total: usize = text.chars().map(|c| if wide(c) { 2 } else { 1 }).sum();
    if total <= max_cols {
        return text.to_string();
    }
    let budget = max_cols.saturating_sub(1);
    let (mut out, mut used) = (String::new(), 0);
    for c in text.chars() {
        let w = if wide(c) { 2 } else { 1 };
        if used + w > budget {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

pub(super) fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub(super) fn wrap_preview(text: &str, max_cols: usize) -> Vec<String> {
    let limit = if max_cols >= 32 { max_cols.min(WIDE) } else { WIDE };
    let mut parts: Vec<&str> = text.split('\n').collect();
    if parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    let mut out = Vec::new();
    for part in parts {
        let (mut line, mut cols) = (String::new(), 0);
        for c in part.chars().flat_map(|c| if c == '\t' { vec![' '; 4] } else { vec![c] }) {
            if c.is_control() || hidden(c) {
                continue;
            }
            let w = if wide(c) { 2 } else { 1 };
            if cols + w > limit {
                out.push(std::mem::take(&mut line));
                cols = 0;
            }
            line.push(c);
            cols += w;
        }
        out.push(line);
    }
    out
}

pub(super) fn refusal_text(code: &str) -> String {
    match code {
        "ssh_unreachable" | "host_timeout" | "host_unreachable" => {
            "Could not connect to that SSH host (unreachable, removed, or SSH auth failed).".into()
        }
        "folder_missing" => "That folder no longer exists.".into(),
        "folder_not_directory" => "That path is not a directory.".into(),
        "folder_unreadable" => "That directory cannot be read.".into(),
        "folder_protected" => "Lico keeps its own private state there.".into(),
        "file_missing" => "That file no longer exists (press F5 to refresh).".into(),
        "file_not_regular" => "Only regular files are previewed (symlinks and special files are skipped).".into(),
        "file_unreadable" => "That file could not be read.".into(),
        other => format!("Could not read ({other})."),
    }
}

impl Explorer {
    pub(crate) fn host_choices(&self) -> Vec<Option<String>> {
        let mut choices: Vec<Option<String>> = vec![None];
        for h in &self.ssh_hosts {
            if !choices.iter().any(|c| c.as_deref() == Some(h.as_str())) {
                choices.push(Some(h.clone()));
            }
        }
        if let Some(cur) = &self.ssh_host {
            if !choices.iter().any(|c| c.as_deref() == Some(cur.as_str())) {
                choices.push(Some(cur.clone()));
            }
        }
        choices
    }

    pub(crate) fn sync_popup_scroll(&mut self, max_vis: usize) {
        let total = self.host_choices().len();
        if total == 0 {
            self.host_popup_sel = 0;
            self.host_popup_top = 0;
            return;
        }
        let vis = max_vis.max(1);
        self.host_popup_sel = self.host_popup_sel.min(total - 1);
        let max_top = total.saturating_sub(vis);
        if self.host_popup_sel < self.host_popup_top {
            self.host_popup_top = self.host_popup_sel;
        } else if self.host_popup_sel >= self.host_popup_top + vis {
            self.host_popup_top = self.host_popup_sel + 1 - vis;
        }
        self.host_popup_top = self.host_popup_top.min(max_top);
    }

    pub(super) fn nav_len(&self) -> usize {
        if self.loading.is_some() && self.entries.is_empty() && self.preview.is_none() {
            1
        } else if self.unreachable {
            1 + usize::from(self.prev_host.is_some())
        } else {
            1 + usize::from(self.parent.is_some()) + self.entries.len()
        }
    }

    pub(super) fn page_window(&self, page_size: usize) -> (usize, usize, usize, usize) {
        let ps = page_size.max(1);
        let total = self.nav_len();
        let total_pages = total.div_ceil(ps).max(1);
        let page_idx = (self.cursor / ps).min(total_pages - 1);
        let start = page_idx * ps;
        let end = (start + ps).min(total);
        (start, end, page_idx, total_pages)
    }

    pub(super) fn row_label(&self, nav_idx: usize, max_cols: usize) -> String {
        let mark = if nav_idx == self.cursor { '>' } else { ' ' };
        let raw = if nav_idx == 0 {
            format!("{mark} [+ New session in this directory]   (N or click)")
        } else {
            let mut rest = nav_idx - 1;
            if self.parent.is_some() {
                if rest == 0 {
                    return clip_cols(&format!("{mark} [..]  (parent directory)"), max_cols);
                }
                rest -= 1;
            }
            let Some(item) = self.entries.get(rest) else { return String::new() };
            match item.kind.as_str() {
                "dir" => format!("{mark} [dir]   {}/", item.name),
                "link" => format!("{mark} [link]  {}  (symlink)", item.name),
                _ if item.is_html => format!("{mark} [html]  {}  ({})  [system default]", item.name, format_bytes(item.size)),
                _ if item.external => format!("{mark} [open]  {}  ({})  [system default]", item.name, format_bytes(item.size)),
                _ => format!("{mark} [file]  {}  ({})", item.name, format_bytes(item.size)),
            }
        };
        clip_cols(&raw, max_cols)
    }

    pub(super) fn selected_entry(&self) -> Option<&FileItem> {
        let base = 1 + usize::from(self.parent.is_some());
        self.cursor.checked_sub(base).and_then(|i| self.entries.get(i))
    }
}

impl App {
    pub(super) fn files_row_cols(&self) -> usize {
        if self.ui.panel_cols >= 24 {
            self.ui.panel_cols.saturating_sub(2)
        } else {
            0
        }
    }

    pub(crate) fn files_host_popup_draw_state(&self) -> Option<(Vec<Option<String>>, Option<String>, usize, usize)> {
        let ex = self
            .ui
            .panel
            .as_ref()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_ref())
            .filter(|ex| ex.host_popup_open)?;
        let choices = ex.host_choices();
        if choices.is_empty() {
            return None;
        }
        Some((choices, ex.ssh_host.clone(), ex.host_popup_sel, ex.host_popup_top))
    }

    pub(crate) fn files_host_popup_json(&self) -> Value {
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return json!({"open": false, "selected": 0, "top": 0, "items": []});
        };
        let choices = ex.host_choices();
        let items: Vec<Value> = choices
            .iter()
            .enumerate()
            .map(|(idx, ch)| {
                let active = *ch == ex.ssh_host;
                let mark = if active { "* " } else { "  " };
                let label = match ch {
                    None => format!("{mark}local (this machine)"),
                    Some(h) => format!("{mark}ssh: {h}"),
                };
                let rect = self.ui.toolbar_geom.host_items.iter().find(|(i, _)| *i == idx).map(|(_, r)| *r);
                json!({"index": idx, "host": ch, "label": label, "active": active, "rect": rect})
            })
            .collect();
        json!({
            "open": ex.host_popup_open,
            "selected": ex.host_popup_sel.min(choices.len().saturating_sub(1)),
            "top": ex.host_popup_top,
            "max_visible": HOST_POPUP_MAX_VIS,
            "items": items,
        })
    }

    pub(crate) fn files_view(&self, p: &super::Panel) -> (String, Vec<String>) {
        let Some(ex) = p.files.as_ref() else {
            return ("Files".into(), Vec::new());
        };
        let max_cols = self.files_row_cols();
        let host_prefix = ex.ssh_host.as_deref().map_or_else(String::new, |h| format!(" [ssh:{h}]"));

        if let Some(msg) = ex.loading.as_deref().filter(|_| ex.entries.is_empty() && ex.preview.is_none()) {
            let body = vec![
                clip_cols("> [Choose another host...]   (Enter, H, or click Host)", max_cols),
                String::new(),
                clip_cols(msg, max_cols),
                clip_cols("Press Esc to cancel, or click 'Host' (H) to choose another host.", max_cols),
            ];
            return (format!("Files{host_prefix}: connecting..."), body);
        }

        if ex.unreachable {
            let m0 = if ex.cursor == 0 { '>' } else { ' ' };
            let mut body = vec![clip_cols(&format!("{m0} [Choose another host...]   (Enter, H, or click Host)"), max_cols)];
            if let Some(prev) = &ex.prev_host {
                let m1 = if ex.cursor == 1 { '>' } else { ' ' };
                let prev_lbl = prev.ssh_host.as_deref().map_or_else(|| "local".to_string(), |h| format!("ssh:{h}"));
                body.push(clip_cols(&format!("{m1} [<- Back to {prev_lbl}: {}]   (Backspace or click)", prev.cwd), max_cols));
            }
            body.push(String::new());
            body.push(clip_cols(
                &format!("Could not connect to ssh:{}.", ex.ssh_host.as_deref().unwrap_or("remote")),
                max_cols,
            ));
            body.push(clip_cols(
                "The host may be offline, removed, or missing BatchMode SSH key authentication.",
                max_cols,
            ));
            body.push(clip_cols(
                "Click 'Host' (H) to pick another host, F5 to retry, or Backspace to go back.",
                max_cols,
            ));
            return (format!("Files{host_prefix}: unreachable"), body);
        }

        if let Some(pv) = &ex.preview {
            let ps = calc_page_size(self.ui.panel_rows, true);
            let mut body = vec!["[<- Back to directory]   (Backspace or Esc)".into(), String::new()];
            if pv.binary {
                body.push("(binary file — text preview is shown only for UTF-8 files)".into());
            } else if pv.lines.is_empty() || (pv.lines.len() == 1 && pv.lines[0].is_empty()) {
                body.push("(empty file)".into());
            } else {
                let total_pages = pv.lines.len().div_ceil(ps).max(1);
                let page_num = (pv.top / ps).min(total_pages - 1) + 1;
                let end = (pv.top + ps).min(pv.lines.len());
                body.extend(pv.lines[pv.top..end].iter().map(|l| clip_cols(l, max_cols)));
                body.push(String::new());
                body.push(clip_cols(
                    &format!(
                        "Page {page_num} / {total_pages}  (lines {}-{} of {}{})   |   PgUp/PgDn or [/] or wheel   F5 refresh   N new session",
                        pv.top + 1,
                        end,
                        pv.lines.len(),
                        if pv.truncated { ", first 64 KiB" } else { "" }
                    ),
                    max_cols,
                ));
            }
            return (format!("File{host_prefix}: {}  ({})", pv.path, format_bytes(pv.size)), body);
        }

        let ps = calc_page_size(self.ui.panel_rows, false);
        let (start, end, page_idx, total_pages) = ex.page_window(ps);
        let mut body: Vec<String> = (start..end).map(|i| ex.row_label(i, max_cols)).collect();
        body.push(String::new());
        let hidden_tag = if self.ui.show_hidden { "hidden: shown" } else { "hidden: off" };
        body.push(clip_cols(
            &format!(
                "Page {} / {total_pages}  ({} entries{}, {hidden_tag})  |  H host  . or Ctrl+H hidden  PgUp/PgDn  F5 refresh  N new  Enter open",
                page_idx + 1,
                ex.entries.len(),
                if ex.truncated { ", first 10000 shown" } else { "" }
            ),
            max_cols,
        ));
        let heading = if total_pages > 1 {
            format!("Files{host_prefix}: {}  [Page {} / {total_pages}]", ex.cwd, page_idx + 1)
        } else {
            format!("Files{host_prefix}: {}", ex.cwd)
        };
        (heading, body)
    }

    pub(crate) fn files_json(&self) -> Value {
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return Value::Null;
        };
        let entries: Vec<Value> = ex
            .entries
            .iter()
            .take(1000)
            .map(|e| json!({"name": e.name, "kind": e.kind, "size": e.size, "is_html": e.is_html, "external": e.external}))
            .collect();
        let (page, total_pages, page_size, vis_start, vis_end) = if let Some(pv) = &ex.preview {
            let ps = calc_page_size(self.ui.panel_rows, true);
            let tp = pv.lines.len().div_ceil(ps).max(1);
            let pg = (pv.top / ps).min(tp - 1) + 1;
            (pg, tp, ps, pv.top, (pv.top + ps).min(pv.lines.len()))
        } else {
            let ps = calc_page_size(self.ui.panel_rows, false);
            let (s, e, p_idx, tp) = ex.page_window(ps);
            (p_idx + 1, tp, ps, s, e)
        };
        let preview = ex.preview.as_ref().map(|pv| {
            json!({
                "path": pv.path,
                "name": pv.name,
                "size": pv.size,
                "binary": pv.binary,
                "truncated": pv.truncated,
                "lines_total": pv.lines.len(),
                "top": pv.top,
            })
        });
        let mode = if ex.loading.is_some() && ex.entries.is_empty() && ex.preview.is_none() {
            "loading"
        } else if ex.unreachable {
            "unreachable"
        } else if ex.preview.is_some() {
            "preview"
        } else {
            "dir"
        };
        json!({
            "ssh_host": ex.ssh_host,
            "cwd": ex.cwd,
            "parent": ex.parent,
            "show_hidden": self.ui.show_hidden,
            "mode": mode,
            "loading": ex.loading,
            "unreachable": ex.unreachable,
            "host_popup": self.files_host_popup_json(),
            "cursor": ex.cursor,
            "page": page,
            "total_pages": total_pages,
            "page_size": page_size,
            "visible_range": [vis_start, vis_end],
            "entry_count": ex.entries.len(),
            "entries": entries,
            "truncated": ex.truncated,
            "load_count": ex.load_count,
            "refresh_count": ex.refresh_count,
            "last_opened": ex.last_opened,
            "preview": preview,
        })
    }
}
