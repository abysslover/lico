//! The memory of a session as the window shows it: a short figure at the right end of its row (amber from 1 GiB, red from 2 GiB), a sentence in the
//! hint line while the mouse is over the row (the same figure and the RSS beside it), and one line at the bottom of the list with the sessions
//! together, the host and this window. All of them use the host's definition (see `crate::memory`): what a program holds of its own, `RssAnon` plus
//! `RssShmem`, never PSS; the window's own figure is read the same way from its own `/proc` entry.

use cosmic_text::{Color, FontSystem, SwashCache};
use serde_json::Value;

use super::Row;
use crate::gui::controls::{Action, Controls, Rect};
use crate::gui::App;
use crate::memory::Figure;

const AMBER_KB: u64 = 1 << 20; // 1 GiB
const RED_KB: u64 = 2 << 20; // 2 GiB
const NAME_COLS: usize = 24; // the longest name the hint sentence repeats
const FLOOR_COLS: usize = 6; // the shortest it is cut to, so that the figures and the caveats stay in view

/// What the list drew last time and what the mouse is over.
#[derive(Default)]
pub struct Look {
    pub host: Figure,
    pub window: Figure,
    pub hover: Option<String>, // the session under the mouse
    reads: u64,
}

/// A figure at the right end of a row, with the colour it is drawn in.
#[derive(Clone)]
pub struct Cell {
    pub text: String,
    pub ink: Color,
}

/// "<1M", "62M", "350M" (to the nearest ten, so that a figure that wobbles by a few MiB does not redraw the list), "1.2G", "12G".
pub fn short(kb: u64) -> String {
    let mib = kb as f64 / 1024.0;
    match mib {
        m if m < 1.0 => "<1M".into(),
        m if m < 100.0 => format!("{}M", m.round() as u64),
        m if m < 1000.0 => format!("{}M", (m / 10.0).round() as u64 * 10),
        m if m < 10.0 * 1024.0 => format!("{:.1}G", m / 1024.0),
        m => format!("{}G", (m / 1024.0).round() as u64),
    }
}

/// The same for a sentence: "350 MiB", "1.2 GiB".
pub fn long(kb: u64) -> String {
    let mib = kb as f64 / 1024.0;
    if mib < 1024.0 {
        format!("{} MiB", mib.round() as u64)
    } else {
        format!("{:.1} GiB", mib / 1024.0)
    }
}

/// The colour of a figure: the row's own below 1 GiB, amber from 1 GiB, red from 2 GiB.
pub fn ink(kb: u64, normal: Color) -> Color {
    match kb {
        k if k >= RED_KB => Color::rgb(0xF0, 0x70, 0x70),
        k if k >= AMBER_KB => Color::rgb(0xE8, 0xB4, 0x50),
        _ => normal,
    }
}

/// The figure of a row, none while the host has not given one (a session that is not running holds nothing).
pub fn cell(row: &Row) -> Option<Cell> {
    (row.memory_kb > 0).then(|| Cell { text: short(row.memory_kb), ink: ink(row.memory_kb, Color::rgb(0x9A, 0xA4, 0xB0)) })
}

/// The row's text with its figure: what tells the list that it has to be drawn again.
pub fn sig(text: &str, cell: &Option<Cell>) -> String {
    format!("{text}{}", cell.as_ref().map_or(String::new(), |c| format!("\t{}", c.text)))
}

/// Draw the figure at the right end of a row (the row's own text stops before it: the list is wide enough for both).
#[allow(clippy::too_many_arguments)]
pub fn draw(controls: &mut Controls, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, r: Rect, cell: &Option<Cell>) {
    if let Some(c) = cell {
        let w = controls.width(fs, &c.text);
        controls.text(fs, swash, buf, width, height, r[2] - 4.0 * controls.scale - w, r[1], r[2] as i32, &c.text, c.ink);
    }
}

/// The line under the list: the running sessions together, the host and this window.
pub fn total(rows: &[Row], look: &Look) -> String {
    let sessions: u64 = rows.iter().map(|r| r.memory_kb).sum();
    let host = if look.host.private_kb > 0 { format!("  host {}", short(look.host.private_kb)) } else { String::new() };
    format!("mem sessions {}{host}  window {}", short(sessions), short(look.window.private_kb)) // at most 39 columns: what the list is wide enough for
}

