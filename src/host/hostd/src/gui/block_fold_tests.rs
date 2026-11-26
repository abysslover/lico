use super::*;
use crate::gui::blocks::{kind_of, Kind};
use crate::gui::scroll_oracle::app;
use crate::gui::transcript::{
    ROLE_BLOCK_FOLDED, ROLE_BOX, ROLE_BOX_BOT, ROLE_BOX_TOP, ROLE_PEER_BG, ROLE_USER_BG, STYLE_AGENT, STYLE_CMD, STYLE_CMD_OUT, STYLE_ERROR, STYLE_USER,
};

const USER: u16 = STYLE_USER | ROLE_USER_BG;

fn e(raw: usize, seq: i64, text: &str, role: u16) -> Entry {
    (raw, seq, text.to_string(), role)
}

fn set(keys: &[(&str, i64)]) -> HashSet<(String, i64)> {
    keys.iter().map(|(s, q)| (s.to_string(), *q)).collect()
}

mod pure {
    use super::*;

    fn conversation() -> Vec<Entry> {
        vec![
            e(0, 10, ">> hello there", USER),
            e(1, 11, "Reply line one", STYLE_AGENT),
            e(2, 12, "Reply line two", STYLE_AGENT),
            e(3, 13, "Reply line three", STYLE_AGENT),
            e(4, 14, ">> thanks", USER),
        ]
    }

    #[test]
    fn nothing_is_changed_when_nothing_is_folded() {
        assert_eq!(apply(conversation(), &HashSet::new(), "s1"), conversation());
        assert_eq!(apply(conversation(), &set(&[("s2", 11)]), "s1"), conversation(), "a block folded in another session is open here");
        assert_eq!(apply(conversation(), &set(&[("s1", 12)]), "s1"), conversation(), "the key is the first line of the block, not another line of it");
    }

    #[test]
    fn a_block_that_is_folded_is_one_line_with_the_first_lines_place_colour_and_time() {
        let out = apply(conversation(), &set(&[("s1", 11)]), "s1");
        assert_eq!(out.len(), 3, "the reply's three lines are one");
        assert_eq!(out[0], conversation()[0], "the block before is as it was");
        assert_eq!(out[2], conversation()[4], "and the one after");
        let (raw, seq, text, role) = &out[1];
        assert_eq!((*raw, *seq, text.as_str()), (1, 11, "▸ Reply line one  / 3 lines"), "the raw index and seq of the first line: its time is the block's");
        assert_ne!(role & ROLE_BLOCK_FOLDED, 0);
        assert_eq!(kind_of(*role), Kind::Response, "who spoke is still said by the colour");
        assert_eq!(role & (ROLE_BOX_TOP | ROLE_BOX_BOT), 0, "the agent's band has no card borders");
    }

