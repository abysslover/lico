//! The menus as trees (`M22-P04a`) after the diet of `M24-P04`: the four menus show only what the owner kept on show, and the menu `More` holds the rest as groups. No entry is lost, a dropdown opens
//! with its groups closed, a heading opens and closes in place, an entry in an open group runs and a greyed one still says why.

use super::super::scroll_oracle::app as window;
use super::*;

const MORE: usize = 4;

fn labels(rows: &[MenuRow]) -> Vec<String> {
    rows.iter().map(|r| r.label.clone()).collect()
}

fn open_all(app: &mut App, menu: usize) {
    let groups: Vec<&'static str> = app.menu_entries(menu).iter().filter_map(|e| group_of(e.cmd)).collect();
    app.ui.menu_geom.groups_open = groups;
}

#[test]
fn the_four_menus_show_only_what_was_kept_on_show_and_have_no_group() {
    let app = window(1.0, false);
    let shown = |menu: usize| app.menu_entries(menu).iter().map(|e| e.label.split(':').next().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(shown(0), ["New session", "Accounts"]);
    assert_eq!(shown(1), ["New Home session", "New browser tab", "Permission mode", "Send to session...", "Hand over conversation..."]);
    assert_eq!(shown(2), ["Working directory files"]);
    assert_eq!(shown(3), ["Keys"]);
    for menu in 0..4 {
        assert_eq!(app.menu_rows(menu).len(), app.menu_entries(menu).len(), "menu {menu} has no group: a row for each entry");
    }
    assert_eq!(TITLES, ["File", "Session", "View", "Help", "More"]);
}

#[test]
fn what_was_not_dropped_or_given_a_key_only_is_on_more_and_a_dropdown_opens_with_its_groups_closed() {
    let mut app = window(1.0, false);
    app.menu_open_at(MORE);
    let rows = app.menu_rows(MORE);
    let heads: Vec<&str> = rows.iter().filter_map(|r| if let RowKind::Group(g, false) = r.kind { Some(g) } else { None }).collect();
    assert_eq!(heads, ["Browser pages", "This session", "Tools", "Show", "Text size", "Settings"], "{:?}", labels(&rows));
    assert!(rows.iter().all(|r| r.depth == 0), "nothing is indented while every group is closed");
    assert_eq!(app.menu_entries(MORE).len(), 23, "the owner's twenty-one hidden commands, the switch of the agents' control of the panes (M24-P05b) and the start of the answers (M25-P09)");
    let at = |l: &str| labels(&rows).iter().position(|x| x == l);
    assert!(at("New local browser tab") < at("Browser pages") && at("Browser pages") < at("Start screen") && at("Start screen") < at("This session"), "a group stands where its first entry was: {:?}", labels(&rows));
    assert!(rows.len() <= app.menu_entries(MORE).len() / 2 + 1, "the long list is short: {} of {}", rows.len(), app.menu_entries(MORE).len());
}

#[test]
fn the_keys_only_and_the_dropped_commands_have_no_line_in_any_menu() {
    let app = window(1.0, false);
    let all: Vec<String> = (0..TITLES.len()).flat_map(|m| app.menu_entries(m)).map(|e| e.label).collect();
    for gone in ["Close session", "Allow approval", "Deny approval", "Copy selected lines", "Cut selection", "Paste", "Find in conversation", "Search all sessions", "Older output", "Newer output", "Newest output", "Quit", "Next session", "Previous session", "Stop real-screen control", "New session options...", "New terminal tab"] {
        assert!(!all.iter().any(|l| l == gone), "{gone} has no line");
    }
    assert!(!all.iter().any(|l| l.starts_with("Session ") || l.starts_with("Hidden files") || l.starts_with("Left sidebar")), "{all:?}");
    assert_eq!(all.len(), 2 + 5 + 1 + 1 + 23, "nine on show and twenty-three on More");
}

#[test]
fn no_entry_of_any_menu_is_lost_when_every_group_is_open() {
    let mut app = window(1.0, false);
    for menu in 0..TITLES.len() {
        app.menu_open_at(menu);
        open_all(&mut app, menu);
        let rows = app.menu_rows(menu);
        let mut seen: Vec<usize> = rows.iter().filter_map(|r| if let RowKind::Entry(i) = r.kind { Some(i) } else { None }).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..app.menu_entries(menu).len()).collect::<Vec<_>>(), "menu {menu}: every entry is a row exactly once");
        assert!(rows.iter().filter(|r| r.depth == 1).count() == rows.iter().filter(|r| matches!(r.kind, RowKind::Entry(i) if group_of(app.menu_entries(menu)[i].cmd).is_some())).count(), "entries in a group are the indented ones");
    }
}

