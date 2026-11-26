use super::*;
use crate::gui::ui::icons::Icon;
use crate::gui::panel::Step;
use crate::gui::scroll_oracle::app;
use crate::gui::BACKGROUND;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;

fn with_choice(choice: Choice, system_dark: Option<bool>) -> State {
    State { choice, system_dark, ..Default::default() }
}

mod pure {
    use super::*;

    #[test]
    fn a_new_state_is_dark_with_nothing_known_of_the_desktop() {
        let s = State::default();
        assert_eq!((s.choice, s.system_dark, s.mode()), (Choice::Dark, None, Mode::Dark));
        assert_eq!(s.json(), json!({"choice": "dark", "mode": "dark", "system": "unknown", "asked": 0}));
    }

    #[test]
    fn the_picture_shows_the_choice_and_not_the_look_the_choice_gives() {
        assert_eq!(with_choice(Choice::Dark, Some(false)).icon(), Icon::Moon);
        assert_eq!(with_choice(Choice::Light, Some(true)).icon(), Icon::Sun);
        for system in [None, Some(true), Some(false)] {
            assert_eq!(with_choice(Choice::System, system).icon(), Icon::Auto, "{system:?}");
        }
    }

    #[test]
    fn the_word_on_the_button_names_the_choice_and_for_the_system_the_look_it_gives_now() {
        assert_eq!(with_choice(Choice::Dark, None).label(), "Theme: dark");
        assert_eq!(with_choice(Choice::Light, None).label(), "Theme: light");
        assert_eq!(with_choice(Choice::System, None).label(), "Theme: system (now dark)", "until the desktop has answered, dark");
        assert_eq!(with_choice(Choice::System, Some(true)).label(), "Theme: system (now dark)");
        assert_eq!(with_choice(Choice::System, Some(false)).label(), "Theme: system (now light)");
    }

    #[test]
    fn the_mode_follows_the_choice_and_only_the_system_choice_follows_the_desktop() {
        assert_eq!(with_choice(Choice::Dark, Some(false)).mode(), Mode::Dark);
        assert_eq!(with_choice(Choice::Light, Some(true)).mode(), Mode::Light);
        assert_eq!(with_choice(Choice::System, Some(false)).mode(), Mode::Light);
        assert_eq!(with_choice(Choice::System, Some(true)).mode(), Mode::Dark);
    }

    #[test]
    fn the_state_says_choice_mode_and_what_the_desktop_said() {
        assert_eq!(with_choice(Choice::System, Some(false)).json(), json!({"choice": "system", "mode": "light", "system": "light", "asked": 0}));
        assert_eq!(with_choice(Choice::Light, Some(true)).json(), json!({"choice": "light", "mode": "light", "system": "dark", "asked": 0}));
    }

    #[test]
    fn the_desktop_is_asked_again_every_thirty_seconds_and_only_while_the_choice_is_system() {
        let mut s = with_choice(Choice::System, None);
        for second in 1..ASK_EVERY {
            assert!(!s.tick(), "second {second}: not yet");
        }
        assert!(s.tick(), "the thirtieth second: ask");
        assert!(s.begin_ask(), "the question is sent");
        for _ in 0..3 * ASK_EVERY {
            assert!(!s.tick(), "one question at a time: none is due while one is on its way");
        }
        s.answered(None);
        for second in 1..ASK_EVERY {
            assert!(!s.tick(), "a new count after the question: second {second}");
        }
        assert!(s.tick());
        for choice in [Choice::Dark, Choice::Light] {
            let mut s = with_choice(choice, None);
            assert!((0..4 * ASK_EVERY).all(|_| !s.tick()), "{choice:?} never asks");
        }
    }

    #[test]
    fn a_question_that_never_comes_back_is_given_up_and_asked_again() {
        let mut s = with_choice(Choice::System, None);
        assert!(s.begin_ask());
        for second in 1..GIVE_UP {
            assert!(!s.tick(), "second {second} of a question on its way");
        }
        assert!(!s.begin_ask(), "still on its way a second before it is given up");
        assert!(s.tick(), "given up: the desktop is asked again");
        assert!(s.begin_ask(), "and the new question may go");
        assert!(!s.begin_ask(), "one at a time again");
    }

