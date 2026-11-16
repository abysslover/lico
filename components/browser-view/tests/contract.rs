//! The contract tests of `lico-browser-view` (SPEC.md): the model is driven with intents and engine events and answers with effects and data; nothing here opens a socket.

use lico_browser_view::*;

fn png(w: u32, h: u32, shade: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = lico_png::PngEncoder::new(&mut out, w, h).unwrap();
    for y in 0..h {
        let row: Vec<u8> = (0..w).flat_map(|x| [shade, (x % 256) as u8, (y % 256) as u8]).collect();
        enc.push_row(&row).unwrap();
    }
    enc.finish().unwrap();
    out
}

fn ready() -> Tab {
    let mut t = Tab::new();
    t.engine_event(EngineEvent::Started);
    t
}

fn shown(area: (u32, u32)) -> Tab {
    let mut t = ready();
    t.show(area, 0);
    t
}

// ---- constants -------------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_constants_are_named_with_the_values_the_spec_gives() {
    assert_eq!((MAX_HISTORY, MAX_URL, MAX_TITLE, MAX_FRAME_BYTES, FALLBACK_AFTER_MS, MAX_INSERT, MAX_NOTICE), (100, 2048, 200, 4 * 1024 * 1024, 500, 4096, 300));
    assert_eq!((MAX_VIEWPORT_W, MAX_VIEWPORT_H, MIN_VIEWPORT), (1920, 1200, 64));
    assert!(MAX_VIEWPORT_W as usize * MAX_VIEWPORT_H as usize * 4 <= 10 << 20, "one decoded frame is at most 9.2 MB");
}

// ---- the address -----------------------------------------------------------------------------------------------------------------------------------

#[test]
fn what_a_person_types_becomes_an_http_or_https_address() {
    for (typed, want) in [
        ("example.com", "https://example.com"),
        ("  example.com/a b ".trim().replace(' ', "%20").as_str(), "https://example.com/a%20b"),
        ("http://127.0.0.1:8080/a?b=c#d", "http://127.0.0.1:8080/a?b=c#d"),
        ("HTTPS://Example.com/Path", "https://Example.com/Path"),
        ("localhost:3000", "https://localhost:3000"),
        ("example.com:99/x", "https://example.com:99/x"),
        ("http://[::1]:9222/json", "http://[::1]:9222/json"),
        ("https://한글.example/경로?q=값", "https://한글.example/경로?q=값"),
        ("192.168.0.1", "https://192.168.0.1"),
    ] {
        assert_eq!(parse_address(typed).as_deref(), Ok(want), "{typed}");
    }
}

#[test]
fn everything_else_is_refused_with_its_reason() {
    for typed in ["", "   ", "\n"] {
        assert_eq!(parse_address(typed), Err(UrlError::Empty), "{typed:?}");
    }
    for typed in ["file:///etc/passwd", "javascript:alert(1)", "data:text/html,hi", "about:blank", "ftp://example.com/", "mailto:a@b.c", "view-source:http://x/", "chrome://settings", "blob:http://x/1", "ws://example.com/"] {
        assert_eq!(parse_address(typed), Err(UrlError::Scheme), "{typed}");
    }
    for typed in ["http://user:pw@host/", "https://u@host/", "http://@host/", "example.com@evil.example"] {
        assert_eq!(parse_address(typed), Err(UrlError::Credentials), "{typed}");
    }
    for typed in ["http://", "http://:80/", "http://host:99999/", "http://host:abc/", "http://ho st/", "http://host/\u{7}", "http://[::1/", "http://host\\path", "https:///x", "a b"] {
        assert_eq!(parse_address(typed), Err(UrlError::Invalid), "{typed:?}");
    }
    assert_eq!(parse_address(&format!("https://e.com/{}", "a".repeat(MAX_URL))), Err(UrlError::TooLong));
    assert!(parse_address(&format!("https://e.com/{}", "a".repeat(MAX_URL - 14))).is_ok(), "exactly the cap");
    assert_eq!(parse_address(&format!("https://e.com/{}", "a".repeat(MAX_URL - 13))), Err(UrlError::TooLong), "one over the cap");
    assert!(parse_address("http://host:65535/").is_ok());
    assert_eq!(parse_address("http://host:65536/"), Err(UrlError::Invalid), "one over the largest port");
    assert_eq!(parse_address("http://host:+80/"), Err(UrlError::Invalid), "a port is digits only");
    assert_eq!(parse_address("http://host:/"), Err(UrlError::Invalid));
}

