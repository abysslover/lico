use super::*;
use lico_memguard::Level::{Ok as Fine, Orange, Red};

const NOW: u64 = 2_000_000_000;
const MIN: u64 = 60;
const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;

fn chat(session: &str, updated_at: u64) -> ChatEntry {
    ChatEntry {
        provider: "claude".into(), provider_session: session.into(), account: None, account_label: None, ssh_host: None, cwd: None, global: false,
        title: "t".into(), updated_at, open_session: None, open_state: None, running_elsewhere: false,
    }
}

fn row(state: &str, max_rss_mb: u64) -> Row {
    Row { session: "s1".into(), state: state.into(), max_rss_kb: max_rss_mb * 1024, memory_kb: 300 * 1024, rss_kb: max_rss_mb.max(1) * 1024, processes: 2, ..Default::default() }
}

#[test]
fn the_time_is_short_nothing_is_finer_than_a_minute_and_beyond_a_week_it_is_weeks_months_years() {
    let at = |secs: u64| age_label(NOW, NOW - secs);
    let words: Vec<String> = [0, 59, MIN, 59 * MIN + 59, HOUR, 23 * HOUR + 59 * MIN, DAY, 6 * DAY + 23 * HOUR, 7 * DAY, 29 * DAY, 30 * DAY, 364 * DAY, 365 * DAY, 800 * DAY].iter().map(|s| at(*s)).collect();
    assert_eq!(words, ["now", "now", "1m", "59m", "1h", "23h", "1d", "6d", "1w", "4w", "1mo", "12mo", "1y", "2y"]);
    assert_eq!((age_label(NOW, 0), age_label(NOW, NOW + 500)), ("now".to_string(), "now".to_string()), "no time known, or one in the future: now");
    assert!((0..800 * DAY).step_by(3 * HOUR as usize + 17).all(|s| at(s).chars().count() <= 4), "the time column is four characters wide at most");
}

#[test]
fn the_colour_of_the_time_has_four_steps_and_one_colour_beyond_a_week() {
    let class = |secs: u64| age_class(NOW, NOW - secs);
    assert_eq!([class(0), class(HOUR - 1), class(HOUR), class(DAY - 1), class(DAY), class(7 * DAY - 1), class(7 * DAY)], [AgeClass::Fresh, AgeClass::Fresh, AgeClass::Today, AgeClass::Today, AgeClass::Week, AgeClass::Week, AgeClass::Old]);
    assert!([8 * DAY, 40 * DAY, 3000 * DAY].iter().all(|s| class(*s) == AgeClass::Old), "an old chat is not told apart from an older one");
    assert_eq!(age_class(NOW, 0), AgeClass::Old, "no time known");
    let all = [AgeClass::Fresh, AgeClass::Today, AgeClass::Week, AgeClass::Old];
    for a in 0..4 {
        for b in a + 1..4 {
            assert!(all[a].ink() != all[b].ink() && all[a].name() != all[b].name(), "{:?} and {:?} are told apart", all[a], all[b]);
        }
    }
}

#[test]
fn a_chat_open_in_a_tab_takes_its_state_and_memory_from_the_tab_and_otherwise_from_the_list() {
    let mut it = chat("cl-1", NOW - 600);
    assert_eq!(state_of(&it, None, NOW), State::None);
    it.open_state = Some("running".into());
    assert_eq!(state_of(&it, None, NOW), State::Open(Fine), "the list says open and the tab list has no figure: fine");
    it.open_state = Some("disconnected".into());
    assert_eq!(state_of(&it, None, NOW), State::Closed);
    it.open_state = Some("exited".into());
    assert_eq!(state_of(&it, None, NOW), State::Ended);
    it.open_state = Some("running".into());
    assert_eq!([399, 400, 499, 500].map(|mb| state_of(&it, Some(&row("running", mb)), NOW)), [State::Open(Fine), State::Open(Orange), State::Open(Orange), State::Open(Red)], "the largest process against 400 and 500 MB, as the tab list");
    assert_eq!(state_of(&it, Some(&row("disconnected", 900)), NOW), State::Closed, "the tab says closed though the list was fetched while it ran");
    it.open_state = Some("disconnected".into());
    assert_eq!(state_of(&it, Some(&row("running", 10)), NOW), State::Open(Fine), "and the other way round: the tab is the newer word");
    it.open_state = Some("running".into());
    let stalled = Row { activity: "working".into(), last_active_at: NOW - 700, ..row("running", 10) };
    assert_eq!(state_of(&it, Some(&stalled), NOW), State::Open(Orange), "a working session that has been silent for ten minutes is orange");
    let busy = Row { activity: "working".into(), last_active_at: NOW - 100, ..row("running", 10) };
    assert_eq!(state_of(&it, Some(&busy), NOW), State::Open(Fine));
    let red_and_stalled = Row { activity: "working".into(), last_active_at: NOW - 700, ..row("running", 600) };
    assert_eq!(state_of(&it, Some(&red_and_stalled), NOW), State::Open(Red), "stalled never lowers a red");
    let mut other = chat("cl-2", NOW);
    other.running_elsewhere = true;
    assert_eq!(state_of(&other, None, NOW), State::None, "a chat another program writes shows no picture (the hint and the click say why)");
}