/// The total line above the hint line, when the list leaves room for it (`at`: left, top, right edge of the list, and the bottom of its last row).
#[allow(clippy::too_many_arguments)]
pub fn draw_total(controls: &mut Controls, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, at: [f32; 4], text: &str, ink: Color) {
    if at[1] >= at[3] {
        controls.text(fs, swash, buf, width, height, at[0], at[1], at[2] as i32, text, ink);
    }
}

/// The sentence for the hint line while the mouse is over a row, cut to `cols` columns.
pub fn sentence(row: &Row, cols: usize) -> Option<String> {
    if row.memory_kb == 0 {
        return None;
    }
    let processes = if row.processes == 1 { "1 process".to_string() } else { format!("{} processes", row.processes) };
    let rest = format!(": {} own (RSS {}), {processes}; detached programs not counted; memory shared between processes counts in each", long(row.memory_kb), long(row.rss_kb));
    let room = cols.saturating_sub(rest.chars().count()).clamp(FLOOR_COLS, NAME_COLS); // the name gives way, the figures and the caveats do not
    let name = row.name();
    let name = if name.chars().count() > room { name.chars().take(room - 1).chain(['…']).collect() } else { name };
    Some(format!("{name}{rest}").chars().take(cols).collect())
}

impl Look {
    /// A status reply: the host's figure, and this window's own (read every third time; one small file).
    pub fn read(&mut self, status: &Value) {
        let kb = |k: &str| status["host_memory"][k].as_u64().unwrap_or(0);
        self.host = Figure { private_kb: kb("private_kb"), rss_kb: kb("rss_kb"), processes: 1 };
        if self.reads % 3 == 0 {
            self.window = crate::memory::own();
        }
        self.reads += 1;
    }
}

impl App {
    /// The mouse moved: which session's row it is over (none outside the list); true when that changed, and the hint line has to be drawn again.
    pub(crate) fn hover_moved(&mut self) -> bool {
        let (x, y) = (self.ui.mouse.0 as f32, self.ui.mouse.1 as f32);
        let over = match self.ui.controls.hit_at(x, y) {
            Some(Action::Session(id)) => Some(id),
            _ => None,
        };
        let changed = over != self.ui.mem.hover;
        self.ui.mem.hover = over;
        changed
    }