#[test]
fn text_from_a_page_is_cleaned_before_it_is_shown() {
    assert_eq!(clean("a\u{0}b\u{7}c\nd\te", 100), "abcde");
    assert_eq!(clean("pay\u{202E}fdp.exe\u{202C}", 100), "payfdp.exe", "bidirectional overrides are removed");
    assert_eq!(clean("a\u{200B}b\u{FEFF}c\u{2066}d\u{2069}", 100), "abcd", "zero-width and isolate characters are removed");
    assert_eq!(clean("안녕하세요 world", 100), "안녕하세요 world");
    assert_eq!(clean(&"x".repeat(300), MAX_TITLE).chars().count(), MAX_TITLE);
    assert!(clean(&"x".repeat(300), MAX_TITLE).ends_with('…'));
    assert_eq!(clean("한".repeat(10).as_str(), 10), "한".repeat(10), "exactly the cap is kept whole");
}

#[test]
fn the_address_field_edits_by_characters_and_keeps_its_cursor_inside() {
    let mut f = AddressField::new();
    f.insert("안녕");
    f.insert("ab");
    assert_eq!((f.text(), f.cursor()), ("안녕ab", 4));
    f.left();
    f.left();
    f.insert("-");
    assert_eq!((f.text(), f.cursor()), ("안녕-ab", 3));
    f.backspace();
    assert_eq!((f.text(), f.cursor()), ("안녕ab", 2));
    f.delete();
    assert_eq!(f.text(), "안녕b");
    f.home();
    f.backspace();
    f.left();
    assert_eq!((f.text(), f.cursor()), ("안녕b", 0), "nothing before the start");
    f.end();
    f.delete();
    f.right();
    assert_eq!((f.text(), f.cursor()), ("안녕b", 3), "nothing after the end");
    f.insert("a\nb\u{7}c");
    assert_eq!(f.text(), "안녕bab" .to_string() + "c", "control characters and line breaks are dropped");
    f.set_text("new\u{0}text");
    assert_eq!((f.text(), f.cursor()), ("newtext", 7));
    f.set_text(&"y".repeat(MAX_URL + 50));
    assert_eq!(f.text().chars().count(), MAX_URL, "set_text holds at most the cap too");
    f.set_text("");
    f.insert(&"x".repeat(MAX_URL + 50));
    assert_eq!(f.text().chars().count(), MAX_URL, "the field holds at most the cap");
}

#[test]
fn a_pre_edit_is_shown_at_the_cursor_and_a_commit_replaces_it() {
    let mut f = AddressField::new();
    f.insert("go ");
    f.set_preedit("ㅎ");
    assert_eq!((f.text(), f.preedit(), f.display().as_str()), ("go ", "ㅎ", "go ㅎ"));
    f.set_preedit("하");
    assert_eq!(f.display(), "go 하");
    f.commit("한");
    assert_eq!((f.text(), f.preedit(), f.cursor()), ("go 한", "", 4));
    f.left();
    f.set_preedit("글");
    assert_eq!(f.display(), "go 글한");
    f.clear_preedit();
    assert_eq!(f.display(), "go 한");
    f.set_preedit("a\nb");
    assert_eq!(f.preedit(), "ab", "control characters never stay in a pre-edit");
}

// ---- navigation and history --------------------------------------------------------------------------------------------------------------------

#[test]
fn submitting_an_address_navigates_and_a_bad_one_does_nothing() {
    let mut t = shown((800, 600));
    assert_eq!(t.submit_address("example.com"), Ok(vec![Effect::Navigate("https://example.com".into())]));
    assert_eq!(t.phase(), &Phase::Loading);
    assert_eq!(t.submit_address("javascript:alert(1)"), Err(UrlError::Scheme));
    assert_eq!((t.history().len(), t.phase()), (1, &Phase::Loading), "a refused address changes nothing");
    assert_eq!(t.submit_address("http://a.example/x"), Ok(vec![Effect::Navigate("http://a.example/x".into())]));
    assert_eq!(t.history(), ["https://example.com", "http://a.example/x"]);
}