#[test]
fn a_state_has_its_own_word_shape_and_colour_and_only_an_open_chat_has_a_memory_word() {
    let states = [State::None, State::Open(Fine), State::Open(Orange), State::Open(Red), State::Closed, State::Ended];
    assert_eq!(states.map(State::name), ["none", "open", "open", "open", "closed", "ended"]);
    assert_eq!(states.map(State::memory_name), [None, Some("ok"), Some("orange"), Some("red"), None, None]);
    let inks: Vec<Color> = [State::Open(Fine), State::Open(Orange), State::Open(Red), State::Closed, State::Ended].iter().map(|s| s.ink()).collect();
    for a in 0..inks.len() {
        for b in a + 1..inks.len() {
            assert!(inks[a] != inks[b], "{a} and {b} have one colour");
        }
    }
    assert_eq!(State::Open(Orange).ink(), memrow::ink(Orange, Color::rgb(1, 2, 3)), "the colour of the tab list's figure");
    assert_eq!(State::Open(Red).ink(), memrow::ink(Red, Color::rgb(1, 2, 3)));
}

#[test]
fn what_tells_the_list_to_draw_again_follows_the_state_and_the_level_of_every_tab() {
    let base = rows_sig(&[row("running", 100)], NOW);
    assert_ne!(base, rows_sig(&[row("running", 450)], NOW), "a level changed");
    assert_ne!(rows_sig(&[row("running", 450)], NOW), rows_sig(&[row("running", 550)], NOW));
    assert_ne!(base, rows_sig(&[row("disconnected", 100)], NOW), "a state changed");
    assert_eq!(base, rows_sig(&[row("running", 101)], NOW), "a wobble inside a level does not");
    let busy = |ago: u64| Row { activity: "working".into(), last_active_at: NOW - ago, ..row("running", 100) };
    assert_ne!(rows_sig(&[busy(10)], NOW), rows_sig(&[busy(700)], NOW), "a session that became stalled");
    assert_ne!(rows_sig(&[row("running", 100)], NOW), rows_sig(&[row("running", 100), Row { session: "s2".into(), ..row("running", 100) }], NOW));
}

#[test]
fn a_card_says_where_the_chat_is_the_end_of_the_folder_as_long_as_it_fits_and_home_by_its_virtual_path() {
    let mut it = chat("cl-1", NOW);
    it.cwd = Some("/work/api/src".into());
    assert_eq!(folder_of(&it).as_deref(), Some("api/src"), "the last two components");
    assert_eq!(folder_line(&it, 40), "/work/api/src", "the whole path when it fits");
    assert!(folder_line(&it, 8).chars().count() <= 8 && folder_line(&it, 8).ends_with("src"), "{}", folder_line(&it, 8));
    it.cwd = Some("/srv/".into());
    assert_eq!((folder_of(&it).as_deref(), folder_line(&it, 40).as_str()), (Some("srv"), "/srv"), "a trailing slash is not a component");
    for none in [Some("/".to_string()), Some(String::new()), None] {
        it.cwd = none;
        assert_eq!((folder_of(&it), folder_line(&it, 40)), (None, String::new()), "no folder: an empty line");
    }
    it.global = true;
    assert_eq!((folder_of(&it).as_deref(), folder_line(&it, 40).as_str()), (Some("lico://home"), "lico://home"), "a Home session says where Home is");
}

