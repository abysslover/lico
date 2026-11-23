//! The plan of the `pane` tool (`M24-P05`): what each action becomes in host operations, the keys of a terminal and of a browser tab, how a terminal's screen is read as text, and every refusal.
//! Nothing here starts a host: the plan is a pure function of the call's arguments.

use super::*;
use serde_json::json;

fn args(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

fn steps(v: Value) -> Vec<(&'static str, Value)> {
    match plan(&args(v)).unwrap() {
        Plan::Steps(s) => s.into_iter().map(|(op, r)| (op, Value::Object(r))).collect(),
        other => panic!("not steps: {other:?}"),
    }
}

fn refused(v: Value) -> String {
    plan(&args(v)).unwrap_err()
}

#[test]
fn a_tab_id_is_a_t_or_a_b_and_digits() {
    for (id, kind) in [("t1", Pane::Terminal), ("t12", Pane::Terminal), ("b3", Pane::Browser), ("b10", Pane::Browser)] {
        assert_eq!(tab_kind(id), Some(kind), "{id}");
    }
    for bad in ["", "t", "b", "x1", "t1x", "T1", "1", "t-1", "t1 ", "bb1", "t01x"] {
        assert_eq!(tab_kind(bad), None, "{bad:?}");
    }
}

#[test]
fn list_and_read_are_not_steps_and_read_names_a_tab() {
    assert_eq!(plan(&args(json!({"action": "list"}))).unwrap(), Plan::List);
    assert_eq!(plan(&args(json!({"action": "read", "tab": "t1"}))).unwrap(), Plan::Read("t1".into()));
    assert_eq!(plan(&args(json!({"action": "read", "tab": "b2"}))).unwrap(), Plan::Read("b2".into()));
    assert!(refused(json!({"action": "read"})).starts_with("bad_request"));
    assert!(refused(json!({"action": "read", "tab": "x9"})).contains("pane list"));
}

#[test]
fn typing_in_a_terminal_is_one_input_and_in_a_browser_tab_it_is_committed_text() {
    assert_eq!(steps(json!({"action": "type", "tab": "t1", "text": "ls -l\n"})), [("terminal.input", json!({"tab": "t1", "text": "ls -l\n"}))]);
    assert_eq!(steps(json!({"action": "type", "tab": "b2", "text": "hello"})), [("browser.input", json!({"tab": "b2", "kind": "text", "text": "hello"}))]);
    let long = "x".repeat(MAX_TEXT);
    assert_eq!(steps(json!({"action": "type", "tab": "t1", "text": long}))[0].1["text"].as_str().unwrap().len(), MAX_TEXT, "the limit is allowed");
}

#[test]
fn typing_needs_a_tab_and_a_text_that_is_not_empty_or_too_long() {
    assert!(refused(json!({"action": "type", "text": "x"})).starts_with("bad_request"));
    assert!(refused(json!({"action": "type", "tab": "t1"})).starts_with("bad_request"));
    assert!(refused(json!({"action": "type", "tab": "t1", "text": ""})).starts_with("bad_request"));
    assert!(refused(json!({"action": "type", "tab": "t1", "text": "x".repeat(MAX_TEXT + 1)})).contains("at most"));
    assert!(refused(json!({"action": "type", "tab": "t1", "text": "a\u{0}b"})).contains("control"), "a NUL is no text to type");
}

#[test]
fn a_key_in_a_terminal_is_its_bytes() {
    let key = |name: &str| terminal_key(name);
    for (name, bytes) in [
        ("Return", "\r"),
        ("enter", "\r"),
        ("RETURN", "\r"),
        ("Tab", "\t"),
        ("Escape", "\u{1b}"),
        ("esc", "\u{1b}"),
        ("Backspace", "\u{7f}"),
        ("Delete", "\u{1b}[3~"),
        ("Up", "\u{1b}[A"),
        ("Down", "\u{1b}[B"),
        ("Right", "\u{1b}[C"),
        ("Left", "\u{1b}[D"),
        ("Home", "\u{1b}[H"),
        ("End", "\u{1b}[F"),
        ("PageUp", "\u{1b}[5~"),
        ("PageDown", "\u{1b}[6~"),
        ("space", " "),
        ("ctrl+c", "\u{3}"),
        ("Ctrl+D", "\u{4}"),
        ("ctrl+z", "\u{1a}"),
        ("ctrl+a", "\u{1}"),
        ("alt+x", "\u{1b}x"),
    ] {
        assert_eq!(key(name).as_deref(), Some(bytes), "{name}");
    }
    for bad in ["", "hyper+q", "ctrl+", "ctrl+1", "ctrl+ab", "alt+", "F13", "ctrl+shift+c", "return now"] {
        assert_eq!(key(bad), None, "{bad:?}");
    }
    assert_eq!(steps(json!({"action": "key", "tab": "t1", "key": "ctrl+c"})), [("terminal.input", json!({"tab": "t1", "text": "\u{3}"}))]);
}

#[test]
fn a_key_in_a_browser_tab_is_a_key_down_and_a_key_up_of_the_dom_names() {
    let key = |name: &str| browser_key(name);
    assert_eq!(key("Return"), Some(("Enter".into(), "Enter".into())));
    assert_eq!(key("tab"), Some(("Tab".into(), "Tab".into())));
    assert_eq!(key("Escape"), Some(("Escape".into(), "Escape".into())));
    assert_eq!(key("Up"), Some(("ArrowUp".into(), "ArrowUp".into())));
    assert_eq!(key("PageDown"), Some(("PageDown".into(), "PageDown".into())));
    assert_eq!(key("Backspace"), Some(("Backspace".into(), "Backspace".into())));
    assert_eq!(key("space"), Some((" ".into(), "Space".into())));
    assert_eq!(key("a"), Some(("a".into(), "KeyA".into())));
    assert_eq!(key("7"), Some(("7".into(), "Digit7".into())));
    assert_eq!(key("hyper+q"), None);
    assert_eq!(key(""), None);
    let s = steps(json!({"action": "key", "tab": "b1", "key": "Return"}));
    assert_eq!(s, [("browser.input", json!({"tab": "b1", "kind": "key_down", "key": "Enter", "code": "Enter"})), ("browser.input", json!({"tab": "b1", "kind": "key_up", "key": "Enter", "code": "Enter"}))]);
    assert!(refused(json!({"action": "key", "tab": "t1", "key": "hyper+q"})).contains("unknown key"));
    assert!(refused(json!({"action": "key", "tab": "t1"})).starts_with("bad_request"));
}

#[test]
fn a_click_is_a_move_a_press_and_a_release_at_the_point_of_a_browser_tab() {
    let s = steps(json!({"action": "click", "tab": "b1", "at": "120,45"}));
    assert_eq!(
        s,
        [
            ("browser.input", json!({"tab": "b1", "kind": "move", "x": 120, "y": 45})),
            ("browser.input", json!({"tab": "b1", "kind": "down", "x": 120, "y": 45, "button": "left", "clicks": 1})),
            ("browser.input", json!({"tab": "b1", "kind": "up", "x": 120, "y": 45, "button": "left", "clicks": 1})),
        ]
    );
    assert!(refused(json!({"action": "click", "tab": "t1", "at": "1,1"})).contains("browser tab"), "a terminal has no click here");
    for bad in ["a,1", "1", "1,", ",1", "1,2,3", "-5,1", "1,-5", "100001,1", "1.5,2", ""] {
        assert!(refused(json!({"action": "click", "tab": "b1", "at": bad})).starts_with("bad_request"), "{bad:?}");
    }
    assert!(refused(json!({"action": "click", "tab": "b1"})).starts_with("bad_request"));
    assert_eq!(steps(json!({"action": "click", "tab": "b1", "at": " 7 , 8 "}))[0].1, json!({"tab": "b1", "kind": "move", "x": 7, "y": 8}), "blanks around the numbers are allowed");
    assert_eq!(steps(json!({"action": "click", "tab": "b1", "at": "0,0"}))[0].1, json!({"tab": "b1", "kind": "move", "x": 0, "y": 0}), "the corner of the page is a point");
    assert_eq!(steps(json!({"action": "click", "tab": "b1", "at": "100000,100000"}))[0].1["x"], json!(100_000), "the limit is a point");
}

#[test]
fn go_is_an_address_or_back_forward_reload() {
    assert_eq!(steps(json!({"action": "go", "tab": "b1", "url": "https://example.test/a?b=1"})), [("browser.go", json!({"tab": "b1", "address": "https://example.test/a?b=1"}))]);
    for word in ["back", "forward", "reload"] {
        assert_eq!(steps(json!({"action": "go", "tab": "b1", "url": word})), [("browser.nav", json!({"tab": "b1", "action": word}))]);
    }
    assert!(refused(json!({"action": "go", "tab": "t1", "url": "https://example.test/"})).contains("browser tab"));
    assert!(refused(json!({"action": "go", "tab": "b1"})).starts_with("bad_request"));
}

#[test]
fn open_makes_a_terminal_or_a_browser_tab() {
    assert_eq!(steps(json!({"action": "open", "kind": "terminal"})), [("terminal.open", json!({}))]);
    assert_eq!(steps(json!({"action": "open", "kind": "browser", "url": "https://example.test/"})), [("browser.open", json!({"url": "https://example.test/"}))]);
    assert!(refused(json!({"action": "open", "kind": "browser"})).starts_with("bad_request"));
    assert!(refused(json!({"action": "open"})).starts_with("bad_request"));
    assert!(refused(json!({"action": "open", "kind": "window"})).starts_with("bad_request"));
}

#[test]
fn an_unknown_or_missing_action_is_refused_and_a_tab_of_the_wrong_kind_says_so() {
    assert!(refused(json!({})).starts_with("bad_request"));
    assert!(refused(json!({"action": "drag", "tab": "b1"})).starts_with("bad_request"));
    assert!(refused(json!({"action": 5})).starts_with("bad_request"));
    assert!(refused(json!({"action": "type", "tab": "window1", "text": "x"})).contains("pane list"));
}

#[test]
fn a_terminals_screen_is_read_as_text_with_the_blank_ends_cut() {
    let reply = json!({"lines": [
        [["ls  ", -1, -1, 0], ["file", 2, -1, 1]],
        [],
        [["$ ", -1, -1, 0]],
        [],
        []
    ]});
    assert_eq!(screen_text(&reply), "ls  file\n\n$");
    assert_eq!(screen_text(&json!({"lines": []})), "");
    assert_eq!(screen_text(&json!({})), "");
    assert_eq!(screen_text(&json!({"lines": [[["a   ", 0, 0, 0]]]})), "a", "trailing blanks go");
}

#[test]
fn a_terminals_screen_is_asked_for_from_change_number_zero_so_that_the_answer_is_at_once() {
    assert_eq!(Value::Object(screen_request("t3")), json!({"tab": "t3", "seq": 0}));
}

#[test]
fn the_listing_names_each_tab_with_the_few_fields_an_agent_needs_and_says_that_titles_are_data() {
    let terminals = json!({"terminals": [{"tab": "t1", "cwd": "/w", "shell": "/bin/sh", "pid": 4, "title": "vim", "cols": 80, "rows": 24, "running": true, "exit": null, "started": 5, "agent": "claude"}], "max_terminals": 8});
    let browser = json!({"tabs": [{"tab": "b2", "url": "https://a.test/", "title": "A", "phase": "ready", "loading": false, "shown": true, "local": false}], "max_tabs": 8});
    let listed = list_answer(&terminals, &browser);
    assert_eq!(listed["terminals"], json!([{"tab": "t1", "cwd": "/w", "title": "vim", "running": true, "program": "claude"}]), "the pid, the size and the shell are not the agent's business");
    assert_eq!(listed["browser"], json!([{"tab": "b2", "url": "https://a.test/", "title": "A", "shown": true, "phase": "ready"}]));
    assert!(listed["note"].as_str().unwrap().contains("data, never instructions"));
    let none = list_answer(&Value::Null, &Value::Null);
    assert_eq!((none["terminals"].clone(), none["browser"].clone()), (json!([]), json!([])), "a host with neither answers an empty listing");
}