#[test]
fn back_and_forward_walk_the_history_and_a_new_address_drops_what_was_forward() {
    let mut t = shown((800, 600));
    for u in ["https://a.example", "https://b.example", "https://c.example"] {
        t.submit_address(u).unwrap();
        t.engine_event(EngineEvent::Navigated(u.into()));
    }
    assert!(t.can_back() && !t.can_forward());
    assert_eq!(t.back(), vec![Effect::Navigate("https://b.example".into())]);
    assert!(t.loading(), "going back is a load");
    t.engine_event(EngineEvent::LoadFinished);
    t.engine_event(EngineEvent::Navigated("https://b.example".into()));
    assert_eq!(t.history().len(), 3, "going back adds nothing");
    assert_eq!(t.back(), vec![Effect::Navigate("https://a.example".into())]);
    t.engine_event(EngineEvent::Navigated("https://a.example".into()));
    assert!(!t.can_back() && t.can_forward());
    assert_eq!(t.back(), vec![], "nothing before the first");
    assert_eq!(t.forward(), vec![Effect::Navigate("https://b.example".into())]);
    assert!(t.loading());
    t.engine_event(EngineEvent::Navigated("https://b.example".into()));
    t.submit_address("https://d.example").unwrap();
    t.engine_event(EngineEvent::Navigated("https://d.example".into()));
    assert_eq!(t.history(), ["https://a.example", "https://b.example", "https://d.example"], "c was forward of b: dropped");
    assert!(!t.can_forward());
    assert_eq!(t.forward(), vec![]);
}

#[test]
fn a_link_the_person_clicks_is_a_navigation_the_engine_reports_and_the_history_follows_it() {
    let mut t = shown((800, 600));
    t.submit_address("https://a.example").unwrap();
    t.engine_event(EngineEvent::Navigated("https://a.example".into()));
    t.engine_event(EngineEvent::Navigated("https://a.example/page2".into()));
    t.engine_event(EngineEvent::Navigated("https://a.example/page2".into()));
    assert_eq!(t.history(), ["https://a.example", "https://a.example/page2"], "the same address twice is one entry");
    assert_eq!(t.url(), "https://a.example/page2");
}

#[test]
fn the_history_holds_at_most_a_hundred_and_drops_the_oldest() {
    let mut t = shown((800, 600));
    for i in 0..130 {
        t.engine_event(EngineEvent::Navigated(format!("https://e.example/{i}")));
    }
    assert_eq!(t.history().len(), MAX_HISTORY);
    assert_eq!(t.history()[0], "https://e.example/30");
    assert_eq!(t.history()[99], "https://e.example/129");
    assert!(t.can_back());
    assert_eq!(t.back(), vec![Effect::Navigate("https://e.example/128".into())]);
}

#[test]
fn the_address_the_engine_reports_is_cleaned_and_does_not_overwrite_what_is_being_typed() {
    let mut t = shown((800, 600));
    t.engine_event(EngineEvent::Navigated("https://e.example/\u{202E}evil".into()));
    assert_eq!(t.url(), "https://e.example/evil");
    assert_eq!(t.address().text(), "https://e.example/evil", "shown in the field while the person is not typing");
    t.focus_address();
    t.address_mut().set_text("my half-typed");
    t.engine_event(EngineEvent::Navigated("https://e.example/next".into()));
    assert_eq!(t.address().text(), "my half-typed");
    assert_eq!(t.url(), "https://e.example/next");
    t.engine_event(EngineEvent::Title("A\u{7} title ".to_string() + &"x".repeat(300)));
    assert!(t.title().starts_with("A title") && t.title().chars().count() == MAX_TITLE);
}