#[test]
fn the_hint_names_the_title_the_time_the_state_in_words_the_folder_who_and_the_id() {
    let mut it = chat("cl-native-long-id", NOW - 600);
    it.title = "Fix the flaky test".into();
    it.cwd = Some("/work/api".into());
    it.account_label = Some("ClaudeWork".into());
    let open = row("running", 350);
    assert_eq!(hint_of(&it, State::Open(Fine), Some(&open), NOW, 200), "Fix the flaky test \u{00b7} 10m ago \u{00b7} open, 350 MiB \u{00b7} work/api \u{00b7} ClaudeWork \u{00b7} #cl-nativ");
    assert!(hint_of(&it, State::Open(Orange), Some(&open), NOW, 200).contains("open, 350 MiB, memory high"));
    assert!(hint_of(&it, State::Open(Red), Some(&open), NOW, 200).contains("open, 350 MiB, memory over the limit"));
    assert!(hint_of(&it, State::Open(Fine), None, NOW, 200).contains("\u{00b7} open \u{00b7}"), "no figure: no figure");
    assert!(hint_of(&it, State::Closed, None, NOW, 200).contains("tab closed"));
    assert!(hint_of(&it, State::Ended, None, NOW, 200).contains("\u{00b7} ended \u{00b7}"));
    it.account_label = Some("Native (~/.claude)".into());
    let plain = hint_of(&it, State::None, None, NOW, 200);
    assert_eq!(plain, "Fix the flaky test \u{00b7} 10m ago \u{00b7} work/api \u{00b7} #cl-nativ", "the default home says nothing, a chat that is not open has no state word");
    it.running_elsewhere = true;
    assert!(hint_of(&it, State::None, None, NOW, 200).contains("written by another program"), "the explanation that the card no longer shows");
    it.title = "A very long title that goes on and on and on and on and on and on and on and on".into();
    let cut = hint_of(&it, State::None, None, NOW, 100);
    assert!(cut.chars().count() <= 100 && cut.ends_with("#cl-nativ") && cut.contains('\u{2026}'), "the title gives way, the rest stays: {cut}");
    assert_eq!(hint_of(&chat("x", NOW), State::None, None, NOW, 200), "t \u{00b7} just now \u{00b7} #x");
}

#[test]
fn the_parts_of_a_card_lie_inside_it_and_never_over_each_other_at_every_scale() {
    for s in [1.0_f32, 1.5, 2.0, 3.0] {
        let lh = 19.0 * s;
        let card = [3.0 * s, 100.0, 330.0 * s - 3.0 * s, 100.0 + 2.0 * lh + 2.0 * s];
        let l = layout(card, s, lh, 30.0 * s);
        let inside = |r: Rect| r[0] >= card[0] && r[2] <= card[2] && r[1] >= card[1] && r[3] <= card[3] && r[2] > r[0] && r[3] > r[1];
        assert!([l.icon, l.title, l.time, l.folder, l.glyph].iter().all(|r| inside(*r)), "scale {s}: {l:?}");
        assert!(l.icon[2] <= l.title[0] && l.icon[2] <= l.folder[0], "the icon is left of both lines at {s}");
        assert!(l.title[2] <= l.time[0] && l.folder[2] <= l.glyph[0], "the title stops before the time, the folder before the picture at {s}");
        assert!(l.title[3] <= l.folder[1], "line one is above line two at {s}");
        assert!(l.time[1] == l.title[1] && l.glyph[1] >= l.folder[1] && l.glyph[3] <= l.folder[3], "the time is on line one, the picture on line two at {s}");
        assert_eq!(l.time[2], l.glyph[2], "the time and the picture end at one right edge at {s}");
        assert!(((l.icon[2] - l.icon[0]) - (l.icon[3] - l.icon[1])).abs() < 0.01 && l.icon[2] - l.icon[0] >= 8.0, "the icon is a square");
        assert!(((l.icon[1] - card[1]) - (card[3] - l.icon[3])).abs() < 1.0, "the icon is centred between the two lines at {s}");
        assert!(l.title[2] - l.title[0] > 150.0 * s, "the title has most of the card's width at {s}: {}", l.title[2] - l.title[0]);
    }
    let narrow = layout([0.0, 0.0, 60.0, 40.0], 1.0, 19.0, 30.0);
    assert!(narrow.title[2] >= narrow.title[0] && narrow.folder[2] >= narrow.folder[0], "a list too narrow for the parts gives a part no width, never a negative one");
}

