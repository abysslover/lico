//! The top rows of the window (`M24-P11g`): the menu bar and the tabs of the left column are one row high, and the row of icons is as high as the two rows under the tabs at the left, so that the lines across the window are level.

use super::*;
use crate::gui::scroll_oracle::app;
use crate::gui::sidebar_chats::calc_chats_ps;

const TAB_LINE: u32 = 0xFF2B_3646;
const BLOCK_LINE: u32 = 0xFF2B_3440;
const BAND: u32 = 0xFF16_1D27;
const W: usize = 1400;
const H: usize = 900;

#[test]
fn the_rows_are_the_text_button_and_the_icon_side_with_the_gaps_between() {
    for s in [1.0f32, 1.25, 1.5, 2.0, 3.0] {
        let lh = 18.0 * s;
        let rows = top_rows(s, lh);
        assert_eq!(rows.first, lh + 6.0 * s, "the tabs' row and the menu bar: a line of text and 3 units above and below");
        assert_eq!(rows.row, lh + s, "a row of the second block is the height of a text button");
        assert_eq!(rows.gap, 2.0 * s);
        assert_eq!(rows.second, 2.0 * rows.row + 3.0 * rows.gap, "two rows and a gap above, between and below");
        assert_eq!(rows.second, icon_side(s, 18.0) + 6.0 * s, "the icon's side, and 3 units above and below it");
    }
    assert_eq!(bar_heights(true, 1.0, 18.0), (24.0, 68.0));
    assert_eq!(bar_heights(true, 2.0, 18.0), (48.0, 136.0));
    assert_eq!(bar_heights(false, 1.0, 18.0), (0.0, 0.0), "no menu, no bars");
    assert_eq!(bar_heights(true, 0.5, 18.0), bar_heights(true, 1.0, 18.0), "a scale under 1 is 1");
}

#[test]
fn the_chats_list_starts_under_the_second_block_and_the_page_holds_the_cards_that_fit() {
    // 24 is the tabs' bottom at scale 1, the second block is 44 high and the list starts 2 under it (70); a card is 40 high; the page footer takes 2 lines and 4 more
    assert_eq!(calc_chats_ps(24.0, 310.0, 18.0, 1.0, false), 5, "200 pixels hold five cards");
    assert_eq!(calc_chats_ps(24.0, 309.0, 18.0, 1.0, false), 4, "199 do not");
    assert_eq!(calc_chats_ps(24.0, 333.0, 18.0, 1.0, true), 5, "an ssh host's folder row takes a line and 5 more");
    assert_eq!(calc_chats_ps(24.0, 332.0, 18.0, 1.0, true), 4);
    assert_eq!(calc_chats_ps(48.0, 620.0, 36.0, 2.0, false), 5, "at scale 2: the tabs end at 48, the list at 48 + 88 + 4 = 140, the footer takes 76, a card is 80 high: 400 pixels");
    assert_eq!(calc_chats_ps(48.0, 619.0, 36.0, 2.0, false), 4);
}

/// A window of `W` x `H` at `scale` with the sidebar on tab `tab`, drawn once.
fn drawn(scale: f32, tab: u8) -> (App, Vec<u32>) {
    let mut app = app(scale, false);
    app.ui.sidebar.tab = tab;
    let mut buf = vec![0u32; W * H];
    assert!(app.compose(&mut buf, W, H, 0).is_some(), "nothing was drawn");
    (app, buf)
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: {a} against {b}");
}