    #[test]
    fn a_message_of_the_person_folds_and_keeps_its_band() {
        let entries = vec![e(0, 5, ">> first line of a long message", USER), e(1, 6, ">> second line", USER), e(2, 7, "Answer", STYLE_AGENT)];
        let out = apply(entries, &set(&[("s1", 5)]), "s1");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].2, "▸ >> first line of a long message  / 2 lines");
        assert_eq!((out[0].3 & (STYLE_USER | ROLE_USER_BG), out[0].3 & ROLE_BLOCK_FOLDED != 0), (STYLE_USER | ROLE_USER_BG, true));
        assert_eq!(out[1].2, "Answer");
        assert_eq!(out[0].3 & (ROLE_BOX_TOP | ROLE_BOX_BOT), 0, "the person's band has no card borders");
    }

    #[test]
    fn a_peers_card_folds_to_one_closed_card() {
        let top = STYLE_USER | ROLE_PEER_BG | ROLE_BOX_TOP;
        let mid = STYLE_USER | ROLE_PEER_BG;
        let end = STYLE_USER | ROLE_PEER_BG | ROLE_BOX_BOT;
        let entries = vec![e(0, 1, ">> [from session 'api' (s012345)]", top), e(1, 2, ">> a message", mid), e(2, 3, ">> its end", end), e(3, 4, "Reply", STYLE_AGENT)];
        let out = apply(entries, &set(&[("s1", 1)]), "s1");
        assert_eq!(out.len(), 2);
        let role = out[0].3;
        assert_eq!(kind_of(role), Kind::Peer);
        assert_eq!((role & ROLE_BOX_TOP != 0, role & ROLE_BOX_BOT != 0, role & ROLE_BLOCK_FOLDED != 0), (true, true, true), "a card of one row has both its borders");
        assert!(out[0].2.ends_with("  / 3 lines"), "{}", out[0].2);
    }

    #[test]
    fn a_command_a_note_and_a_short_single_line_are_never_folded() {
        let cmd_top = STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP;
        let cmd_end = STYLE_CMD_OUT | ROLE_BOX | ROLE_BOX_BOT;
        let entries = vec![e(0, 1, "$ ls", cmd_top), e(1, 2, "a.txt", cmd_end), e(2, 3, "[error] boom", STYLE_ERROR), e(3, 4, "short", STYLE_AGENT)];
        let all = set(&[("s1", 1), ("s1", 3), ("s1", 4)]);
        assert_eq!(apply(entries.clone(), &all, "s1"), entries, "the card has its own fold, a note is one line, a short line has nothing to fold");
        let long = format!("[error] {}", "boom ".repeat(30));
        let wide = vec![e(0, 1, &long, STYLE_ERROR), e(1, 2, &format!("$ {}", "x".repeat(100)), STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP | ROLE_BOX_BOT)];
        assert_eq!(apply(wide.clone(), &set(&[("s1", 1), ("s1", 2)]), "s1"), wide, "a note or a card is not the block of anyone, however long");
    }

    #[test]
    fn a_long_single_line_folds_and_says_it_was_one_line() {
        let long = "word ".repeat(30);
        let entries = vec![e(0, 1, &long, STYLE_AGENT), e(1, 2, ">> next", USER)];
        let out = apply(entries, &set(&[("s1", 1)]), "s1");
        assert_eq!(out.len(), 2);
        assert!(out[0].2.starts_with("▸ word word word"), "{}", out[0].2);
        assert!(out[0].2.ends_with("…  / 1 line"), "the words are cut and the count is singular: {}", out[0].2);
    }

    #[test]
    fn each_block_folds_by_itself() {
        let entries = vec![
            e(0, 1, "a1", STYLE_AGENT), e(1, 2, "a2", STYLE_AGENT),
            e(2, 3, "$ ls", STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP | ROLE_BOX_BOT),
            e(3, 4, "b1", STYLE_AGENT), e(4, 5, "b2", STYLE_AGENT),
        ];
        let out = apply(entries.clone(), &set(&[("s1", 4)]), "s1");
        assert_eq!(out.iter().map(|x| x.2.as_str()).collect::<Vec<_>>(), ["a1", "a2", "$ ls", "▸ b1  / 2 lines"], "the second reply folds, the first stays");
        let both = apply(entries, &set(&[("s1", 1), ("s1", 4)]), "s1");
        assert_eq!(both.iter().map(|x| x.2.as_str()).collect::<Vec<_>>(), ["▸ a1  / 2 lines", "$ ls", "▸ b1  / 2 lines"]);
    }

    #[test]
    fn a_block_that_is_folded_has_a_count_that_follows_it() {
        let mut entries = vec![e(0, 1, "a1", STYLE_AGENT), e(1, 2, "a2", STYLE_AGENT)];
        let folded = set(&[("s1", 1)]);
        assert_eq!(apply(entries.clone(), &folded, "s1")[0].2, "▸ a1  / 2 lines");
        entries.push(e(2, 3, "a3", STYLE_AGENT));
        assert_eq!(apply(entries, &folded, "s1")[0].2, "▸ a1  / 3 lines", "a reply that goes on growing keeps its key");
    }

    #[test]
    fn a_block_has_something_to_fold_with_two_lines_or_a_line_longer_than_the_pill() {
        let one = |text: &str| vec![e(0, 1, text, STYLE_AGENT)];
        assert!(!foldable(&one("short")));
        assert!(!foldable(&one(&"x".repeat(84))), "as wide as the pill is nothing to fold");
        assert!(foldable(&one(&"x".repeat(85))));
        assert!(!foldable(&one(&format!("   {}   ", "x".repeat(84)))), "the spaces around the words are not in the pill: nothing to fold");
        assert!(!foldable(&one(&"한".repeat(84))), "characters are counted, not bytes");
        assert!(foldable(&one(&"한".repeat(85))));
        assert!(foldable(&[e(0, 1, "a", STYLE_AGENT), e(1, 2, "b", STYLE_AGENT)]));
        assert!(!foldable(&[]));
        assert!(foldable(&[e(0, 1, "▸ short  / 3 lines", STYLE_AGENT | ROLE_BLOCK_FOLDED)]), "a block that is folded can be opened, whatever its pill says");
    }

    #[test]
    fn the_pill_says_how_many_lines_were_folded() {
        assert_eq!(pill("head", 3), "▸ head  / 3 lines");
        assert_eq!(pill("head", 2), "▸ head  / 2 lines");
        assert_eq!(pill("head", 1), "▸ head  / 1 line");
        assert_eq!(pill("  padded  ", 2), "▸ padded  / 2 lines", "the spaces around the words go");
        let cut = pill(&"x".repeat(100), 2);
        assert_eq!(cut, format!("▸ {}…  / 2 lines", "x".repeat(83)), "84 characters of the head at most, the last of them the ellipsis");
        assert_eq!(pill(&"x".repeat(84), 2), format!("▸ {}  / 2 lines", "x".repeat(84)), "84 fit");
    }
}