#[test]
fn the_three_pictures_are_a_disc_a_ring_and_a_square_in_their_colours_and_stay_inside_their_rectangle() {
    const BG: u32 = 0xFF00_0000;
    const GROUND: u32 = 0xFF15_1E29;
    let r: Rect = [10.0, 10.0, 24.0, 24.0];
    let paint = |state: State| {
        let mut buf = vec![BG; 40 * 40];
        fill(&mut buf, 40, 40, r, GROUND);
        draw_glyph(&mut buf, 40, 40, state, r, GROUND);
        buf
    };
    let at = |buf: &[u32], x: usize, y: usize| buf[y * 40 + x];
    let ink = |s: State| argb(s.ink());
    for state in [State::Open(Fine), State::Open(Orange), State::Open(Red), State::Closed, State::Ended] {
        let buf = paint(state);
        let inked: Vec<usize> = (0..1600).filter(|&i| buf[i] == ink(state)).collect();
        assert!(inked.len() >= 20, "{state:?} draws something: {}", inked.len());
        assert!(inked.iter().all(|&i| (10..24).contains(&(i % 40)) && (10..24).contains(&(i / 40))), "{state:?} stays inside its rectangle");
        assert!((0..1600).filter(|i| !(10..24).contains(&(i % 40)) || !(10..24).contains(&(i / 40))).all(|i| buf[i] == BG), "{state:?}: nothing outside it");
    }
    let mut blank = vec![BG; 40 * 40];
    fill(&mut blank, 40, 40, r, GROUND);
    assert_eq!(paint(State::None), blank, "no state, no picture");
    let disc = paint(State::Open(Fine));
    assert!(at(&disc, 17, 17) == ink(State::Open(Fine)) && at(&disc, 10, 10) == GROUND && at(&disc, 23, 23) == GROUND, "a disc: ink in the middle and the card's colour in the corners");
    let ring = paint(State::Closed);
    assert!(at(&ring, 17, 17) == GROUND && at(&ring, 17, 11) == ink(State::Closed) && at(&ring, 11, 17) == ink(State::Closed), "a ring: hollow in the middle, ink on the round edge");
    let square = paint(State::Ended);
    assert!(at(&square, 17, 17) == ink(State::Ended) && at(&square, 12, 12) == ink(State::Ended) && at(&square, 10, 10) == GROUND, "a square: ink in its corners (inset a little), the card's colour round it");
    assert!(disc != ring && disc != square && ring != square, "three shapes, so that the colour is not the only thing to read");
    assert!(paint(State::Open(Orange)) != paint(State::Open(Red)) && paint(State::Open(Fine)) != paint(State::Open(Orange)), "the memory levels are three colours of one disc");
}

#[test]
fn the_provider_is_a_picture_of_the_wizard_and_has_no_letter() {
    assert_eq!([icon_of("claude"), icon_of("codex"), icon_of("local"), icon_of("other")], [Icon::Claude, Icon::Codex, Icon::Local, Icon::Codex]);
    assert!(icon_ink("claude") != icon_ink("codex") && icon_ink("codex") != icon_ink("local"));
}

#[test]
fn a_title_that_fits_has_one_line_and_a_longer_one_goes_on_to_the_second_at_a_space_when_there_is_one_near() {
    let none = String::new;
    assert_eq!(title_lines("Fix the test", 20, 20), ("Fix the test".to_string(), none()));
    assert_eq!(title_lines("  padded  ", 20, 20), ("padded".to_string(), none()), "the spaces round a title are not its own");
    assert_eq!(title_lines("exactly twenty chars", 20, 5), ("exactly twenty chars".to_string(), none()), "a title that fills the line to its last column is one line");
    let t = "Refactor the billing exporter so that the nightly job can be resumed";
    assert_eq!(title_lines(t, 30, 40), ("Refactor the billing exporter".to_string(), "so that the nightly job can be resumed".to_string()), "the break falls on the space");
    assert_eq!(title_lines(t, 20, 50), ("Refactor the billing".to_string(), "exporter so that the nightly job can be resumed".to_string()), "a space exactly at the edge");
    assert_eq!(title_lines(t, 25, 50), ("Refactor the billing".to_string(), "exporter so that the nightly job can be resumed".to_string()), "a word that would be cut goes whole to the next line");
    assert_eq!(title_lines(t, 30, 12).1, "so that the\u{2026}", "the second line is cut with an ellipsis: the title is never longer than two lines");
    assert_eq!(title_lines("supercalifragilisticexpialidocious", 10, 40), ("supercalif".to_string(), "ragilisticexpialidocious".to_string()), "a word longer than the line is cut where the line ends");
    assert_eq!(title_lines("a b", 0, 5).0, "", "a line with no room has no text, and no panic");
}

