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

/// A row of the list under the pointer, nothing open, a folder listed, on this machine.
fn on_a_row() -> Ctx {
    Ctx { remote: false, listed: true, panel_open: false, on_row: true, protected: false }
}

#[test]
fn the_target_of_a_row_knows_the_folder_the_list_shows_and_so_its_own_path() {
    let on_folder = target_for("/work", Some(("src", "dir")));
    assert_eq!((on_folder.parent.as_str(), on_folder.folder.as_str()), ("/work", "/work/src"));
    assert_eq!(on_folder.entry(), Some("/work/src".to_string()));
    let on_file = target_for("/work/", Some(("a.txt", "file")));
    assert_eq!((on_file.parent.as_str(), on_file.folder.as_str()), ("/work/", "/work/"));
    assert_eq!(on_file.entry(), Some("/work/a.txt".to_string()), "a closing slash makes no double slash");
    let on_link = target_for("/work", Some(("l", "link")));
    assert_eq!(on_link.entry(), Some("/work/l".to_string()));
    let on_space = target_for("/work", None);
    assert_eq!((on_space.parent.as_str(), on_space.entry()), ("/work", None), "the empty space has no entry");
}

#[test]
fn the_menu_holds_the_owners_six_lines_in_his_order_as_far_as_they_are_built() {
    let all = items(&on_a_row());
    assert_eq!(labels(&all), ["Open terminal", "New folder", "Rename", "Delete"], "the owner's order: the terminal first, then the folder, the name, the delete");
    assert_eq!(all.iter().map(|i| i.kind).collect::<Vec<_>>(), [Kind::OpenTerminal, Kind::NewFolder, Kind::Rename, Kind::Delete]);
    assert!(all.iter().all(|i| i.enabled() && i.reason.is_none()));
}

#[test]
fn the_empty_space_greys_rename_and_delete_and_a_protected_row_greys_them_with_another_reason() {
    let space = items(&Ctx { on_row: false, ..on_a_row() });
    assert!(space[0].enabled() && space[1].enabled(), "the terminal and the new folder do not need a row");
    assert_eq!((space[2].reason, space[3].reason), (Some("point at a file or folder"), Some("point at a file or folder")));
    let home = items(&Ctx { protected: true, ..on_a_row() });
    assert!(home[0].enabled() && home[1].enabled(), "a protected folder may still be entered and filled");
    assert_eq!((home[2].reason, home[3].reason), (Some("protected"), Some("protected")));
    let both = items(&Ctx { on_row: false, protected: true, ..on_a_row() });
    assert_eq!(both[2].reason, Some("point at a file or folder"), "no row is said before a protected one");
}

#[test]
fn an_ssh_host_greys_every_line_with_its_reason_and_no_listed_folder_greys_them_too() {
    let remote = items(&Ctx { remote: true, ..on_a_row() });
    assert_eq!(labels(&remote), ["Open terminal", "New folder", "Rename", "Delete"], "the lines stay in their place");
    assert!(remote.iter().all(|i| i.reason == Some("this machine only")));
    let none = items(&Ctx { listed: false, ..on_a_row() });
    assert!(none.iter().all(|i| i.reason == Some("no folder here")));
    let both = items(&Ctx { remote: true, listed: false, ..on_a_row() });
    assert!(both.iter().all(|i| i.reason == Some("this machine only")), "the machine is the first thing said");
    let first = items(&Ctx { remote: true, on_row: false, protected: true, panel_open: true, listed: true });
    assert!(first.iter().all(|i| i.reason == Some("this machine only")), "the machine before the screen, the row and the protection");
}

#[test]
fn an_open_screen_greys_every_line_that_asks_a_question_but_not_the_terminal() {
    let open = items(&Ctx { panel_open: true, ..on_a_row() });
    assert!(open[0].enabled(), "a terminal can be opened over any screen");
    for line in &open[1..] {
        assert_eq!(line.reason, Some("close the open screen first"), "{}", line.label);
    }
    let over_space = items(&Ctx { panel_open: true, on_row: false, ..on_a_row() });
    assert_eq!(over_space[2].reason, Some("close the open screen first"), "the screen is said before the missing row");
}

#[test]
fn the_selection_starts_at_the_first_usable_line_and_steps_over_the_greyed_ones_without_wrapping() {
    let four = items(&on_a_row());
    assert_eq!(first_enabled(&four), Some(0));
    assert_eq!((step(&four, 0, true), step(&four, 3, true), step(&four, 3, false), step(&four, 0, false)), (1, 3, 2, 0), "it stops at the ends");
    let over = items(&Ctx { panel_open: true, ..on_a_row() });
    assert_eq!((step(&over, 0, true), step(&over, 0, false)), (0, 0), "the questions are greyed: Down stays on the terminal line");
    let space = items(&Ctx { on_row: false, ..on_a_row() });
    assert_eq!((step(&space, 1, true), step(&space, 1, false)), (1, 0), "the two greyed lines at the end are stepped over, and then it stays");
    let none = items(&Ctx { remote: true, ..on_a_row() });
    assert_eq!((first_enabled(&none), step(&none, 0, true)), (None, 0), "nothing to select");
    let mut only_second = four.clone();
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
