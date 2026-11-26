use super::super::{fonts, BACKGROUND};
use super::*;

/// The blit as it was before the rows were blended directly: `Buffer::draw` and a closure per pixel.
fn reference(buffer: &mut Buffer, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, ox: i32, oy: i32, clip_x: i32, ink: Color) {
    buffer.draw(fs, swash, ink, |x, y, gw, gh, color| {
        let alpha = color.a() as u32;
        for row in 0..gh as i32 {
            for col in 0..gw as i32 {
                let (px, py) = (x + col + ox, y + row + oy);
                if px >= 0 && py >= 0 && px < clip_x && (px as usize) < width && (py as usize) < height {
                    let idx = py as usize * width + px as usize;
                    let dst = buf[idx];
                    let mix = |s: u8, shift: u32| {
                        let d = (dst >> shift) & 0xFF;
                        (d + ((s as u32).wrapping_sub(d) as i32 * alpha as i32 / 255) as u32) & 0xFF
                    };
                    buf[idx] = 0xFF00_0000 | mix(color.r(), 16) << 16 | mix(color.g(), 8) << 8 | mix(color.b(), 0);
                }
            }
        }
    });
}

#[test]
fn the_row_blit_draws_the_same_pixels_as_the_callback_blit() {
    let mut fs = fonts::system(false);
    let mut faces = fonts::Faces::new(false);
    let mut buffer = Buffer::new(&mut fs, Metrics::new(15.0, 18.0));
    buffer.set_size(Some(380.0), Some(150.0));
    buffer.lines = [
        "Hello, world fn main() { let x = 42; }",
        "\u{d30c}\u{c77c}\u{c744} \u{c77d}\u{c2b5}\u{b2c8}\u{b2e4} \u{c548}\u{b155}\u{d558}\u{c138}\u{c694} \u{6f22}\u{5b57} \u{65e5}\u{672c}\u{8a9e} \u{05e9}\u{05dc}\u{05d5}\u{05dd}",
        "\u{1f642} e\u{301} \u{2192} \u{2713} mixed \u{d30c}\u{c77c}.rs",
        "a line that is long enough to wrap around the edge of the window, twice over, so that it does",
    ]
    .iter()
    .map(|t| BufferLine::new(*t, LineEnding::None, faces.line(&mut fs, t), Shaping::Advanced))
    .collect();
    buffer.shape_until_scroll(&mut fs, true);
    let (width, height) = (400usize, 160usize);
    let mut swash = SwashCache::new();
    for (ox, oy, clip) in [(10, 5, i32::MAX), (-7, 3, 150), (20, -9, 400)] {
        let (mut a, mut b) = (vec![BACKGROUND; width * height], vec![BACKGROUND; width * height]);
        reference(&mut buffer, &mut fs, &mut swash, &mut a, width, height, ox, oy, clip, Color::rgb(0xE0, 0xE0, 0xE0));
        blit(&mut buffer, &mut fs, &mut swash, &mut b, width, height, ox, oy, clip, Color::rgb(0xE0, 0xE0, 0xE0));
        assert!(a.iter().filter(|p| **p != BACKGROUND).count() > 2000, "the picture must hold text");
        assert!(a == b, "the pixels differ for offset ({ox}, {oy}) clip {clip}");
    }
}

#[test]
fn hangul_ime_logical_keys_fall_back_to_physical_keycode_for_shortcuts() {
    use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
    assert_eq!(super::super::key_letter_parts(&Key::Character("ㅊ".into()), &PhysicalKey::Code(KeyCode::KeyC)), Some('c'));
    assert_eq!(super::super::key_letter_parts(&Key::Character("ㅍ".into()), &PhysicalKey::Code(KeyCode::KeyV)), Some('v'));
    assert_eq!(super::super::key_letter_parts(&Key::Character("ㄴ".into()), &PhysicalKey::Code(KeyCode::KeyS)), Some('s'));
    assert_eq!(super::super::key_letter_parts(&Key::Character("ㅁ".into()), &PhysicalKey::Code(KeyCode::KeyA)), Some('a'));
    // Remapped ASCII layouts (e.g. Dvorak) keep their logical ASCII letter
    assert_eq!(super::super::key_letter_parts(&Key::Character("j".into()), &PhysicalKey::Code(KeyCode::KeyC)), Some('j'));
    assert_eq!(super::super::key_letter_parts(&Key::Named(NamedKey::Enter), &PhysicalKey::Code(KeyCode::Enter)), None);
}