#[test]
fn reload_is_an_effect_and_the_states_follow_the_engines_events() {
    let mut t = shown((800, 600));
    assert_eq!(t.reload(), vec![Effect::Reload]);
    t.engine_event(EngineEvent::LoadStarted);
    assert!(t.loading() && t.phase() == &Phase::Loading);
    t.engine_event(EngineEvent::LoadFinished);
    assert!(!t.loading() && t.phase() == &Phase::Ready);
    t.engine_event(EngineEvent::Crashed);
    assert!(matches!(t.phase(), Phase::Failed(m) if !m.is_empty()));
    let mut t = shown((800, 600));
    t.engine_event(EngineEvent::TargetGone);
    assert!(matches!(t.phase(), Phase::Failed(_)));
    let mut absent = Tab::new();
    absent.engine_event(EngineEvent::Missing { expected_sha256: "ab".repeat(32), release: "v0.2.6".into() });
    assert_eq!(absent.phase(), &Phase::Absent { expected_sha256: "ab".repeat(32), release: "v0.2.6".into() });
    assert_eq!(absent.submit_address("example.com"), Ok(vec![]), "nothing to navigate with an engine that is absent");
    assert!(!absent.no_font());
    absent.engine_event(EngineEvent::NoFontFound);
    assert!(absent.no_font());
}

#[test]
fn an_address_typed_before_the_engine_is_ready_is_navigated_when_it_is() {
    let mut t = Tab::new();
    assert_eq!(t.phase(), &Phase::Starting);
    assert_eq!(t.show((800, 600), 0), vec![], "nothing can be asked of an engine that has not started");
    assert_eq!(t.submit_address("example.com"), Ok(vec![]));
    let fx = t.engine_event(EngineEvent::Started);
    assert_eq!(fx, vec![Effect::SetViewport { width: 800, height: 600 }, Effect::StartScreencast, Effect::Navigate("https://example.com".into())]);
    assert!(t.phase() == &Phase::Loading || t.phase() == &Phase::Ready);
}

// ---- the viewport, show and hide -----------------------------------------------------------------------------------------------------------------

#[test]
fn showing_a_tab_sets_the_viewport_and_starts_the_screencast_and_hiding_stops_it() {
    let mut t = ready();
    assert_eq!(t.show((800, 600), 10), vec![Effect::SetViewport { width: 800, height: 600 }, Effect::StartScreencast]);
    assert!(t.shown());
    assert_eq!(t.viewport(), (800, 600));
    assert_eq!(t.hide(), vec![Effect::StopScreencast]);
    assert!(!t.shown());
    assert_eq!(t.hide(), vec![], "hiding a hidden tab does nothing");
}

#[test]
fn the_viewport_is_the_shown_area_within_the_cap_and_the_cap_is_said() {
    let mut t = ready();
    t.show((1000, 700), 0);
    assert!(!t.viewport_capped());
    assert_eq!(t.resize((1000, 700)), vec![], "the same size: nothing");
    assert_eq!(t.resize((1500, 900)), vec![Effect::SetViewport { width: 1500, height: 900 }]);
    assert_eq!(t.resize((3840, 2160)), vec![Effect::SetViewport { width: 1920, height: 1200 }]);
    assert!(t.viewport_capped());
    assert_eq!(t.resize((4000, 2000)), vec![], "still the capped size");
    assert_eq!(t.resize((10, 20)), vec![Effect::SetViewport { width: 64, height: 64 }]);
    assert!(!t.viewport_capped());
    t.hide();
    assert_eq!(t.resize((900, 500)), vec![], "a hidden tab is not resized");
    assert_eq!(t.show((900, 500), 5), vec![Effect::SetViewport { width: 900, height: 500 }, Effect::StartScreencast]);
}

// ---- the frames -------------------------------------------------------------------------------------------------------------------------------------

#[test]
fn every_frame_is_acknowledged_at_once_and_only_the_newest_is_kept() {
    let mut t = shown((64, 64));
    assert_eq!(t.engine_event(EngineEvent::Frame { id: 1, png: png(64, 64, 10) }), vec![Effect::Ack(1)]);
    assert_eq!(t.engine_event(EngineEvent::Frame { id: 2, png: png(64, 64, 20) }), vec![Effect::Ack(2)]);
    assert_eq!(t.engine_event(EngineEvent::Frame { id: 3, png: png(64, 64, 30) }), vec![Effect::Ack(3)]);
    assert_eq!(t.dropped_frames(), 2, "two frames were replaced before they were painted");
    let img = t.paint().unwrap().clone();
    assert_eq!(img.pixels[0] & 0xFF0000, 30 << 16, "the newest frame is the one decoded");
    assert_eq!(t.decodes(), 1);
    t.paint();
    assert_eq!(t.decodes(), 1, "painting again decodes nothing");
    t.engine_event(EngineEvent::Frame { id: 4, png: png(64, 64, 40) });
    assert_eq!(t.dropped_frames(), 2, "a frame that replaces a painted one is not a drop");
    assert_eq!(t.paint().unwrap().pixels[0] & 0xFF0000, 40 << 16);
    assert_eq!(t.decodes(), 2);
}

