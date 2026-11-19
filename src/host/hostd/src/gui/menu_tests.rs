//! The menus as trees (`M22-P04a`): no entry is lost, a dropdown opens with its groups closed, a heading opens and closes in place, an entry in an open group runs and a greyed one still says why.

use super::super::scroll_oracle::app as window;
use super::*;

fn labels(rows: &[MenuRow]) -> Vec<String> {
    rows.iter().map(|r| r.label.clone()).collect()
}

fn open_all(app: &mut App, menu: usize) {
    let groups: Vec<&'static str> = app.menu_entries(menu).iter().filter_map(|e| group_of(e.cmd)).collect();
    app.ui.menu_geom.groups_open = groups;
}

#[test]
fn a_dropdown_opens_with_its_groups_closed_and_the_entries_that_stay_are_on_the_dropdown() {
    let mut app = window(1.0, false);
    app.menu_open_at(1);
    let rows = app.menu_rows(1);
    let heads: Vec<&str> = rows.iter().filter_map(|r| if let RowKind::Group(g, false) = r.kind { Some(g) } else { None }).collect();
    assert_eq!(heads, ["Browser pages", "Go to", "This session", "Share", "Tools", "Approvals", "Edit"], "{:?}", labels(&rows));
    assert!(rows.iter().all(|r| r.depth == 0 && r.kind != RowKind::Group("Go to", true)), "nothing is indented while every group is closed");
    let head = |l: &str| labels(&rows).iter().position(|x| x == l);
    assert!(head("New Home session") < head("Go to") && head("Go to") < head("Start screen"), "a group stands where its first entry was: {:?}", labels(&rows));
    assert!(rows.len() <= app.menu_entries(1).len() / 2, "the long list is short now: {} of {}", rows.len(), app.menu_entries(1).len());
    assert_eq!(app.menu_rows(0).len(), app.menu_entries(0).len(), "File has no group");
    assert_eq!(app.menu_rows(3).len(), app.menu_entries(3).len(), "Help has no group");
    let view = app.menu_rows(2);
    let view_heads: Vec<&str> = view.iter().filter_map(|r| if let RowKind::Group(g, _) = r.kind { Some(g) } else { None }).collect();
    assert_eq!(view_heads, ["Find", "Scroll", "Show", "Text size", "Settings"], "{:?}", labels(&view));
}

#[test]
fn no_entry_of_any_menu_is_lost_when_every_group_is_open() {
    let mut app = window(1.0, false);
    for menu in 0..4 {
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
    app.menu_open_at(1);
    let at = |app: &App, g: &str| app.menu_rows(1).iter().position(|r| matches!(r.kind, RowKind::Group(x, _) if x == g)).unwrap();
    let share = at(&app, "Share");
    app.menu_row_run(1, share);
    let rows = app.menu_rows(1);
    assert_eq!(rows[share].kind, RowKind::Group("Share", true));
    assert!(rows[share + 1].label == "Send to session..." && rows[share + 1].depth == 1 && rows[share + 2].label == "Hand over conversation..." && rows[share + 2].depth == 1, "{:?}", labels(&rows));
    assert_eq!(rows[share + 3].kind, RowKind::Group("Tools", false), "the next group's heading follows, still closed");
    assert_eq!(app.ui.menu_sel, share, "the selection stays on the heading");
    assert_eq!(app.ui.menu_open, Some(1), "a heading does not close the menu");
    app.menu_row_run(1, share);
    assert_eq!(app.menu_rows(1)[share].kind, RowKind::Group("Share", false));
    assert!(!app.menu_rows(1).iter().any(|r| r.label == "Send to session..."));
}

#[test]
fn an_entry_in_an_open_group_runs_like_before_and_a_greyed_one_says_why() {
    let mut app = window(1.0, false);
    app.menu_open_at(1);
    app.menu_group_set(1, "Share", true);
    let at = app.menu_rows(1).iter().position(|r| r.label == "Send to session...").unwrap();
    let reason = app.menu_rows(1)[at].reason.expect("with no session open the entry is greyed");
    app.menu_row_run(1, at);
    assert_eq!(app.ui.menu_open, None, "an entry closes the menu");
    assert!(app.ui.flash.as_deref().is_some_and(|f| f.starts_with("Send to session...") && f.ends_with(reason)), "{:?}", app.ui.flash);
    // a group that is open does not stay open: the dropdown opens closed the next time
    app.menu_open_at(1);
    assert!(app.menu_rows(1).iter().all(|r| r.label != "Send to session..."));
}

#[test]
fn the_selection_of_a_dropdown_is_a_row_and_starts_on_the_first_row_that_can_act() {
    let mut app = window(1.0, false);
    app.menu_open_at(1);
    let rows = app.menu_rows(1);
    assert_eq!(rows[app.ui.menu_sel].label, "New Home session");
    assert!(rows[app.ui.menu_sel].reason.is_none());
}