#[test]
fn local_exit_and_quit_slash_commands_dismiss_tab_without_sending_input_and_toolbar_surfaces_send_to_and_copy() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    let scratch = crate::handoff::Scratch::new("exit_tab");
    let sock_path = scratch.0.join("host.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();
    let handle = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut req = String::new();
        BufReader::new(&mut s).read_line(&mut req).unwrap();
        s.write_all(b"{\"v\":1,\"ok\":1,\"session\":\"s111111\",\"state\":\"disconnected\"}\n").unwrap();
        req
    });

    let (sock, mut peer) = UnixStream::pair().unwrap();
    peer.set_nonblocking(true).unwrap();
    let mut app = super::super::scroll_oracle::app(1.0, false);
    app.sock = sock;
    app.socket = sock_path.to_string_lossy().into_owned();
    app.session = "s111111".into();
    app.generation = 3;
    app.ui.rows = vec![
        Row { session: "s111111".into(), provider: "codex".into(), state: "running".into(), generation: 3, ..Default::default() },
        Row { session: "s222222".into(), provider: "claude".into(), state: "running".into(), generation: 1, ..Default::default() },
    ];
    app.refresh_list();
    app.push_line(0, "hello world".into());

    // the toolbar is the account icon and the three others (and Copy while lines are selected); Send to session is the Session menu's entry (M22-P04)
    let send = |app: &super::super::App| app.menu_entries(1).into_iter().find(|e| e.cmd == super::super::menu::Cmd::SendTo).map(|e| e.reason);
    let btns = app.toolbar_entries();
    assert_eq!(btns.iter().map(|b| b.id).collect::<Vec<_>>(), ["accounts", "new_bot", "new_session", "new_browser", "theme"]);
    assert_eq!(btns.iter().map(|b| b.label.as_str()).skip(1).take(2).collect::<Vec<_>>(), ["New Bot session", "New session"], "M25-P10: the Bot session comes before the session and has the owner's name for it");
    assert_eq!(send(&app), Some(Some("nothing to send: type in the input box or select lines")));

    app.ui.sel = Some((0, 0));
    let btns_sel = app.toolbar_entries();
    assert!(btns_sel.iter().any(|b| b.id == "copy" && b.label == "Copy" && b.reason.is_none() && b.icon.is_none()));
    assert_eq!(send(&app), Some(None));

    app.ed.set("/exit".into());
    app.send_draft();
    let one_shot_req = handle.join().unwrap();
    assert_eq!(app.ed.text(), "");
    assert_eq!(app.pending_show, Some(("s222222".into(), None)), "closing s111111 switches cleanly to adjacent open tab s222222");
    assert!(app.ui.rows.iter().find(|r| r.session == "s111111").unwrap().tab_closed);
    assert!(one_shot_req.contains("\"op\":\"session.close\"") && one_shot_req.contains("\"hide_tab\":true"), "req={one_shot_req}");
    let mut stream_sent = String::new();
    let _ = peer.read_to_string(&mut stream_sent);
    assert!(!stream_sent.contains("\"op\":\"input\""), "/exit must never be sent to the LLM: {stream_sent}");
}

