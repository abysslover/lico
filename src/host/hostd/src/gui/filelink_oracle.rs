//! A name under the pointer and the scroll step (`M24-P11e`): the oracle of `scroll_oracle.rs` run with a link hovered. While a name is under the pointer the frame is a full repaint (as for a
//! selection), so a kept row can never carry the underline of a moment ago; the pixels must be those of a full repaint at every scale, while lines arrive and when the pointer leaves.

use super::*;
use crate::gui::ui::Row;

fn linked(n: i64) -> String {
    format!("notes.md {}", text(n))
}

#[test]
fn a_hovered_name_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_across_scales() {
    let dir = std::env::temp_dir().join(format!("lico_filelink_oracle_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    std::fs::write(dir.join("notes.md"), "x").expect("a file");
    for scale in [1.0f32, 1.25, 2.0] {
        let size = if scale == 1.0 { (1000, 620) } else if scale == 2.0 { (1500, 1440) } else { (1100, 800) };
        let mut p = Pair::new(size, scale);
        let row = Row { session: "session-s0".into(), provider: "claude".into(), state: "running".into(), activity: "idle".into(), cwd: dir.to_string_lossy().into(), ..Default::default() };
        p.both(|app| app.ui.rows = vec![row.clone()]);
        p.full("the first frame", Cause::First);
        let texts: Vec<String> = (0..60).map(linked).collect();
        p.push(&texts);
        p.full("sixty lines", Cause::NothingInCommon);
        // the pointer is over the first columns of a row of the middle of the screen, where every line begins with the name
        let ox = p.on.app.ui.links.origin.0;
        let mid = p.on.app.ui.hit_lines[p.on.app.ui.hit_lines.len() / 2];
        let at = ((ox + 3.0 * 8.0 * scale) as f64, (mid.0 + 2.0 * scale) as f64);
        p.both(|app| app.ui.mouse = at);
        let cause = p.step("the pointer is over a name");
        assert!(p.on.app.ui.links.hover.is_some() && p.off.app.ui.links.hover.is_some(), "a name is under the pointer (scale {scale})");
        assert_eq!(cause, Cause::Selection, "a hovered name is an overlay of the scroll step");
        let mut hovered = 1;
        let mut next = 60;
        for burst in [1, 1, 3, 1, 12, 2, 1] {
            p.push(&(next..next + burst).map(linked).collect::<Vec<_>>());
            next += burst;
            let cause = p.step(&format!("a burst of {burst} lines with the pointer in place"));
            if p.on.app.ui.links.hover.is_some() {
                hovered += 1;
                assert_ne!(cause, Cause::Shift, "no kept rows while a name is underlined (scale {scale}, burst {burst})");
            }
        }
        assert!(hovered >= 4, "the pointer stayed over names in most frames ({hovered}) at scale {scale}");
        // the pointer leaves: the underline is gone from every row, and the next burst is the cheap step again
        p.both(|app| app.ui.mouse = (3.0, 3.0));
        p.step("the pointer leaves");
        assert!(p.on.app.ui.links.hover.is_none());
        p.push(&[linked(next)]);
        p.shift("a line after the pointer has left");
    }
    let _ = std::fs::remove_dir_all(dir);
}