mod click {
    use super::*;
    use crate::gui::panel::Step;

    const W: usize = 1600;
    const H: usize = 800;

    struct Window {
        app: App,
        buf: Vec<u32>,
    }

    impl Window {
        fn new(lines: &[&str]) -> Window {
            let mut app = app(1.0, false);
            for (i, text) in lines.iter().enumerate() {
                app.push_line(10 + i as i64, text.to_string());
            }
            let mut w = Window { app, buf: vec![0; W * H] };
            w.frame();
            w
        }

        fn frame(&mut self) {
            assert!(self.app.compose(&mut self.buf, W, H, 0).is_some(), "nothing was drawn");
        }

        /// The point in the middle of the line with the raw index `raw`.
        fn at(&self, raw: usize) -> (f32, f32) {
            let line = self.app.ui.hit_lines.iter().find(|l| l.2 == raw).unwrap_or_else(|| panic!("line {raw} is not on the screen: {:?}", self.app.ui.hit_lines));
            (self.app.effective_list_w(1.0) as f32 + 300.0, (line.0 + line.1) / 2.0)
        }

        /// A stationary click on the line, then the frame that follows it.
        fn click(&mut self, raw: usize) {
            let (x, y) = self.at(raw);
            self.app.main_mouse_press(x, y);
            self.app.main_mouse_release();
            self.frame();
        }

        fn shown(&self) -> Vec<String> {
            self.app.shown_lines_dump()
        }
    }

    const CONVERSATION: [&str; 5] = [">> hello there", "Reply line one", "Reply line two", "Reply line three", ">> thanks"];

    #[test]
    fn a_click_selects_the_whole_block_and_copies_nothing() {
        let mut w = Window::new(&CONVERSATION);
        w.click(2);
        assert_eq!(w.app.ui.sel, Some((1, 3)), "the reply from its first line to its last, though the click was on the middle one");
        assert_eq!(w.shown().len(), 5, "nothing is folded by a first click");
        assert_eq!(w.app.selection_text().as_deref(), Some("Reply line one\nReply line two\nReply line three"));
        assert_eq!(w.app.ui.copied, 0, "a click selects, it does not copy");
    }

    #[test]
    fn a_second_click_folds_the_block_and_a_third_opens_it() {
        let mut w = Window::new(&CONVERSATION);
        w.click(2);
        w.click(2);
        assert_eq!(w.shown(), [">> hello there", "▸ Reply line one  / 3 lines", ">> thanks"]);
        assert_eq!(w.app.ui.sel, Some((1, 1)), "the block is the selection still, as one line");
        assert_eq!(w.app.selection_text().as_deref(), Some("▸ Reply line one  / 3 lines"));
        w.click(1);
        assert_eq!(w.shown(), CONVERSATION, "a click on the folded line opens the block");
        assert_eq!(w.app.ui.sel, Some((1, 3)), "and the whole block is selected again");
        w.click(3);
        assert_eq!(w.shown().len(), 3, "and so on, for as long as the person clicks");
    }