#[test]
fn the_left_column_starts_at_the_top_of_the_window_and_only_without_the_menu_bar_under_what_is_on_top() {
    let mut app = super::super::scroll_oracle::app(1.0, false);
    assert!(app.ui.menu);
    assert_eq!(app.left_column_tops(70.0, 1.0), (0.0, 4.0), "with the menu bar the column still starts at the very top (the bar is to its right)");
    assert_eq!(app.left_column_tops(70.0, 2.0), (0.0, 8.0));
    app.ui.markers = true;
    assert_eq!(app.left_column_tops(70.0, 1.0), (16.0, 20.0), "the measurement client keeps its two squares in the corner");
    app.ui.markers = false;
    app.ui.menu = false;
    assert_eq!(app.left_column_tops(30.0, 1.0), (30.0, 40.0), "no menu bar: under whatever is on top");
}

#[test]
fn the_account_icon_comes_first_is_lit_only_when_the_host_says_there_is_no_account_and_never_greys_the_plus_or_the_bot() {
    use serde_json::json;
    let mut app = super::super::scroll_oracle::app(1.0, false);
    let entry = |app: &super::super::App, id: &str| app.toolbar_entries().into_iter().find(|b| b.id == id).map(|b| (b.active, b.reason, b.icon.map(super::icons::Icon::name)));
    assert_eq!(app.toolbar_entries()[0].id, "accounts");
    assert_eq!(entry(&app, "accounts"), Some((false, None, Some("account"))), "before the host has answered nothing is known, so nothing is lit");
    app.sync_accounts_value(&json!({"ok": 1, "accounts": []}));
    assert_eq!(entry(&app, "accounts"), Some((true, None, Some("account"))), "no account at all: the first step is lit");
    assert_eq!(entry(&app, "new_session"), Some((false, None, Some("plus"))), "Local LLM needs no account, so the plus is never greyed");
    assert_eq!(entry(&app, "new_bot"), Some((false, None, Some("bot"))));
    app.sync_accounts_value(&json!({"ok": 1, "accounts": [{"account": "a1", "provider": "codex", "label": "Lab"}]}));
    assert_eq!(entry(&app, "accounts"), Some((false, None, Some("account"))), "an account of any provider puts the light out");
    assert_eq!(app.ui.account_kinds.as_deref(), Some(&["codex".to_string()][..]));
    let word = |app: &super::super::App| app.toolbar_entries()[0].label.clone();
    assert_eq!(word(&app), "Accounts");
    app.sync_accounts_value(&json!({"ok": 1, "accounts": []}));
    assert!(word(&app).contains("add one"), "the word beside the icon says what to do: {}", word(&app));
}

#[test]
fn the_toolbar_tightens_its_spacing_before_it_hides_a_button() {
    use super::toolbar::toolbar_spacing;
    // four icons of 25 px and six text buttons (the Files panel's) of 636 px of text: 100 + 636 + 6 x 14 + 9 x 5 = 865 px at the usual spacing
    let widths = [25.0, 25.0, 25.0, 25.0, 156.0, 120.0, 84.0, 132.0, 72.0, 72.0];
    assert_eq!(toolbar_spacing(&widths, 6, 865.0, 1.0), (14.0, 5.0), "it fits: the usual spacing");
    assert_eq!(toolbar_spacing(&widths, 6, 864.0, 1.0), (6.0, 3.0), "one pixel short: tightened");
    assert_eq!(toolbar_spacing(&widths, 6, 300.0, 1.0), (6.0, 3.0), "far too short: tightened as far as it goes (the last buttons are then left out, as before)");
    assert_eq!(toolbar_spacing(&widths, 6, 994.0, 2.0), (14.0, 5.0), "the scale multiplies the padding and the gap, not the widths: 736 + 6 x 28 + 9 x 10");
    assert_eq!(toolbar_spacing(&widths, 6, 993.0, 2.0), (6.0, 3.0));
    assert_eq!(toolbar_spacing(&[], 0, 0.0, 1.0), (14.0, 5.0), "no buttons: nothing to tighten");
}

