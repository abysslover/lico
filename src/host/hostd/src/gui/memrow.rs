//! The memory of a session as the window shows it: a short figure at the right end of its row (the RSS of the whole tree; orange from 400 MB and red
//! from 500 MB of its largest process, as the host sends the limits), a sentence in the hint line while the mouse is over the row (the figure of its own,
//! `RssAnon` plus `RssShmem`, and the RSS beside it), and one line at the bottom of the list with the sessions together, the host and this window (when the
//! machine runs low it starts with the machine's available share). The host and window figures use the host's definition (see `crate::memory`): what a
//! program holds of its own, never PSS; the window's own figure is read the same way from its own `/proc` entry.

use cosmic_text::{Color, FontSystem, SwashCache};
use serde_json::Value;

use super::Row;
use crate::gui::controls::{Action, Controls, Rect};
use crate::gui::App;
use crate::memory::Figure;

const NAME_COLS: usize = 24; // the longest name the hint sentence repeats
const FLOOR_COLS: usize = 6; // the shortest it is cut to, so that the figures and the caveats stay in view

/// What the list drew last time and what the mouse is over.
#[derive(Default)]
pub struct Look {
    pub host: Figure,
    pub window: Figure,
    pub hover: Option<String>, // the session under the mouse
    /// The machine's `MemAvailable` and `MemTotal` in kB as the host last said (none until it does).
    pub machine: Option<(u64, u64)>,
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

/// The worst level among the rows at the last `total` call (0 ok, 1 orange, 2 red): the colour of the line under the list. Per thread: the window draws on one thread, and the unit tests
/// that call `total` run on their own threads, so one cannot colour the line another test draws (the scroll oracle compared two frames across such a call once).
thread_local! {
    static WORST: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// The worst level among the rows.
pub fn worst(rows: &[Row]) -> lico_memguard::Level {
    let now = now_secs();
    rows.iter().map(|r| if stalled(r, now) { level(r).max(lico_memguard::Level::Orange) } else { level(r) }).max().unwrap_or(lico_memguard::Level::Ok)
}

/// The colour of the total line: orange or red when a session is, else `plain`.
pub fn total_ink(plain: Color) -> Color {
    match WORST.with(std::cell::Cell::get) {
        2 => ink(lico_memguard::Level::Red, plain),
        1 => ink(lico_memguard::Level::Orange, plain),
        _ => plain,
    }
}

/// The warning level and the limit in MB the host sent (the defaults until it does).
static WARN_MB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(400);
static LIMIT_MB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(500);
static STALL_MIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(lico_memguard::DEFAULT_STALL_MIN);

/// Minutes without activity after which a working session is stalled (what the host sent; 0: off).
pub fn stall_min() -> u64 {
    STALL_MIN.load(std::sync::atomic::Ordering::Relaxed)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Seconds since the session's last activity at `now`.
pub fn quiet_secs(row: &Row, now: u64) -> u64 {
    now.saturating_sub(row.last_active_at)
}

/// A working session with no activity for the set time.
pub fn stalled(row: &Row, now: u64) -> bool {
    row.last_active_at > 0 && lico_memguard::stalled(row.activity == "working", quiet_secs(row, now), stall_min())
}

pub fn limits() -> lico_memguard::Limits {
    lico_memguard::Limits { warn_mb: WARN_MB.load(std::sync::atomic::Ordering::Relaxed), limit_mb: LIMIT_MB.load(std::sync::atomic::Ordering::Relaxed) }
}

/// Takes the limits of a `status` reply when they are usable.
pub fn set_limits(status: &Value) {
    let l = lico_memguard::Limits { warn_mb: status["memory_limits"]["warn_mb"].as_u64().unwrap_or(0), limit_mb: status["memory_limits"]["limit_mb"].as_u64().unwrap_or(0) };
    if let Some(m) = status["memory_limits"]["stall_min"].as_u64() {
        STALL_MIN.store(m, std::sync::atomic::Ordering::Relaxed);
    }
    if l.valid() {
        WARN_MB.store(l.warn_mb, std::sync::atomic::Ordering::Relaxed);
        LIMIT_MB.store(l.limit_mb, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The level of a session for the memory guard: its largest single process against the warning level and the limit (400 and 500 MB unless the owner's setting says otherwise).
pub fn level(row: &Row) -> lico_memguard::Level {
    limits().level(row.max_rss_kb)
}

/// The colour of a figure: the row's own below the warning level, orange from it, red from the limit.
pub fn ink(level: lico_memguard::Level, normal: Color) -> Color {
    match level {
        lico_memguard::Level::Red => Color::rgb(0xF0, 0x70, 0x70),
        lico_memguard::Level::Orange => Color::rgb(0xF0, 0x98, 0x40),
        lico_memguard::Level::Ok => normal,
    }
}

/// The figure of a row, none while the host has not given one (a session that is not running holds nothing).
pub fn cell(row: &Row) -> Option<Cell> {
    (row.memory_kb > 0).then(|| Cell { text: short(if row.rss_kb > 0 { row.rss_kb } else { row.memory_kb }), ink: ink(if stalled(row, now_secs()) { level(row).max(lico_memguard::Level::Orange) } else { level(row) }, Color::rgb(0x9A, 0xA4, 0xB0)) })
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
    WORST.with(|w| w.set(worst(rows).max(look.machine_level()) as u8)); // the line is drawn after this call in the same frame, by four callers that each pass their own plain colour
    let sessions: u64 = rows.iter().map(|r| if r.rss_kb > 0 { r.rss_kb } else { r.memory_kb }).sum();
    if let Some(pct) = look.machine_percent().filter(|_| look.machine_level() != lico_memguard::Level::Ok) {
        // the machine runs low: its figure comes first, and the line is as short as the list allows (at most 39 columns)
        let (sessions, window) = (short(sessions), short(look.window.private_kb));
        let with_host = format!("free {pct}% sessions {sessions} host {} win {window}", short(look.host.private_kb));
        return if look.host.private_kb > 0 && with_host.chars().count() <= 39 { with_host } else { format!("free {pct}% sessions {sessions} win {window}") }; // the host figure is the first to go when the line is too long
    }
    let host = if look.host.private_kb > 0 { format!("  host {}", short(look.host.private_kb)) } else { String::new() };
    format!("mem sessions {}{host}  window {}", short(sessions), short(look.window.private_kb)) // at most 39 columns: what the list is wide enough for
}

/// The total line above the hint line, when the list leaves room for it (`at`: left, top, right edge of the list, and the bottom of its last row).
#[allow(clippy::too_many_arguments)]
pub fn draw_total(controls: &mut Controls, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, at: [f32; 4], text: &str, ink: Color) {
    if at[1] >= at[3] {
        controls.text(fs, swash, buf, width, height, at[0], at[1], at[2] as i32, text, total_ink(ink));
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
    let quiet = if row.last_active_at > 0 && row.activity == "working" { format!("; {}", lico_memguard::quiet_text(quiet_secs(row, now_secs()))) } else { String::new() };
    Some(format!("{name}{rest}{quiet}").chars().take(cols).collect())
}

/// The warning for the person starting a session while the machine runs low on memory (a warning, never a block); none while it is well.
pub fn new_session_warning(look: &Look) -> Option<String> {
    let pct = look.machine_percent()?;
    match look.machine_level() {
        lico_memguard::Level::Ok => None,
        lico_memguard::Level::Orange => Some(format!("Memory on this machine is getting low ({pct}% available). The new session may fail; consider closing a heavy session.")),
        lico_memguard::Level::Red => Some(format!("Memory on this machine is very low ({pct}% available): the system may kill sessions. Close a heavy session first if you can.")),
    }
}

impl Look {
    /// The share of the machine's memory that is available, in percent.
    pub fn machine_percent(&self) -> Option<u64> {
        self.machine.map(|(avail, total)| avail.saturating_mul(100) / total.max(1))
    }

    /// The machine's level: orange under 20 percent available, red under 10 (ok while the host has not said).
    pub fn machine_level(&self) -> lico_memguard::Level {
        self.machine.map_or(lico_memguard::Level::Ok, |(avail, total)| lico_memguard::machine_level(avail, total))
    }

    /// A status reply: the host's figure, and this window's own (read every third time; one small file).
    pub fn read(&mut self, status: &Value) {
        set_limits(status);
        let m = &status["machine_memory"];
        self.machine = m["total_kb"].as_u64().filter(|t| *t > 0).map(|t| (m["available_kb"].as_u64().unwrap_or(0), t));
        let kb = |k: &str| status["host_memory"][k].as_u64().unwrap_or(0);
        self.host = Figure { private_kb: kb("private_kb"), rss_kb: kb("rss_kb"), max_rss_kb: kb("rss_kb"), processes: 1 };
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
        let hit = self.ui.controls.hit_at(x, y);
        let popup_changed = match &hit {
            Some(Action::HostItem(idx)) => self.hover_files_host_item(*idx),
            Some(Action::SidebarHostItem(idx)) => self.hover_sidebar_host_item(*idx),
            Some(Action::SidebarBookmarkItem(idx)) => self.hover_sidebar_bookmark_item(*idx),
            _ => false,
        };
        let over = match hit {
            Some(Action::Session(id) | Action::Tab(id)) => Some(id),
            _ => None,
        };
        let changed = over != self.ui.mem.hover;
        self.ui.mem.hover = over;
        changed || popup_changed
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
    fn the_colour_turns_orange_at_400_mb_and_red_at_500_mb_by_the_largest_process_and_a_session_without_a_figure_has_none() {
        let normal = Color::rgb(1, 2, 3);
        let at = |kb| {
            let r = Row { max_rss_kb: kb, ..row(5000, 9000, 2) };
            cell(&r).unwrap().ink
        };
        let orange = at(400 * 1024);
        let red = at(500 * 1024);
        assert_ne!(at(400 * 1024 - 1), orange);
        assert!(orange != red && at(500 * 1024 - 1) == orange && at(u64::MAX) == red);
        assert_ne!(at(0), orange, "a small process keeps the row's own colour");
        assert_eq!(ink(lico_memguard::Level::Ok, normal), normal);
        let many_small = Row { max_rss_kb: 100 * 1024, rss_kb: 900 * 1024, memory_kb: 800 * 1024, ..row(0, 0, 9) };
        assert_eq!(level(&many_small), lico_memguard::Level::Ok, "the sum of many small processes is not the colour: the largest one is");
        assert!(cell(&row(0, 0, 0)).is_none() && cell(&row(5000, 9000, 2)).is_some());
        assert_ne!(sig("x", &cell(&row(5000, 9000, 2))), sig("x", &None), "a new figure redraws the list");
    }

    #[test]
    fn the_total_line_takes_the_level_of_the_worst_session() {
        use lico_memguard::Level::{Ok as Fine, Orange, Red};
        let at = |kb: &[u64]| worst(&kb.iter().map(|k| Row { max_rss_kb: *k * 1024, ..row(10, 10, 1) }).collect::<Vec<_>>());
        assert_eq!((at(&[10, 399]), at(&[10, 450]), at(&[450, 10, 500]), at(&[])), (Fine, Orange, Red, Fine));
        let plain = Color::rgb(1, 2, 3);
        assert!(ink(Orange, plain) != plain && ink(Red, plain) != ink(Orange, plain));
    }

    #[test]
    fn a_working_session_with_no_activity_for_ten_minutes_is_stalled_and_the_hint_says_when_it_last_did_something() {
        let now = 1_000_000;
        let working = |ago: u64| Row { activity: "working".into(), last_active_at: now - ago, ..row(350 * 1024, 700 * 1024, 2) };
        assert!(!stalled(&working(599), now) && stalled(&working(600), now) && stalled(&working(3 * 3600 + 1800), now));
        let idle = Row { activity: "idle".into(), last_active_at: now - 7200, ..row(1, 1, 1) };
        assert!(!stalled(&idle, now), "an idle session waits for the person");
        assert!(!stalled(&Row { activity: "working".into(), last_active_at: 0, ..row(1, 1, 1) }, now), "no time known: not stalled");
        assert_eq!(quiet_secs(&working(125), now), 125);
        let hint = sentence(&Row { last_active_at: now_secs().saturating_sub(5 * 60), ..working(0) }, 300).unwrap();
        assert!(hint.contains("; last activity 5 min ago"), "{hint}");
        assert!(!sentence(&Row { activity: "idle".into(), last_active_at: 5, ..row(350 * 1024, 700 * 1024, 2) }, 300).unwrap().contains("last activity"), "only a working session says it");
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
        assert_eq!(total(&[a, row(850 * 1024, 900 * 1024, 2)], &look), "mem sessions 1.6G  host 41M  window 28M", "the RSS of the sessions, as the rows show");
        assert_eq!(total(&[], &Look::default()), "mem sessions <1M  window <1M");
    }

    #[test]
    fn the_row_figure_is_the_rss_of_the_whole_tree_and_the_machine_line_and_the_new_session_warning_come_from_the_status_reply() {
        assert_eq!(cell(&row(350 * 1024, 707 * 1024, 4)).unwrap().text, "710M", "the RSS, which is what the colour follows");
        assert_eq!(cell(&row(350 * 1024, 0, 1)).unwrap().text, "350M", "no RSS from the host: its own figure");
        let gib = 1024 * 1024;
        let at = |avail_gib: f64| {
            let mut look = Look::default();
            look.read(&serde_json::json!({"machine_memory": {"available_kb": (avail_gib * gib as f64) as u64, "total_kb": 16 * gib}}));
            look
        };
        let quiet = Look::default();
        assert_eq!((quiet.machine_level(), new_session_warning(&quiet)), (lico_memguard::Level::Ok, None), "a host that says nothing: no line, no warning");
        assert_eq!((at(8.0).machine_level(), new_session_warning(&at(8.0))), (lico_memguard::Level::Ok, None));
        let orange = at(2.5); // 15 percent
        assert_eq!(orange.machine_level(), lico_memguard::Level::Orange);
        assert!(new_session_warning(&orange).unwrap().contains("15%"), "{:?}", new_session_warning(&orange));
        let red = at(1.0); // 6 percent
        assert_eq!(red.machine_level(), lico_memguard::Level::Red);
        assert!(new_session_warning(&red).unwrap().contains("6%"));
        let mut look = Look { host: Figure { private_kb: 41 * 1024, ..Default::default() }, window: Figure { private_kb: 28 * 1024, ..Default::default() }, ..at(1.0) };
        let line = total(&[row(1200 * 1024, 1200 * 1024, 2)], &look);
        assert_eq!(line, "free 6% sessions 1.2G host 41M win 28M");
        assert!(line.chars().count() <= 39);
        let wide = Look { host: Figure { private_kb: 100 * 1024, ..Default::default() }, window: Figure { private_kb: 100 * 1024, ..Default::default() }, ..at(2.5) };
        let line = total(&[row(10137 * 1024, 10137 * 1024, 2)], &wide);
        assert_eq!(line, "free 15% sessions 9.9G win 100M", "{line}: with the host it would be 41 columns");
        assert!(line.chars().count() <= 39);
        look.read(&serde_json::json!({"machine_memory": {"available_kb": 1, "total_kb": 0}}));
        assert_eq!(look.machine_level(), lico_memguard::Level::Ok, "no total: not known");
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