#[test]
fn a_wide_character_counts_two_columns_when_a_title_is_laid_out() {
    let width = |s: &str| s.chars().map(|c| if super::super::editor::wide(c) { 2 } else { 1 }).sum::<usize>();
    let (a, b) = title_lines("\u{d55c}\u{ae00} \u{c81c}\u{baa9}\u{c774} \u{b9e4}\u{c6b0} \u{ae38}\u{c5b4}\u{c11c} \u{b450} \u{c904}\u{b85c} \u{b118}\u{c5b4}\u{ac11}\u{b2c8}\u{b2e4}", 10, 12);
    assert_eq!(a, "\u{d55c}\u{ae00}", "the break is on the space before the word that does not fit");
    assert!(width(&a) <= 10 && width(&b) <= 12 && b.starts_with('\u{c81c}'), "{a:?} {b:?}");
    for cols in [1, 2, 3, 7, 10, 25] {
        let (a, b) = title_lines("\u{d55c}\u{ae00}\u{d55c}\u{ae00}\u{d55c}\u{ae00}\u{d55c}\u{ae00}\u{d55c}\u{ae00}\u{d55c}\u{ae00}\u{d55c}\u{ae00} wide", cols, cols);
        assert!(width(&a) <= cols && width(&b) <= cols, "{cols}: {a:?} {b:?}");
    }
}

#[test]
fn a_remembered_page_has_the_browsers_picture_no_folder_and_says_its_address_on_the_card_and_whole_in_the_hint() {
    let mut page = chat("https://docs.example.test/guide/getting-started/installing-on-linux?version=2#top", NOW - 2 * HOUR);
    page.provider = "browser".into();
    page.title = "Installing on Linux".into();
    assert_eq!(icon_of("browser"), Icon::Browser);
    assert_ne!(icon_ink("browser"), icon_ink("claude"), "the picture's colour is its own");
    assert_eq!(folder_of(&page), None, "a page has no folder");
    assert_eq!(state_of(&page, None, NOW), State::None, "a page is not a session: no state picture");
    // the card's second line is the address, `https://` dropped and cut in the middle as the bar cuts it
    let line = folder_line(&page, 30);
    assert!(line.starts_with("docs.example.test/") && line.contains('\u{2026}') && line.chars().count() <= 30, "{line:?}");
    assert_eq!(folder_line(&chat("cl-1", NOW), 30), "", "a chat with no folder has an empty second line, as before");
    // the hint row: title, time, the whole address; no id and no folder
    let hint = hint_of(&page, State::None, None, NOW, 200);
    assert_eq!(hint, format!("Installing on Linux \u{00b7} {} \u{00b7} {}", age_words(NOW, NOW - 2 * HOUR), page.provider_session), "{hint}");
    let short = hint_of(&page, State::None, None, NOW, 60);
    assert!(short.chars().count() <= 60 && short.starts_with("Installing"), "{short}");
}

#[test]
fn a_conversation_of_agy_has_its_own_picture_and_colour_the_folder_no_state_and_a_hint_of_the_time_the_folder_and_the_id() {
    let mut c = chat("11111111-aaaa-4bbb-8ccc-000000000001", NOW - 2 * HOUR);
    (c.provider, c.cwd) = ("agy".into(), Some("/work/api/v2".into()));
    assert_eq!(icon_of("agy"), Icon::Agy);
    for other in ["claude", "local", "browser", "codex"] {
        assert_ne!(icon_ink("agy"), icon_ink(other), "its colour is its own, not {other}'s");
    }
    assert_eq!(folder_of(&c).as_deref(), Some("api/v2"));
    assert!(folder_line(&c, 40).ends_with("api/v2"), "{:?}", folder_line(&c, 40));
    assert_eq!(state_of(&c, None, NOW), State::None, "a conversation of agy is no session of Lico's: no state picture");
    let hint = hint_of(&c, State::None, None, NOW, 200);
    assert_eq!(hint, format!("t \u{00b7} {} \u{00b7} api/v2 \u{00b7} #11111111", age_words(NOW, NOW - 2 * HOUR)), "{hint}");
}