    #[test]
    fn leaving_the_system_choice_resets_the_count() {
        let mut s = with_choice(Choice::System, None);
        for _ in 0..ASK_EVERY - 1 {
            s.tick();
        }
        s.choice = Choice::Light;
        assert!(!s.tick());
        s.choice = Choice::System;
        for second in 1..ASK_EVERY {
            assert!(!s.tick(), "back to system: the count began again, second {second}");
        }
        assert!(s.tick());
    }

    #[test]
    fn only_one_question_is_on_its_way_and_an_answer_frees_the_next() {
        let mut s = State::default();
        assert!(s.begin_ask());
        assert!(!s.begin_ask(), "one is on its way");
        s.answered(Some(true));
        assert!(s.begin_ask(), "the answer came: the next may go");
    }

    #[test]
    fn an_answer_changes_what_is_known_only_when_it_says_something_different() {
        let mut s = State::default();
        assert!(s.answered(Some(true)), "the first answer");
        assert_eq!(s.system_dark, Some(true));
        assert!(!s.answered(Some(true)), "the same again changes nothing");
        assert!(s.answered(Some(false)));
        assert_eq!(s.system_dark, Some(false));
        assert!(!s.answered(None), "an answer that says nothing keeps what was known");
        assert_eq!(s.system_dark, Some(false));
    }

    #[test]
    fn the_request_is_one_settings_set_with_the_word_and_nothing_else() {
        for choice in [Choice::Dark, Choice::Light, Choice::System] {
            assert_eq!(request(choice), json!({"op": "settings.set", "theme": choice.word()}));
        }
    }

    #[test]
    fn the_cadence_is_thirty_seconds_and_a_question_is_given_up_after_two_minutes() {
        assert_eq!(ASK_EVERY, 30, "the guide says about every thirty seconds");
        assert_eq!(GIVE_UP, 120, "and a question that has not come back in two minutes is forgotten");
    }

    #[test]
    fn every_question_started_is_counted_and_one_that_is_refused_is_not() {
        let mut s = with_choice(Choice::System, None);
        assert_eq!(s.json()["asked"], 0);
        assert!(s.begin_ask());
        assert!(!s.begin_ask(), "one at a time");
        assert_eq!(s.json()["asked"], 1, "the refused one is not counted");
        s.answered(Some(true));
        assert!(s.begin_ask());
        assert_eq!(s.json()["asked"], 2);
    }

    #[test]
    fn the_desktop_is_asked_the_colour_scheme_first_and_the_theme_name_after_it() {
        let seen = std::cell::RefCell::new(Vec::new());
        let said = probe_with(|key| {
            seen.borrow_mut().push(key.to_string());
            match key {
                "color-scheme" => "'prefer-light'".to_string(),
                _ => "'Adwaita-dark'".to_string(),
            }
        });
        assert_eq!(*seen.borrow(), ["color-scheme", "gtk-theme"]);
        assert_eq!(said, Some(false), "the scheme the person set says light, whatever the theme's name");
        assert_eq!(probe_with(|_| String::new()), None, "a desktop that says nothing says nothing");
    }

    #[test]
    fn a_program_that_ends_badly_is_not_believed_and_one_that_is_not_there_says_nothing() {
        assert_eq!(run("sh", &["-c", "echo prefer-dark"]), "prefer-dark\n");
        assert_eq!(run("sh", &["-c", "echo prefer-dark; exit 3"]), "", "it printed, but it failed");
        assert_eq!(run("lico-no-such-program-for-the-theme", &["get"]), "");
    }

    #[test]
    fn the_window_systems_two_themes_are_dark_and_light() {
        assert!(is_dark(winit::window::Theme::Dark));
        assert!(!is_dark(winit::window::Theme::Light));
    }
}

mod window {
    use super::*;

    const SIZE: (usize, usize) = (900, 600);

    /// A host that answers `n` requests with the settings the request names; the requests it took, when the thread is joined.
    fn host(n: usize, tag: &str) -> (String, std::thread::JoinHandle<Vec<Value>>) {
        host_with(n, tag, |req| match req["theme"].as_str() {
            Some("dark") | None => json!({}),
            Some(word) => json!({"theme": word}),
        })
    }