    #[test]
    fn a_short_line_is_selected_and_stays_open() {
        let mut w = Window::new(&CONVERSATION);
        w.click(0);
        assert_eq!(w.app.ui.sel, Some((0, 0)));
        w.click(0);
        assert_eq!((w.app.ui.sel, w.shown().len()), (Some((0, 0)), 5), "a line of one short row has nothing to fold");
        assert!(w.app.ui.block_fold.folded.is_empty());
    }

    #[test]
    fn a_click_on_another_block_moves_the_selection_and_folds_nothing() {
        let mut w = Window::new(&CONVERSATION);
        w.click(2);
        w.click(4);
        assert_eq!(w.app.ui.sel, Some((4, 4)));
        assert_eq!(w.shown().len(), 5);
        w.click(1);
        assert_eq!(w.app.ui.sel, Some((1, 3)), "the reply is selected again, not folded: it was not the selection");
        assert_eq!(w.shown().len(), 5);
    }

    #[test]
    fn the_press_alone_remembers_what_was_selected_and_a_drag_still_goes_line_by_line() {
        let mut w = Window::new(&CONVERSATION);
        w.click(2);
        let (x, y) = w.at(2);
        w.app.main_mouse_press(x, y);
        assert_eq!(w.app.ui.block_fold.before, Some((1, 3)), "the selection before the button went down");
        assert_eq!(w.app.ui.sel, Some((2, 2)), "the pressed line, so that a drag starts there");
        let (x3, y3) = w.at(3);
        w.app.main_mouse_drag(x3, y3);
        assert_eq!(w.app.ui.sel, Some((2, 3)), "a drag selects line by line");
        assert!(w.app.ui.block_fold.folded.is_empty());
    }

    #[test]
    fn a_selection_that_was_made_backwards_counts_as_the_block_when_it_is_the_block() {
        let mut w = Window::new(&CONVERSATION);
        w.app.ui.sel = Some((3, 1));
        let (x, y) = w.at(2);
        w.app.main_mouse_press(x, y);
        assert_eq!(w.app.ui.block_fold.before, Some((1, 3)), "the lower index first");
    }

    #[test]
    fn a_card_of_a_command_keeps_its_one_click() {
        let mut w = Window::new(&["Answer", "$ ls", "a.txt", "[exit 0]", "Done"]);
        assert_eq!(w.shown().iter().filter(|l| l.starts_with('▸')).count(), 1, "the card is folded to begin with");
        w.click(1);
        assert!(w.shown().iter().any(|l| l.starts_with('▾')), "one click opens it: {:?}", w.shown());
        assert_eq!(w.app.ui.sel, None, "as it always did");
        assert!(w.app.ui.block_fold.folded.is_empty());
    }

    #[test]
    fn the_block_click_is_not_for_a_note_or_a_card() {
        let w = Window::new(&["Answer", "[error] boom", "$ ls", "a.txt", "[exit 0]", "Done"]);
        let mut app = w.app;
        assert!(!app.try_block_click(1), "a note");
        assert!(!app.try_block_click(2), "the head of a card");
        assert!(!app.try_block_click(3), "its output");
        assert!(app.try_block_click(0), "the agent's reply");
        assert!(app.try_block_click(5), "and the one after the card");
    }

    #[test]
    fn a_note_is_one_line_and_keeps_the_line_click() {
        let mut w = Window::new(&["Answer", "[error] boom", "Done"]);
        w.click(1);
        assert_eq!(w.app.ui.sel, Some((1, 1)));
        w.click(1);
        assert_eq!((w.app.ui.sel, w.shown().len()), (Some((1, 1)), 3));
    }