#[test]
fn a_frame_over_the_cap_is_acknowledged_counted_and_not_kept() {
    let mut t = shown((64, 64));
    t.engine_event(EngineEvent::Frame { id: 1, png: png(64, 64, 10) });
    let big = vec![0u8; MAX_FRAME_BYTES + 1];
    assert_eq!(t.engine_event(EngineEvent::Frame { id: 2, png: big }), vec![Effect::Ack(2)]);
    assert_eq!(t.too_big_frames(), 1);
    assert_eq!(t.paint().unwrap().pixels[0] & 0xFF0000, 10 << 16, "the earlier frame is still the newest kept");
    let exact = vec![7u8; MAX_FRAME_BYTES];
    t.engine_event(EngineEvent::Frame { id: 3, png: exact });
    assert_eq!(t.too_big_frames(), 1, "exactly the cap is kept (and then fails to decode)");
}

#[test]
fn a_frame_that_does_not_decode_keeps_the_old_picture_and_is_not_tried_again() {
    let mut t = shown((64, 64));
    t.engine_event(EngineEvent::Frame { id: 1, png: png(64, 64, 10) });
    t.paint();
    t.engine_event(EngineEvent::Frame { id: 2, png: b"not a png at all".to_vec() });
    let img = t.paint().unwrap();
    assert_eq!(img.pixels[0] & 0xFF0000, 10 << 16, "the old picture stays");
    assert_eq!((t.decode_errors(), t.decodes()), (1, 1));
    t.paint();
    assert_eq!(t.decode_errors(), 1, "the same bad frame is not decoded again");
}

#[test]
fn a_hidden_tab_decodes_nothing_keeps_its_last_compressed_frame_and_paints_it_when_shown_again() {
    let mut t = shown((64, 64));
    t.engine_event(EngineEvent::Frame { id: 1, png: png(64, 64, 10) });
    assert!(t.paint().is_some());
    t.hide();
    assert!(t.paint().is_none(), "a hidden tab paints nothing");
    assert_eq!(t.decoded_bytes(), 0, "the decoded picture is freed when the tab is hidden");
    assert!(t.kept_bytes() > 0, "the compressed frame is kept");
    assert_eq!(t.engine_event(EngineEvent::Frame { id: 2, png: png(64, 64, 20) }), vec![Effect::Ack(2)], "a late frame is still acknowledged");
    assert_eq!(t.decodes(), 1, "and not decoded");
    t.show((64, 64), 100);
    assert_eq!(t.paint().unwrap().pixels[0] & 0xFF0000, 20 << 16);
    assert_eq!(t.decodes(), 2);
}

#[test]
fn memory_is_one_decoded_picture_and_one_compressed_frame() {
    let mut t = shown((1920, 1200));
    assert_eq!((t.decoded_bytes(), t.kept_bytes()), (0, 0));
    let frame = png(300, 200, 5);
    t.engine_event(EngineEvent::Frame { id: 1, png: frame.clone() });
    t.engine_event(EngineEvent::Frame { id: 2, png: frame.clone() });
    assert_eq!(t.kept_bytes(), frame.len(), "one compressed frame, not two");
    t.paint();
    assert_eq!(t.decoded_bytes(), 300 * 200 * 4);
    assert!(t.decoded_bytes() + t.kept_bytes() <= MAX_VIEWPORT_W as usize * MAX_VIEWPORT_H as usize * 4 + MAX_FRAME_BYTES);
}