    /// The memory sentence of the row under the mouse, in place of the keys.
    pub(crate) fn hover_hint(&self, cols: usize) -> Option<String> {
        let id = self.ui.mem.hover.as_ref()?;
        sentence(self.ui.rows.iter().find(|r| &r.session == id)?, cols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kb: u64, rss: u64, processes: u32) -> Row {
        Row { session: "s0123456789".into(), name: "api".into(), memory_kb: kb, rss_kb: rss, processes, ..Default::default() }
    }

    #[test]
    fn a_figure_is_five_characters_at_most_and_a_wobble_of_a_few_mib_does_not_change_it() {
        let shown: Vec<String> = [0, 600, 1024, 63 * 1024, 351 * 1024, 354 * 1024, 999 * 1024, 1228 * 1024, 10 * 1024 * 1024, 123 * 1024 * 1024].iter().map(|k| short(*k)).collect();
        assert_eq!(shown, ["<1M", "<1M", "1M", "63M", "350M", "350M", "1000M", "1.2G", "10G", "123G"]);
        assert!(shown.iter().all(|s| s.chars().count() <= 5));
        assert_eq!((long(350 * 1024), long(1536 * 1024)), ("350 MiB".to_string(), "1.5 GiB".to_string()));
    }

    #[test]
    fn the_colour_turns_amber_at_1_gib_and_red_at_2_gib_and_a_session_without_a_figure_has_none() {
        let normal = Color::rgb(1, 2, 3);
        let at = |kb| ink(kb, normal);
        assert_eq!(at(AMBER_KB - 1), normal);
        assert!(at(AMBER_KB) != normal && at(AMBER_KB) != at(RED_KB) && at(RED_KB - 1) == at(AMBER_KB) && at(RED_KB * 5) == at(RED_KB));
        assert!(cell(&row(0, 0, 0)).is_none() && cell(&row(5000, 9000, 2)).is_some());
        assert_ne!(sig("x", &cell(&row(5000, 9000, 2))), sig("x", &None), "a new figure redraws the list");
    }

    #[test]
    fn the_hint_sentence_names_both_figures_and_the_processes_and_the_total_line_sums_the_sessions() {
        let a = row(350 * 1024, 707 * 1024, 4);
        assert_eq!(sentence(&a, 200).unwrap(), "api: 350 MiB own (RSS 707 MiB), 4 processes; detached programs not counted; memory shared between processes counts in each");
        assert_eq!(sentence(&a, 20).unwrap().chars().count(), 20);
        assert!(sentence(&row(0, 0, 0), 200).is_none());
        assert!(sentence(&row(1, 1, 1), 200).unwrap().contains("1 process;"));
        let long_named = Row { name: "a-very-long-session-name-that-needs-the-whole-row-width-1".into(), ..row(59 * 1024, 82 * 1024, 3) };
        let said = sentence(&long_named, 200).unwrap();
        assert!(said.starts_with("a-very-long-session-nam…: 59 MiB") && said.ends_with("counts in each"), "{said}");
        assert!(total(&[row(123 * 1024 * 1024, 0, 1)], &Look { host: Figure { private_kb: 99 * 1024, ..Default::default() }, window: Figure { private_kb: 99 * 1024, ..Default::default() }, ..Default::default() }).chars().count() <= 39);
        let look = Look { host: Figure { private_kb: 41 * 1024, ..Default::default() }, window: Figure { private_kb: 28 * 1024, ..Default::default() }, ..Default::default() };
        assert_eq!(total(&[a, row(850 * 1024, 900 * 1024, 2)], &look), "mem sessions 1.2G  host 41M  window 28M");
        assert_eq!(total(&[], &Look::default()), "mem sessions <1M  window <1M");
    }

    #[test]
    fn the_hint_sentence_keeps_its_figures_and_clauses_in_the_default_window_for_every_name_up_to_the_limit() {
        const CLAUSE: &str = "memory shared between processes counts in each";
        let default_window = (1200.0_f32 / 8.5) as usize; // hint_text: the width over the width of one column, at scale 1.0
        assert_eq!(default_window, 141);
        for name in ["a", "api", "twelve-chars", "a-name-of-24-chars-long", "a-name-of-exactly-24-char", &"n".repeat(60)] {
            for (kb, rss, processes) in [(350 * 1024, 707 * 1024, 4), (1023 * 1024, 1023 * 1024, 12), (126_000_000, 127_000_000, 12)] {
                let said = sentence(&Row { name: name.into(), ..row(kb, rss, processes) }, default_window).unwrap();
                assert!(said.chars().count() <= default_window && said.ends_with(CLAUSE), "{said}");
                assert!(said.starts_with(&name.chars().take(FLOOR_COLS - 1).collect::<String>()), "{said}");
                assert!(said.contains(&long(kb)) && said.contains(&format!("(RSS {})", long(rss))), "{said}");
            }
        }
        let cut = sentence(&Row { name: "n".repeat(60), ..row(350 * 1024, 707 * 1024, 4) }, default_window).unwrap();
        assert!(cut.starts_with(&format!("{}…: 350 MiB", "n".repeat(21))), "{cut}");
    }

    #[test]
    fn a_status_reply_gives_the_host_figure_and_the_window_reads_its_own_every_third_time() {
        let mut look = Look::default();
        look.read(&serde_json::json!({"host_memory": {"private_kb": 41000, "rss_kb": 90000, "processes": 1}}));
        assert_eq!((look.host.private_kb, look.host.rss_kb), (41000, 90000));
        assert!(look.window.private_kb > 0, "this test process holds memory");
        look.window = Figure::default();
        look.read(&serde_json::json!({}));
        look.read(&serde_json::json!({}));
        assert_eq!((look.host.private_kb, look.window.private_kb), (0, 0), "a host that says nothing shows nothing, and the window is not read at every status");
        look.read(&serde_json::json!({}));
        assert!(look.window.private_kb > 0);
    }
}