    /// The same, with the settings it reports given by `settings_of` the request.
    fn host_with(n: usize, tag: &str, settings_of: impl Fn(&Value) -> Value + Send + 'static) -> (String, std::thread::JoinHandle<Vec<Value>>) {
        let scratch = crate::handoff::Scratch::new(tag);
        let path = scratch.0.join("host.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let handle = std::thread::spawn(move || {
            let _keep = scratch;
            let mut seen = Vec::new();
            for _ in 0..n {
                let (mut s, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&mut s).read_line(&mut line).unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let settings = settings_of(&req);
                s.write_all(format!("{}\n", json!({"v": 1, "ok": 1, "settings": settings})).as_bytes()).unwrap();
                seen.push(req);
            }
            seen
        });
        (path.to_string_lossy().into_owned(), handle)
    }

    fn frame(app: &mut crate::gui::App) -> Vec<u32> {
        let mut buf = vec![0xFF12_3456u32; SIZE.0 * SIZE.1];
        assert!(app.compose(&mut buf, SIZE.0, SIZE.1, 0).is_some());
        buf
    }

    fn count(buf: &[u32], color: u32) -> usize {
        buf.iter().filter(|&&c| c == color).count()
    }

    #[test]
    fn a_press_goes_dark_light_system_dark_and_the_host_keeps_each_choice() {
        let (socket, handle) = host(4, "theme_cycle");
        let mut a = app(1.0, false);
        a.socket = socket;
        let mut seen = Vec::new();
        for _ in 0..4 {
            a.cycle_theme();
            seen.push((a.ui.theme.choice, a.ui.theme.mode(), a.ui.theme.icon().name(), a.ui.flash.clone().unwrap()));
        }
        assert_eq!(
            seen,
            [
                (Choice::Light, Mode::Light, "sun", "Theme: light.".to_string()),
                (Choice::System, Mode::Dark, "auto", "Theme: system (now dark).".to_string()),
                (Choice::Dark, Mode::Dark, "moon", "Theme: dark.".to_string()),
                (Choice::Light, Mode::Light, "sun", "Theme: light.".to_string()),
            ],
            "the first press from dark gives light; system is dark until the desktop answers"
        );
        let requests = handle.join().unwrap();
        assert_eq!(requests.iter().map(|r| r["theme"].as_str().unwrap()).collect::<Vec<_>>(), ["light", "system", "dark", "light"]);
        assert!(requests.iter().all(|r| r["op"] == "settings.set"));
    }

    #[test]
    fn without_a_host_the_look_still_changes_and_the_notice_says_it_was_not_kept() {
        let mut a = app(1.0, false);
        a.cycle_theme();
        assert_eq!((a.ui.theme.choice, a.ui.theme.mode()), (Choice::Light, Mode::Light), "it is a view: it changes anyway");
        let flash = a.ui.flash.clone().unwrap();
        assert!(flash.starts_with("Theme: light") && flash.contains("not kept"), "{flash}");
        a.cycle_theme();
        assert_eq!(a.ui.theme.choice, Choice::System, "and goes on round");
        assert!(a.dirty);
    }

    #[test]
    fn what_the_host_reports_sets_the_choice_and_a_report_without_a_theme_is_dark() {
        let mut a = app(1.0, false);
        for (settings, want) in [
            (json!({"theme": "light"}), Choice::Light),
            (json!({"theme": "system"}), Choice::System),
            (json!({"theme": "dark"}), Choice::Dark),
            (json!({"theme": "light"}), Choice::Light),
            (json!({}), Choice::Dark),
            (json!({"theme": "light"}), Choice::Light),
            (json!({"theme": "blue"}), Choice::Dark),
            (json!({"theme": 3}), Choice::Dark),
        ] {
            a.sync_settings(&settings);
            assert_eq!(a.ui.theme.choice, want, "{settings}");
        }
    }

    #[test]
    fn the_desktops_answer_moves_the_look_only_while_the_choice_is_system() {
        let mut a = app(1.0, false);
        a.sync_theme(&json!({"theme": "system"}));
        assert_eq!(a.ui.theme.mode(), Mode::Dark, "nothing asked yet that was answered");
        a.on_system_theme(Some(false));
        assert_eq!(a.ui.theme.mode(), Mode::Light, "the desktop says light");
        a.on_system_theme(None);
        assert_eq!(a.ui.theme.mode(), Mode::Light, "an answer of nothing keeps it");
        a.on_system_theme(Some(true));
        assert_eq!(a.ui.theme.mode(), Mode::Dark);
        a.sync_theme(&json!({"theme": "light"}));
        a.on_system_theme(Some(true));
        assert_eq!(a.ui.theme.mode(), Mode::Light, "a chosen light ignores the desktop");
        a.sync_theme(&json!({}));
        a.on_system_theme(Some(false));
        assert_eq!(a.ui.theme.mode(), Mode::Dark, "a chosen dark too");
        a.sync_theme(&json!({"theme": "system"}));
        assert_eq!(a.ui.theme.mode(), Mode::Light, "and when the system is chosen again the desktop's last answer is there already");
    }

    #[test]
    fn a_change_of_look_drops_the_kept_frame_and_a_change_of_the_picture_alone_does_not() {
        let mut a = app(1.0, false);
        frame(&mut a);
        assert!(a.ui.composed.is_some(), "a frame is kept");
        a.on_system_theme(Some(false));
        assert!(a.ui.composed.is_some(), "the desktop's answer while dark is chosen changes nothing that is drawn");
        a.sync_theme(&json!({"theme": "light"}));
        assert!(a.ui.composed.is_none(), "the colours moved: the next frame is whole");
        frame(&mut a);
        a.sync_theme(&json!({"theme": "system"}));
        assert!(a.ui.composed.is_some(), "light to system, the desktop having said light: the look is the same, so the kept frame is still good");
        a.on_system_theme(Some(true));
        assert!(a.ui.composed.is_none(), "the desktop went dark while the system is chosen: the look moved");
    }

    #[test]
    fn the_window_is_drawn_in_the_look_that_is_chosen_and_dark_is_what_it_always_was() {
        let mut a = app(1.0, false);
        let dark = frame(&mut a);
        assert!(count(&dark, BACKGROUND) > 1000, "the dark window has its background: {}", count(&dark, BACKGROUND));
        a.sync_theme(&json!({"theme": "light"}));
        let light = frame(&mut a);
        assert_eq!(count(&light, BACKGROUND), 0, "no pixel of the light window is the dark background");
        assert!(count(&light, crate::gui::theme::tone_with(Mode::Light, BACKGROUND)) > 1000, "the background is its pale mirror");
        a.sync_theme(&json!({}));
        assert_eq!(frame(&mut a), dark, "and back: the same pixels as before, none left over");
    }

    #[test]
    fn the_theme_button_follows_the_browser_in_the_bar_and_says_what_it_is() {
        let mut a = app(1.0, false);
        let ids = |a: &crate::gui::App| a.toolbar_entries().iter().map(|b| (b.id, b.icon.map(Icon::name), b.label.clone(), b.reason)).collect::<Vec<_>>();
        let tail = |a: &crate::gui::App| ids(a).split_off(3);
        assert_eq!(tail(&a), [("new_browser", Some("browser"), "Open browser".to_string(), None), ("theme", Some("moon"), "Theme: dark".to_string(), None)]);
        a.sync_theme(&json!({"theme": "light"}));
        assert_eq!(tail(&a)[1], ("theme", Some("sun"), "Theme: light".to_string(), None));
        a.sync_theme(&json!({"theme": "system"}));
        assert_eq!(tail(&a)[1], ("theme", Some("auto"), "Theme: system (now dark)".to_string(), None));
        a.on_system_theme(Some(false));
        assert_eq!(tail(&a)[1], ("theme", Some("auto"), "Theme: system (now light)".to_string(), None), "never greyed, whatever the state");
    }

    #[test]
    fn a_press_on_the_button_cycles_and_leaves_an_open_panel_open() {
        let mut a = app(1.0, false);
        let index = a.toolbar_entries().iter().position(|b| b.id == "theme").unwrap();
        a.panel_open(Step::Label);
        a.toolbar_click(index);
        assert!(a.ui.panel.is_some(), "the panel stays: the look is not a place to go");
        assert_eq!(a.ui.theme.choice, Choice::Light);
        a.panel_close();
        a.toolbar_click(index);
        assert_eq!(a.ui.theme.choice, Choice::System);
        let browser = a.toolbar_entries().iter().position(|b| b.id == "new_browser").unwrap();
        assert_eq!(browser + 1, index, "the theme is the icon after the browser");
    }

    #[test]
    fn the_theme_is_no_line_of_a_menu_and_a_key_of_the_window_only_by_its_button() {
        use crate::gui::menu::{place_of, Cmd, Place};
        assert_eq!(place_of(Cmd::CycleTheme), Place::Key);
        let a = app(1.0, false);
        for menu in 0..5 {
            assert!(!a.menu_entries(menu).iter().any(|e| e.cmd == Cmd::CycleTheme), "menu {menu}");
        }
    }

    #[test]
    fn the_command_cycles_like_the_button_does() {
        use crate::gui::menu::Cmd;
        let mut a = app(1.0, false);
        a.run_cmd(Cmd::CycleTheme);
        a.run_cmd(Cmd::CycleTheme);
        assert_eq!(a.ui.theme.choice, Choice::System);
    }

    #[test]
    fn the_system_choice_asks_the_desktop_at_once_and_again_every_thirty_seconds_and_no_other_choice_asks() {
        let mut a = app(1.0, false);
        a.sync_theme(&json!({"theme": "light"}));
        for _ in 0..2 * ASK_EVERY {
            a.theme_tick();
        }
        assert_eq!(a.ui.theme.json()["asked"], 0, "a chosen light never asks, however long it runs");
        a.sync_theme(&json!({"theme": "system"}));
        assert_eq!(a.ui.theme.json()["asked"], 1, "the system is asked at once, when it is chosen");
        for second in 1..ASK_EVERY {
            a.theme_tick();
            assert_eq!(a.ui.theme.json()["asked"], 1, "second {second}: not yet");
        }
        a.theme_tick();
        assert_eq!(a.ui.theme.json()["asked"], 2, "the thirtieth second asks again");
        a.sync_theme(&json!({}));
        for _ in 0..2 * ASK_EVERY {
            a.theme_tick();
        }
        assert_eq!(a.ui.theme.json()["asked"], 2, "and a dark that is chosen again stops the asking");
    }

    #[test]
    fn the_windows_own_second_is_the_one_that_counts_for_the_desktop_question() {
        // (the oracle's window writes to a socket nobody reads, which holds some hundreds of small writes: a few dozen ticks, not more)
        let mut a = app(1.0, false);
        a.sync_theme(&json!({"theme": "system"}));
        assert_eq!(a.ui.theme.json()["asked"], 1);
        for _ in 1..ASK_EVERY {
            a.ui_tick();
        }
        assert_eq!(a.ui.theme.json()["asked"], 1, "not yet");
        a.ui_tick();
        assert_eq!(a.ui.theme.json()["asked"], 2, "the window's tick asks when the thirty seconds are over");
    }

    #[test]
    fn what_the_window_system_says_is_the_answer_and_nothing_is_asked_of_the_desktop() {
        let mut a = app(1.0, false);
        a.sync_theme(&json!({"theme": "system"}));
        assert_eq!(a.ui.theme.json()["asked"], 1);
        assert!(a.ask_with(Some(false)).is_none());
        assert_eq!((a.ui.theme.system_dark, a.ui.theme.mode()), (Some(false), Mode::Light), "the window system said light");
        assert_eq!(a.ui.theme.json()["asked"], 1, "and nothing was asked of the desktop for it");
        assert!(a.ask_with(None).is_none(), "with no loop to answer through there is no proxy to give");
        assert_eq!(a.ui.theme.json()["asked"], 2, "but the question was begun, and is not held back for the next");
        a.on_window_theme(winit::window::Theme::Dark);
        assert_eq!((a.ui.theme.system_dark, a.ui.theme.mode()), (Some(true), Mode::Dark));
        a.on_window_theme(winit::window::Theme::Light);
        assert_eq!((a.ui.theme.system_dark, a.ui.theme.mode()), (Some(false), Mode::Light));
    }

    #[test]
    fn a_change_of_the_picture_alone_still_draws_the_window_again() {
        let mut a = app(1.0, false);
        a.dirty = false;
        a.sync_theme(&json!({"theme": "system"}));
        assert_eq!(a.ui.theme.mode(), Mode::Dark, "the look is the same");
        assert!(a.dirty, "the picture on the button is not");
        let (socket, handle) = host_with(1, "theme_same", |_| json!({}));
        a.socket = socket;
        a.sync_theme(&json!({}));
        a.dirty = false;
        a.cycle_theme();
        handle.join().unwrap();
        assert_eq!(a.ui.theme.choice, Choice::Dark, "the host kept dark, whatever was asked");
        assert!(a.ui.flash.is_some() && a.dirty, "the notice is drawn all the same");
    }
}
