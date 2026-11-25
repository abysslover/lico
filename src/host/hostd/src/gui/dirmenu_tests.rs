use super::*;

fn labels(items: &[Item]) -> Vec<&'static str> {
    items.iter().map(|i| i.label).collect()
}

#[test]
fn a_child_path_joins_a_name_to_a_folder_with_one_slash() {
    assert_eq!(child_path("/work/a", "b"), "/work/a/b");
    assert_eq!(child_path("/work/a/", "b"), "/work/a/b");
    assert_eq!(child_path("/", "b"), "/b");
    assert_eq!(child_path("/work/a", "프로젝트"), "/work/a/프로젝트");
}

#[test]
fn a_folder_row_is_the_target_folder_and_anything_else_means_the_folder_the_list_shows() {
    let on_folder = target_for("/work", Some(("src", "dir")));
    assert_eq!((on_folder.folder.as_str(), on_folder.row.clone()), ("/work/src", Some(("src".into(), "dir".into()))));
    let on_file = target_for("/work", Some(("a.txt", "file")));
    assert_eq!((on_file.folder.as_str(), on_file.row.clone()), ("/work", Some(("a.txt".into(), "file".into()))));
    let on_link = target_for("/work/", Some(("l", "link")));
    assert_eq!(on_link.folder, "/work/", "a link is not entered: the list's folder, as the list shows it");
    let on_space = target_for("/work", None);
    assert_eq!((on_space.folder.as_str(), on_space.row), ("/work", None));
    assert_eq!(target_for("/", Some(("etc", "dir"))).folder, "/etc");
}

#[test]
fn the_menu_holds_open_terminal_first_and_new_folder_second_and_both_work_where_a_folder_is_listed() {
    let all = items(&Ctx { remote: false, listed: true, panel_open: false });
    assert_eq!(labels(&all), ["Open terminal", "New folder"], "the owner's order: the terminal first");
    assert_eq!(all.iter().map(|i| i.kind).collect::<Vec<_>>(), [Kind::OpenTerminal, Kind::NewFolder]);
    assert!(all.iter().all(|i| i.enabled() && i.reason.is_none()));
}

#[test]
fn an_ssh_host_greys_every_line_with_its_reason_and_no_listed_folder_greys_them_too() {
    let remote = items(&Ctx { remote: true, listed: true, panel_open: false });
    assert_eq!(labels(&remote), ["Open terminal", "New folder"], "the lines stay in their place");
    assert!(remote.iter().all(|i| i.reason == Some("this machine only")));
    let none = items(&Ctx { remote: false, listed: false, panel_open: false });
    assert!(none.iter().all(|i| i.reason == Some("no folder here")));
    let both = items(&Ctx { remote: true, listed: false, panel_open: false });
    assert!(both.iter().all(|i| i.reason == Some("this machine only")), "the machine is the first thing said");
}

#[test]
fn an_open_screen_greys_new_folder_only() {
    let open = items(&Ctx { remote: false, listed: true, panel_open: true });
    assert!(open[0].enabled(), "a terminal can be opened over any screen");
    assert_eq!(open[1].reason, Some("close the open screen first"));
}

#[test]
fn the_selection_starts_at_the_first_usable_line_and_steps_over_the_greyed_ones_without_wrapping() {
    let two = items(&Ctx { remote: false, listed: true, panel_open: false });
    assert_eq!(first_enabled(&two), Some(0));
    assert_eq!((step(&two, 0, true), step(&two, 1, true), step(&two, 1, false), step(&two, 0, false)), (1, 1, 0, 0), "it stops at the ends");
    let over = items(&Ctx { remote: false, listed: true, panel_open: true });
    assert_eq!((step(&over, 0, true), step(&over, 0, false)), (0, 0), "New folder is greyed: Down stays on the terminal line");
    let none = items(&Ctx { remote: true, listed: true, panel_open: false });
    assert_eq!((first_enabled(&none), step(&none, 0, true)), (None, 0), "nothing to select");
    let mut only_second = two.clone();
    only_second[0].reason = Some("x");
    assert_eq!((first_enabled(&only_second), step(&only_second, 1, false)), (Some(1), 1), "the greyed first line is not stepped to");
}

#[test]
fn the_menu_is_placed_at_the_pointer_and_kept_inside_its_bounds() {
    let bounds = [0.0, 20.0, 300.0, 400.0];
    assert_eq!(place((50.0, 60.0), (100.0, 40.0), bounds), (50.0, 60.0), "inside: where the pointer is");
    assert_eq!(place((290.0, 60.0), (100.0, 40.0), bounds), (200.0, 60.0), "too far right: its right edge on the bound");
    assert_eq!(place((50.0, 395.0), (100.0, 40.0), bounds), (50.0, 360.0), "too far down: its bottom on the bound");
    assert_eq!(place((-5.0, 5.0), (100.0, 40.0), bounds), (0.0, 20.0), "outside the top left: the corner");
    assert_eq!(place((10.0, 10.0), (500.0, 40.0), bounds), (0.0, 20.0), "wider than the bounds: from the left edge");
}
