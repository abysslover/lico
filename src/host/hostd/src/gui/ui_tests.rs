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

    let btns = app.toolbar_entries();
    assert!(btns.iter().any(|b| b.id == "send_to" && b.label == "Send -> Session (1)" && b.reason == Some("nothing to send: type in the input box or select lines")));
    assert!(!btns.iter().any(|b| b.id == "copy"));

    app.ui.sel = Some((0, 0));
    let btns_sel = app.toolbar_entries();
    assert!(btns_sel.iter().any(|b| b.id == "send_to" && b.label == "Send -> Session (1)" && b.reason.is_none()));
    assert!(btns_sel.iter().any(|b| b.id == "copy" && b.label == "Copy" && b.reason.is_none()));

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