#[test]
fn a_heading_opens_and_closes_in_place_and_the_group_opens_after_its_heading_only() {
    let mut app = window(1.0, false);
    app.menu_open_at(MORE);
    let at = |app: &App, g: &str| app.menu_rows(MORE).iter().position(|r| matches!(r.kind, RowKind::Group(x, _) if x == g)).unwrap();
    let session = at(&app, "This session");
    app.menu_row_run(MORE, session);
    let rows = app.menu_rows(MORE);
    assert_eq!(rows[session].kind, RowKind::Group("This session", true));
    assert_eq!(labels(&rows[session + 1..session + 5]), ["Resume session", "Rebind account...", "Rename session", "Delete session"]);
    assert!(rows[session + 1..session + 5].iter().all(|r| r.depth == 1), "{:?}", labels(&rows));
    assert_eq!(rows[session + 5].kind, RowKind::Group("Tools", false), "the next group's heading follows, still closed");
    assert_eq!(app.ui.menu_sel, session, "the selection stays on the heading");
    assert_eq!(app.ui.menu_open, Some(MORE), "a heading does not close the menu");
    app.menu_row_run(MORE, session);
    assert_eq!(app.menu_rows(MORE)[session].kind, RowKind::Group("This session", false));
    assert!(!app.menu_rows(MORE).iter().any(|r| r.label == "Resume session"));
}

#[test]
fn an_entry_in_an_open_group_runs_like_before_and_a_greyed_one_says_why() {
    let mut app = window(1.0, false);
    app.menu_open_at(MORE);
    app.menu_group_set(MORE, "This session", true);
    let at = app.menu_rows(MORE).iter().position(|r| r.label == "Rename session").unwrap();
    let reason = app.menu_rows(MORE)[at].reason.expect("with no session open the entry is greyed");
    app.menu_row_run(MORE, at);
    assert_eq!(app.ui.menu_open, None, "an entry closes the menu");
    assert!(app.ui.flash.as_deref().is_some_and(|f| f.starts_with("Rename session") && f.ends_with(reason)), "{:?}", app.ui.flash);
    // a group that is open does not stay open: the dropdown opens closed the next time
    app.menu_open_at(MORE);
    assert!(app.menu_rows(MORE).iter().all(|r| r.label != "Rename session"));
}

#[test]
fn the_selection_of_a_dropdown_is_a_row_and_starts_on_the_first_row_that_can_act() {
    let mut app = window(1.0, false);
    app.menu_open_at(1);
    let rows = app.menu_rows(1);
    assert_eq!(rows[app.ui.menu_sel].label, "New Home session");
    assert!(rows[app.ui.menu_sel].reason.is_none());
}

#[test]
fn each_group_of_more_holds_exactly_its_entries() {
    let app = window(1.0, false);
    let entries = app.menu_entries(MORE);
    let held = |group: &str| entries.iter().filter(|e| group_of(e.cmd) == Some(group)).map(|e| e.label.clone()).collect::<Vec<_>>();
    assert_eq!(held("Browser pages"), ["Reopen closed tab", "Clear remembered pages"]);
    assert_eq!(held("This session"), ["Resume session", "Rebind account...", "Rename session", "Delete session"]);
    assert_eq!(held("Tools"), ["Find Local LLM servers...", "Virtual display: start"]);
    assert_eq!(held("Show"), ["Group sessions: account", "Internal events: hidden"]);
    assert_eq!(held("Text size"), ["Text size: larger", "Text size: smaller", "Text size: 14 px (reset)"]);
    assert_eq!(held("Settings").len(), 6, "the clipboard write, the pane control, the notifications, the start of the answers and the two native logins: {:?}", held("Settings"));
    let on_the_dropdown: Vec<&str> = entries.iter().filter(|e| group_of(e.cmd).is_none()).map(|e| e.label.as_str()).collect();
    assert_eq!(on_the_dropdown, ["New local browser tab", "Start screen", "Capture screen to input", "About Lico"]);
}

