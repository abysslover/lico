//! The pure parts of the browser bar: where its parts are, what a point is on, and how far the text of the field is scrolled for the caret.

use super::*;

fn bar() -> Rect {
    [200.0, 40.0, 1000.0, 60.0]
}

#[test]
fn the_bar_is_three_square_buttons_in_a_row_and_then_the_field_to_the_right_edge() {
    let l = layout(bar(), 1.0);
    for (r, name) in [(l.back, "back"), (l.forward, "forward"), (l.reload, "reload")] {
        assert_eq!((r[2] - r[0], r[3] - r[1]), (20.0, 20.0), "{name} is as high as the bar and square: {r:?}");
        assert_eq!((r[1], r[3]), (40.0, 60.0), "{name} is on the bar's rows");
    }
    assert_eq!(l.back[0], 200.0, "the first button starts at the bar's left edge");
    assert!(l.forward[0] > l.back[2] && l.reload[0] > l.forward[2], "a gap between the buttons");
    assert_eq!(l.field[0] - l.reload[2], l.forward[0] - l.back[2], "the field is one gap right of the last button, the gap of the buttons");
    assert_eq!((l.field[2], l.field[1], l.field[3]), (1000.0, 40.0, 60.0), "to the bar's right edge, as high as the bar");
    let l2 = layout([200.0, 40.0, 1000.0, 80.0], 2.0);
    assert_eq!(l2.back[2] - l2.back[0], 40.0, "at scale 2 and a bar of 40 pixels the buttons are 40 wide");
    assert_eq!(l2.forward[0] - l2.back[2], 4.0, "and the gap is 4");
}

#[test]
fn a_bar_too_narrow_for_its_buttons_has_a_field_of_no_width_and_nothing_outside_it() {
    let l = layout([10.0, 0.0, 50.0, 20.0], 1.0);
    for r in [l.back, l.forward, l.reload, l.field] {
        assert!(r[0] <= r[2] && r[2] <= 50.0 && r[0] >= 10.0, "{r:?}");
    }
    assert_eq!(l.field[0], l.field[2], "no room: the field is empty");
}

#[test]
fn a_point_is_on_the_one_part_it_is_in_and_the_gaps_and_the_outside_are_nothing() {
    let l = layout(bar(), 1.0);
    let mid = |r: Rect| ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    for (r, want) in [(l.back, BarHit::Back), (l.forward, BarHit::Forward), (l.reload, BarHit::Reload), (l.field, BarHit::Field)] {
        let (x, y) = mid(r);
        assert_eq!(hit(&l, x, y), Some(want));
    }
    assert_eq!(hit(&l, l.back[2] + 0.5, 50.0), None, "the gap between the buttons");
    assert_eq!(hit(&l, 100.0, 50.0), None, "left of the bar");
    assert_eq!(hit(&l, 500.0, 39.0), None, "above the bar");
    assert_eq!(hit(&l, 500.0, 60.0), None, "below the bar (the page starts there)");
    assert_eq!(hit(&l, 1000.0, 50.0), None, "right of the bar");
    assert_eq!(hit(&l, l.back[0], 40.0), Some(BarHit::Back), "the top left corner is the button's");
}

#[test]
fn the_text_is_scrolled_only_as_far_as_the_caret_with_a_little_of_what_follows_needs() {
    // all of it fits: no scroll, wherever the caret is
    assert_eq!(scroll(0.0, 2.0, 50.0, 300.0, 400.0), 0.0);
    assert_eq!(scroll(298.0, 2.0, 50.0, 300.0, 400.0), 0.0);
    // longer than the field, the caret at the start: no scroll
    assert_eq!(scroll(0.0, 2.0, 50.0, 1000.0, 400.0), 0.0);
    // the caret in the middle: it and the `ahead` after it end at the right edge
    assert_eq!(scroll(600.0, 2.0, 50.0, 1000.0, 400.0), 600.0 + 2.0 + 50.0 - 400.0);
    // the caret at the end: nothing after it to keep in view, so the end of the text (and the caret) is the right edge
    assert_eq!(scroll(1000.0, 2.0, 50.0, 1000.0, 400.0), 1000.0 + 2.0 - 400.0);
    // the caret just inside what is in view: no scroll
    assert_eq!(scroll(300.0, 2.0, 50.0, 1000.0, 400.0), 0.0);
    // a caret that has moved left again scrolls back
    assert!(scroll(500.0, 2.0, 50.0, 1000.0, 400.0) < scroll(600.0, 2.0, 50.0, 1000.0, 400.0));
}