#[test]
fn without_a_first_frame_the_model_asks_for_one_screenshot_once_after_half_a_second() {
    let mut t = ready();
    t.show((200, 100), 1000);
    assert_eq!(t.tick(1100), vec![]);
    assert_eq!(t.tick(1499), vec![]);
    assert_eq!(t.tick(1500), vec![Effect::CaptureScreenshot]);
    assert_eq!(t.tick(1600), vec![], "asked once");
    t.engine_event(EngineEvent::Screenshot(png(200, 100, 9)));
    assert_eq!(t.paint().unwrap().pixels[0] & 0xFF0000, 9 << 16, "the screenshot is a frame (it has no number to acknowledge)");
    // a frame that arrives in time cancels the fallback; a new show starts the wait again
    let mut t = ready();
    t.show((200, 100), 0);
    assert_eq!(t.engine_event(EngineEvent::Frame { id: 1, png: png(200, 100, 1) }), vec![Effect::Ack(1)]);
    assert_eq!(t.tick(900), vec![]);
    t.hide();
    t.show((200, 100), 2000);
    assert_eq!(t.tick(2400), vec![]);
    assert_eq!(t.tick(2500), vec![Effect::CaptureScreenshot], "shown again and no new frame in half a second (a still page may send none): one screenshot, though an old frame is kept");
    let mut t = ready();
    assert_eq!(t.tick(10_000), vec![], "a hidden tab asks for nothing");
}

// ---- input ------------------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_persons_pointer_is_mapped_to_the_page_and_outside_the_page_is_nothing() {
    let mut t = shown((400, 300));
    assert_eq!(t.mouse(MouseKind::Pressed, 10, 20, Button::Left, 1, 0.0), Some(Effect::Mouse { kind: MouseKind::Pressed, x: 10.0, y: 20.0, button: Button::Left, clicks: 1, delta: 0.0 }));
    assert_eq!(t.mouse(MouseKind::Wheel, 399, 299, Button::None, 0, 120.0), Some(Effect::Mouse { kind: MouseKind::Wheel, x: 399.0, y: 299.0, button: Button::None, clicks: 0, delta: 120.0 }));
    assert_eq!(t.mouse(MouseKind::Moved, 400, 10, Button::None, 0, 0.0), None, "one past the right edge");
    assert_eq!(t.mouse(MouseKind::Moved, 10, -1, Button::None, 0, 0.0), None);
    assert_eq!(t.map_point(5, 6), Some((5.0, 6.0)));
    t.resize((3000, 2000));
    assert_eq!(t.map_point(1919, 1199), Some((1919.0, 1199.0)));
    assert_eq!(t.map_point(1920, 5), None, "the capped viewport is smaller than the area: outside it is not the page");
    t.hide();
    assert_eq!(t.mouse(MouseKind::Pressed, 10, 20, Button::Left, 1, 0.0), None, "a hidden tab takes no pointer");
}

#[test]
fn keys_and_text_go_to_the_page_only_while_the_page_has_the_focus() {
    let mut t = shown((400, 300));
    assert_eq!(t.focus(), Focus::Page);
    assert_eq!(t.key(KeyKind::Down, "a", "KeyA", "a"), Some(Effect::Key { kind: KeyKind::Down, key: "a".into(), code: "KeyA".into(), text: "a".into() }));
    assert_eq!(t.text("안녕\nx\u{7}"), Some(Effect::InsertText("안녕x".into())), "control characters are dropped from committed text");
    assert_eq!(t.text("\n\u{7}"), None, "nothing left");
    assert_eq!(t.text(&"가".repeat(MAX_INSERT + 10)), Some(Effect::InsertText("가".repeat(MAX_INSERT))));
    t.focus_address();
    assert_eq!(t.focus(), Focus::Address);
    assert_eq!(t.key(KeyKind::Down, "a", "KeyA", "a"), None);
    assert_eq!(t.text("x"), None);
    assert_eq!(t.mouse(MouseKind::Pressed, 5, 5, Button::Left, 1, 0.0).is_some(), true, "the pointer still reaches the page");
    t.focus_page();
    assert!(t.key(KeyKind::Up, "a", "KeyA", "").is_some());
    let mut away = Tab::new();
    away.engine_event(EngineEvent::Missing { expected_sha256: "a".into(), release: "r".into() });
    assert_eq!(away.text("x"), None, "an absent engine takes nothing");
}