    #[test]
    fn the_click_is_left_alone_where_a_block_has_no_meaning() {
        let mut w = Window::new(&CONVERSATION);
        w.app.ui.show_events = true;
        assert!(!w.app.try_block_click(2), "in the view of the internal events");
        w.app.ui.show_events = false;
        w.app.session.clear();
        assert!(!w.app.try_block_click(2), "with no session");
        w.app.session = "session-s0".into();
        w.app.ui.calls.active_call = Some("call".into());
        assert!(!w.app.try_block_click(2), "in the call room");
        w.app.ui.calls.active_call = None;
        assert!(!w.app.try_block_click(99), "a line the transcript does not hold");
        w.app.panel_open(Step::Label);
        assert!(!w.app.try_block_click(2), "in a panel");
        assert!(w.app.ui.block_fold.folded.is_empty());
    }

    #[test]
    fn a_toggle_drops_the_frame_that_was_kept_and_marks_the_window_for_a_frame() {
        let mut w = Window::new(&CONVERSATION);
        w.click(2);
        let (x, y) = w.at(2);
        w.app.dirty = false;
        w.app.main_mouse_press(x, y);
        w.app.main_mouse_release();
        assert!(w.app.ui.composed.is_none(), "what the buffer holds is not what the next frame needs");
        assert!(w.app.dirty);
    }

    #[test]
    fn a_reply_that_grows_after_it_was_folded_grows_the_count() {
        let mut w = Window::new(&[">> question", "Reply line one", "Reply line two", "Reply line three"]);
        w.click(2);
        w.click(2);
        assert_eq!(w.shown(), [">> question", "▸ Reply line one  / 3 lines"]);
        w.app.push_line(20, "Reply line four".into());
        w.frame();
        assert_eq!(w.shown(), [">> question", "▸ Reply line one  / 4 lines"]);
    }

    #[test]
    fn the_set_of_folded_blocks_starts_again_when_it_is_full() {
        let mut w = Window::new(&CONVERSATION);
        for n in 0..MAX_FOLDED as i64 {
            w.app.ui.block_fold.folded.insert(("session-s0".into(), 1_000 + n));
        }
        w.click(2);
        w.click(2);
        assert_eq!(w.app.ui.block_fold.folded.len(), 1, "the old keys went, the new one is there");
        assert_eq!(w.shown().len(), 3);
    }

    #[test]
    fn the_time_of_a_folded_block_is_the_time_of_its_first_line() {
        let mut w = Window::new(&CONVERSATION);
        w.click(2);
        w.click(2);
        let line = w.app.ui.hit_lines.iter().find(|l| l.2 == 1).expect("the pill is on the screen");
        assert_eq!(w.app.view.as_ref().and_then(|v| v.lines.get(line.2)).map(|l| l.0), Some(11), "the seq under the pill is the first line's");
        assert_eq!(kind_of(line.3), Kind::Response);
    }
}

mod look {
    use super::*;
    use crate::gui::transcript::{tint_role, AGENT_INK, PEER_INK, TOOL_FOLD_INK, USER_INK};
    use cosmic_text::{Attrs, AttrsList};

    #[test]
    fn a_folded_block_is_in_the_dim_ink_of_a_folded_card_whoever_spoke_and_an_open_one_is_not() {
        let base = AttrsList::new(&Attrs::new());
        for role in [STYLE_AGENT, USER, STYLE_USER | ROLE_PEER_BG | ROLE_BOX_TOP | ROLE_BOX_BOT] {
            let pill = tint_role(base.clone(), "▸ words  / 2 lines", role | ROLE_BLOCK_FOLDED);
            assert_eq!(pill.defaults().color_opt, Some(TOOL_FOLD_INK), "role {role:#x}");
        }
        let open = [(STYLE_AGENT, AGENT_INK), (USER, USER_INK), (STYLE_USER | ROLE_PEER_BG, PEER_INK)];
        for (role, ink) in open {
            assert_eq!(tint_role(base.clone(), "words", role).defaults().color_opt, Some(ink), "role {role:#x}: an open block keeps its own ink");
        }
    }

    #[test]
    fn the_dump_says_how_many_blocks_of_the_session_are_folded_and_what_is_selected() {
        let mut state = State::default();
        state.folded.insert(("s1".into(), 4));
        state.folded.insert(("s1".into(), 9));
        state.folded.insert(("s2".into(), 4));
        assert_eq!(state.json("s1", Some((2, 5))), serde_json::json!({"folded": 2, "sel": [2, 5]}));
        assert_eq!(state.json("s3", None), serde_json::json!({"folded": 0, "sel": null}));
    }
}