#[test]
fn the_lines_across_the_top_of_the_window_are_level_at_every_scale_in_both_tabs() {
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        for tab in [0u8, 1] {
            let (app, buf) = drawn(scale, tab);
            let at = format!("scale {scale}, tab {tab}");
            let lh = app.line_h() * scale;
            let rows = top_rows(scale, lh);
            let sb = &app.ui.sidebar.geom;
            let menu = app.ui.menu_geom.titles.first().copied().unwrap_or_else(|| panic!("a menu title ({at})"));
            let icon = app.ui.toolbar_geom.rects.first().copied().unwrap_or_else(|| panic!("an icon ({at})"));
            let dir_tab = sb.dir_tab_rect.unwrap_or_else(|| panic!("the Directories tab ({at})"));
            // the first row: the menu bar and the tabs end on one line
            near(menu[3], rows.first, &format!("the menu bar's bottom ({at})"));
            near(dir_tab[3] + 1.0, rows.first, &format!("the tabs' bottom ({at})"));
            // the two rows at the left
            let (first, second) = if tab == 0 { (sb.back_rect, sb.host_rect) } else { (sb.host_rect, sb.filter_rect) };
            let (first, second) = (first.unwrap_or_else(|| panic!("the first row ({at})")), second.unwrap_or_else(|| panic!("the second row ({at})")));
            near(first[1], rows.first + rows.gap, &format!("the first row's top ({at})"));
            near(first[3] - first[1], rows.row, &format!("the first row's height ({at})"));
            near(second[1], first[3] + rows.gap, &format!("the second row's top ({at})"));
            near(second[3] - second[1], rows.row, &format!("the second row's height ({at})"));
            let left_bottom = second[3] + rows.gap;
            // the icons: the same block at the right, the icon centred in it
            near(icon[1], rows.first + 3.0 * scale, &format!("the icon's top ({at})"));
            near(icon[3] - icon[1], icon_side(scale, app.line_h()), &format!("the icon's side ({at})"));
            let right_bottom = icon[3] + 3.0 * scale;
            near(left_bottom, right_bottom, &format!("the second block's bottom, left against right ({at})"));
            near(right_bottom, bar_heights(true, scale, app.line_h()).1, &format!("the bars' height ({at})"));
            near(right_bottom, rows.first + rows.second, &format!("the block ({at})"));
            // the two lines are drawn, one pixel each, from the left edge to the right edge
            let list_w = app.effective_list_w(scale);
            let (x_left, x_right) = (5usize, list_w + 300);
            for (y, colour, what) in [(rows.first as usize - 1, TAB_LINE, "under the tabs and the menu bar"), ((rows.first + rows.second) as usize - 1, BLOCK_LINE, "under the second block")] {
                for x in [x_left, x_right] {
                    assert_eq!(buf[y * W + x], colour, "the line {what} at row {y}, column {x} ({at})");
                }
            }
            // the band of the second block at the left starts on the tabs' line and reaches the block's own line: the rows stand on its tone, the gaps too
            for (y, what) in [(rows.first as usize, "the first row of the band, under the tabs"), ((rows.first + rows.gap + rows.row) as usize, "the gap between the two rows"), ((rows.first + rows.second) as usize - 2, "the last row of the band, over its line")] {
                assert_eq!(buf[y * W + 1], BAND, "{what} at row {y} ({at})");
            }
        }
    }
}

#[test]
fn the_rows_under_the_second_block_start_below_its_line() {
    for scale in [1.0f32, 2.0] {
        let (app, _) = drawn(scale, 0);
        let rows = top_rows(scale, app.line_h() * scale);
        let sb = &app.ui.sidebar.geom;
        let filter = sb.filter_rect.expect("the filter row");
        near(filter[1], rows.first + rows.second + 2.0 * scale, &format!("the filter row of the Directories tab is 2 units under the block's line (scale {scale})"));
    }
}

#[test]
fn the_first_chat_card_starts_two_units_under_the_second_block() {
    use crate::gui::sidebar_chats::ChatEntry;
    for scale in [1.0f32, 2.0] {
        let mut app = app(scale, false);
        app.ui.sidebar.tab = 1;
        app.ui.sidebar.chats.items = vec![ChatEntry {
            provider: "claude".into(), provider_session: "p1".into(), account: None, account_label: None, ssh_host: None, cwd: None, global: true, title: "A question".into(),
            updated_at: 1_795_000_000, open_session: None, open_state: None, running_elsewhere: false,
        }];
        app.ui.sidebar.chats.total = 1;
        let mut buf = vec![0u32; W * H];
        assert!(app.compose(&mut buf, W, H, 0).is_some());
        let rows = top_rows(scale, app.line_h() * scale);
        let card = app.ui.sidebar.chats.entries_geom.first().unwrap_or_else(|| panic!("a card was drawn (scale {scale})")).rect;
        near(card[1], rows.first + rows.second + 2.0 * scale, &format!("the first card's top (scale {scale})"));
    }
}