#[test]
fn history_cannot_be_walked_while_the_engine_is_not_running_and_the_fallback_wait_restarts_with_each_show() {
    let mut t = shown((800, 600));
    for u in ["https://a.example", "https://b.example"] {
        t.submit_address(u).unwrap();
    }
    t.engine_event(EngineEvent::Crashed);
    assert_eq!(t.back(), vec![], "a tab whose engine stopped asks for nothing");
    assert_eq!(t.forward(), vec![]);
    assert_eq!(t.reload(), vec![]);
    let mut t = ready();
    t.show((200, 100), 0);
    assert_eq!(t.tick(500), vec![Effect::CaptureScreenshot]);
    t.hide();
    t.show((200, 100), 3000);
    assert_eq!(t.tick(3499), vec![]);
    assert_eq!(t.tick(3500), vec![Effect::CaptureScreenshot], "asked again after the next show");
}

// ---- notices (0.2.0): what the guard saw and what the host knows ------------------------------------------------------------------------------------

#[test]
fn a_refused_fetch_becomes_a_notice_with_the_host_and_the_reason() {
    let mut t = ready();
    assert_eq!(t.notice(), None);
    t.engine_event(EngineEvent::FetchRefused { status: 403, host: "intranet.example".into() });
    let n = t.notice().unwrap().to_string();
    assert!(n.contains("intranet.example") && n.contains("private network"), "{n}");
    t.engine_event(EngineEvent::FetchRefused { status: 502, host: "gone.example".into() });
    let n = t.notice().unwrap().to_string();
    assert!(n.contains("gone.example") && n.contains("could not be found or reached"), "the newest notice replaces the older: {n}");
    t.engine_event(EngineEvent::FetchRefused { status: 504, host: "slow.example".into() });
    assert!(t.notice().unwrap().contains("too long"));
    t.engine_event(EngineEvent::FetchRefused { status: 418, host: "odd.example".into() });
    assert!(t.notice().unwrap().contains("418"), "an unknown status is named, not guessed");
}

#[test]
fn the_host_in_a_notice_is_cleaned_and_the_notice_is_short() {
    let mut t = ready();
    t.engine_event(EngineEvent::FetchRefused { status: 403, host: "a\u{202E}b\nc.example".into() });
    let n = t.notice().unwrap();
    assert!(!n.contains('\u{202E}') && !n.contains('\n'), "{n:?}");
    t.engine_event(EngineEvent::FetchRefused { status: 403, host: "h".repeat(5000) });
    assert!(t.notice().unwrap().chars().count() <= MAX_NOTICE, "{}", t.notice().unwrap().chars().count());
}

#[test]
fn a_new_navigation_clears_the_notice_and_the_unpinned_mark_stays() {
    let mut t = ready();
    t.engine_event(EngineEvent::UnpinnedEngine);
    for step in 0..4 {
        t.engine_event(EngineEvent::FetchRefused { status: 403, host: "x.example".into() });
        assert!(t.notice().is_some());
        match step {
            0 => drop(t.submit_address("https://example.org/").unwrap()),
            1 => drop(t.submit_address("https://example.org/two").unwrap()),
            2 => drop(t.back()),
            _ => drop(t.reload()),
        }
        assert_eq!(t.notice(), None, "step {step}: leaving the page clears what was said about it");
        assert!(t.unpinned(), "step {step}: the engine does not change");
    }
}

#[test]
fn an_address_the_engine_refuses_as_local_has_its_own_plain_notice() {
    let mut t = ready();
    t.engine_event(EngineEvent::LocalAddressRefused);
    assert_eq!(t.notice(), Some("the engine refuses addresses on this computer or a private network, by number and as localhost"));
}

#[test]
fn only_an_engine_that_is_not_the_pinned_release_is_marked() {
    let mut t = Tab::new();
    assert!(!t.unpinned());
    assert_eq!(UNPINNED_NOTICE, "running an engine that is not the pinned release");
    t.engine_event(EngineEvent::UnpinnedEngine);
    assert!(t.unpinned());
}