#[test]
fn the_toolbar_icons_are_twice_the_old_size_and_the_row_grows_with_them() {
    use super::toolbar::bar_heights;
    for (s, line_h) in [(1.0f32, 17.0f32), (1.5, 17.0), (2.0, 21.0), (1.0, 33.0)] {
        let old_side = line_h * s + s; // M22-P04: one line and a pixel
        let (menu_h, bar_h) = bar_heights(true, s, line_h);
        assert_eq!(menu_h, line_h * s + 6.0 * s, "the menu bar is as high as the left column's tabs (`M24-P11g`: it was a line and 4 s)");
        // the icon sits 3 s below the menu bar and 3 s above the bottom of the row (`draw_top_bars`, `M24-P11g`: it was 2 s and 3 s)
        let side = (bar_h - 3.0 * s) - (menu_h + 3.0 * s);
        assert!((side - 2.0 * old_side).abs() < 0.01, "twice the old side at scale {s}, line {line_h}: {side} against {old_side}");
    }
    assert_eq!(bar_heights(false, 1.0, 17.0), (0.0, 0.0), "no menu, no toolbar");
    assert_eq!(bar_heights(true, 0.2, 17.0).0, bar_heights(true, 1.0, 17.0).0, "a scale under one counts as one, as before");
}

#[test]
fn a_text_button_keeps_its_height_and_is_centred_in_the_taller_row() {
    use super::toolbar::button_rect;
    // the row starts at 30 and is 36 high (an icon of 36); a text button is 18 high
    assert_eq!(button_rect(true, 10.0, 36.0, 30.0, 36.0, 18.0), [10.0, 30.0, 46.0, 66.0], "an icon fills the row");
    assert_eq!(button_rect(true, 10.0, 24.0, 30.0, 36.0, 18.0), [10.0, 36.0, 34.0, 60.0], "a narrower icon is a square, centred in the row");
    assert_eq!(button_rect(true, 10.0, 50.0, 30.0, 36.0, 18.0), [10.0, 30.0, 60.0, 66.0], "an icon is never taller than the row");
    assert_eq!(button_rect(false, 60.0, 80.0, 30.0, 36.0, 18.0), [60.0, 39.0, 140.0, 57.0], "a text button: its own height, 9 px of room above and below");
    assert_eq!(button_rect(false, 0.0, 10.0, 0.0, 12.0, 18.0), [0.0, 0.0, 10.0, 12.0], "never taller than the row");
}

#[test]
fn the_icons_shrink_before_the_last_buttons_of_the_row_are_pushed_out() {
    use super::toolbar::icon_fit;
    // four icons of 36 (the old side 18), six text buttons of 636 px of text in all, scale 1: at the tightest spacing the row needs 4 x 36 + 636 + 6 x 6 + 9 x 3 = 843
    assert_eq!(icon_fit(36.0, 18.0, 4, 636.0, 6, 843.0, 1.0), 36.0, "it fits at the tightest spacing: the full size (the spacing is what gives)");
    assert_eq!(icon_fit(36.0, 18.0, 4, 636.0, 6, 842.0, 1.0), 35.0, "four pixels short in all: each icon one pixel less (rounded down)");
    assert_eq!(icon_fit(36.0, 18.0, 4, 636.0, 6, 800.0, 1.0), 25.0, "(800 - 699) / 4 = 25.25");
    assert_eq!(icon_fit(36.0, 18.0, 4, 636.0, 6, 700.0, 1.0), 18.0, "never below the old side: what does not fit is left out, as before");
    assert_eq!(icon_fit(36.0, 18.0, 4, 636.0, 6, 0.0, 1.0), 18.0);
    assert_eq!(icon_fit(36.0, 18.0, 4, 0.0, 0, 50.0, 1.0), 18.0, "the icons alone: 4 x 36 + 3 x 3 = 153 does not fit 50");
    assert_eq!(icon_fit(36.0, 18.0, 4, 0.0, 0, 153.0, 1.0), 36.0);
    assert_eq!(icon_fit(36.0, 18.0, 0, 100.0, 1, 10.0, 1.0), 36.0, "no icon: nothing to shrink");
    assert_eq!(icon_fit(72.0, 36.0, 4, 636.0, 6, 1000.0, 2.0), 59.0, "the scale doubles the padding and the gap, not the texts: 636 + 72 + 54 = 762 and (1000 - 762) / 4 = 59.5");
}
