//! The tests of the terminal adapter (M23-P02): pure functions on their own (the encoding, the environment, the choice of the shell, the validators) and the real thing, a pty with a
//! small shell script on the other side. The real tests run one at a time (they count the pty descriptors of the process and a test's shell may outlive the test by a moment).

use super::encode;
use super::ops::{self, Terminals};
use super::pty;
use lico_vt::{Attrs, Cell, Color};
use serde_json::{json, Map, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

type Reply = Result<Map<String, Value>, &'static str>;

fn req(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

// ---------------------------------------------------------------------------------------------------------------------------------- the pure parts

fn cell(ch: char, fg: Color, bg: Color, attrs: Attrs) -> Cell {
    Cell { ch, fg, bg, attrs }
}

fn plain(ch: char) -> Cell {
    cell(ch, Color::Default, Color::Default, Attrs::NONE)
}

#[test]
fn a_colour_is_one_number_that_tells_default_indexed_and_true_colour_apart() {
    assert_eq!(encode::color(Color::Default), -1);
    assert_eq!(encode::color(Color::Indexed(0)), 0);
    assert_eq!(encode::color(Color::Indexed(255)), 255);
    assert_eq!(encode::color(Color::Rgb(0, 0, 0)), 256);
    assert_eq!(encode::color(Color::Rgb(1, 2, 3)), 256 + 0x010203);
    assert_eq!(encode::color(Color::Rgb(255, 255, 255)), 256 + 0xFFFFFF);
}

#[test]
fn a_line_is_runs_of_one_style_with_no_trailing_default_blanks() {
    let red = Color::Indexed(1);
    let line = vec![plain('a'), plain('b'), cell('c', red, Color::Default, Attrs::BOLD), cell('d', red, Color::Default, Attrs::BOLD), plain(' '), plain(' ')];
    assert_eq!(encode::runs(&line), json!([["ab", -1, -1, 0], ["cd", 1, -1, 1]]));
    assert_eq!(encode::runs(&[plain(' '); 5]), json!([]), "a blank line is no run");
    assert_eq!(encode::runs(&[]), json!([]));
    // a blank that is drawn (a background, an inverse, an underline) is kept, a blank in the middle is text
    let bg = cell(' ', Color::Default, Color::Indexed(4), Attrs::NONE);
    assert_eq!(encode::runs(&[plain('x'), bg, bg]), json!([["x", -1, -1, 0], ["  ", -1, 4, 0]]));
    assert_eq!(encode::runs(&[plain('x'), cell(' ', Color::Default, Color::Default, Attrs::INVERSE)]), json!([["x", -1, -1, 0], [" ", -1, -1, 32]]));
    assert_eq!(encode::runs(&[plain('x'), plain(' '), plain('y')]), json!([["x y", -1, -1, 0]]));
}

#[test]
fn a_wide_character_is_sent_once_and_its_flag_is_not_part_of_the_style() {
    let wide = cell('한', Color::Default, Color::Default, Attrs::WIDE);
    let tail = cell(' ', Color::Default, Color::Default, Attrs::TAIL);
    assert_eq!(encode::runs(&[wide, tail, plain('x')]), json!([["한x", -1, -1, 0]]), "the tail is skipped, the wide flag is not a style");
    assert_eq!(encode::runs(&[plain('x'), wide, tail]), json!([["x한", -1, -1, 0]]), "a wide character at the end is kept with its tail trimmed away");
    assert_eq!(encode::attrs(Attrs::WIDE | Attrs::BOLD), 1);
    assert_eq!(encode::attrs(Attrs::TAIL), 0);
}

#[test]
fn a_screen_of_a_style_soup_stays_under_three_megabytes() {
    // the worst case for the encoding: every cell of 200 columns by 60 rows has a style of its own
    let rows: Vec<Vec<Cell>> = (0..60u32)
        .map(|r| (0..200u32).map(|c| cell(char::from(b'a' + (c % 26) as u8), Color::Rgb((r * 4) as u8, (c % 256) as u8, (r + c) as u8), Color::Rgb((c * 3) as u8, (r * 7) as u8, 9), Attrs::NONE)).collect())
        .collect();
    let lines: Vec<Value> = rows.iter().map(|l| encode::runs(l)).collect();
    let size = Value::Array(lines).to_string().len();
    assert!(size < 3_000_000, "{size} bytes");
    assert!(size > 200 * 60 * 10, "the soup is really sent: {size}");
}

#[test]
fn the_modes_are_named_the_way_the_protocol_says() {
    let mut t = lico_vt::Term::new(10, 3);
    t.feed(b"\x1b[?1h\x1b[?2004h\x1b[?1049h\x1b[?1002h\x1b[?1006h\x1b[?1004h\x1b[?25l");
    assert_eq!(
        encode::modes(t.modes()),
        json!({"app_cursor": true, "app_keypad": false, "bracketed_paste": true, "alt_screen": true, "mouse": "drag", "mouse_sgr": true, "focus_events": true, "cursor_visible": false})
    );
    assert_eq!(encode::modes(lico_vt::Term::new(5, 2).modes())["mouse"], json!("off"));
}

fn vars(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn the_shell_gets_a_short_list_of_names_and_nothing_else_of_the_hosts_environment() {
    // the list is spelt out here, not read from the program: a name added or dropped there shows here
    let kept = [
        "PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LANGUAGE", "TZ", "TMPDIR", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS", "SSH_AUTH_SOCK", "EDITOR", "VISUAL", "PAGER",
        "DESKTOP_SESSION", "XMODIFIERS", "GTK_IM_MODULE", "QT_IM_MODULE", "ENV", "ZDOTDIR", "INPUTRC", "LC_ALL", "LC_CTYPE", "LC_X", "XDG_RUNTIME_DIR", "XDG_SESSION_TYPE", "XDG_DATA_DIRS", "XDG_X", "LICO_SOCKET",
    ];
    let dropped = [
        "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "HF_TOKEN", "GITHUB_TOKEN", "AWS_SECRET_ACCESS_KEY", "ORCA_AGENT_HOOK_TOKEN", "ORCA_BITBUCKET_API_TOKEN", "http_proxy", "HTTPS_PROXY", "GIT_ASKPASS",
        "SSH_ASKPASS", "LICO_TOKEN", "LICO_", "LICOX", "LICO_SOCKETX", "NOT_LC_ALL", "MY_XDG_X", "BASH_ENV", "ENVIRON", "INPUTRC2", "LC", "LCX", "XDG", "XDGX", "PATHX", "HOMEWORK", "path", "home", "Display", "lang", "xdg_runtime_dir", "lc_all",
    ];
    let host: Vec<(String, String)> = kept.iter().chain(dropped.iter()).map(|k| (k.to_string(), format!("value-of-{k}"))).chain(vars(&[("TERM", "dumb"), ("COLORTERM", "no")])).collect();
    let env = ops::child_env(host);
    let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    for k in kept {
        assert_eq!(get(k), Some(format!("value-of-{k}").as_str()), "{k} arrives as it is");
    }
    for k in dropped {
        assert_eq!(get(k), None, "{k} does not arrive");
    }
    assert_eq!(get("TERM"), Some("xterm-256color"));
    assert_eq!(get("COLORTERM"), Some("truecolor"));
    assert_eq!(env.iter().filter(|(k, _)| k == "TERM").count(), 1, "not twice");
    assert_eq!(env.len(), kept.len() + 2, "nothing else arrives: the list and the two names Lico sets");
}

#[test]
fn the_shell_is_the_one_asked_for_else_the_persons_else_bin_sh() {
    let has = |p: &str| matches!(p, "/bin/zsh" | "/bin/sh" | "/usr/bin/fish");
    assert_eq!(ops::choose_shell(Some("/usr/bin/fish"), Some("/bin/zsh"), &has), Ok("/usr/bin/fish".into()));
    assert_eq!(ops::choose_shell(None, Some("/bin/zsh"), &has), Ok("/bin/zsh".into()));
    assert_eq!(ops::choose_shell(None, Some("/bin/gone"), &has), Ok("/bin/sh".into()), "a shell of the person's that is not there falls back");
    assert_eq!(ops::choose_shell(None, Some("zsh"), &has), Ok("/bin/sh".into()), "not an absolute path");
    assert_eq!(ops::choose_shell(None, None, &has), Ok("/bin/sh".into()));
    assert_eq!(ops::choose_shell(Some("/bin/gone"), Some("/bin/zsh"), &has), Err("shell_missing"), "a shell that was asked for is not replaced");
    assert_eq!(ops::choose_shell(None, None, &|_| false), Err("shell_missing"));
}

#[test]
fn the_seven_operations_accept_exactly_their_fields_and_refuse_unknown_ones() {
    let ok = |v: Value| ops::valid(v["op"].as_str().unwrap(), &req(v.clone()));
    assert!(ok(json!({"op": "terminal.open"})));
    assert!(ok(json!({"op": "terminal.open", "v": 1, "id": "x", "cwd": "/w", "shell": "/bin/sh", "cols": 80, "rows": 24})));
    assert!(ok(json!({"op": "terminal.open", "cols": 500, "rows": 200})));
    assert!(ok(json!({"op": "terminal.list"})));
    assert!(ok(json!({"op": "terminal.close", "tab": "t12"})));
    assert!(ok(json!({"op": "terminal.input", "tab": "t1", "text": "ls\r"})));
    assert!(ok(json!({"op": "terminal.input", "tab": "t1", "data": "AQID"})));
    assert!(ok(json!({"op": "terminal.resize", "tab": "t1", "cols": 1, "rows": 1})));
    assert!(ok(json!({"op": "terminal.watch", "tab": "t1"})));
    assert!(ok(json!({"op": "terminal.watch", "tab": "t1", "seq": 0, "scroll": 1_000_000})));
    assert!(ok(json!({"op": "terminal.text", "tab": "t1", "from": [0, 0], "to": [1_000_000, 1000]})));
    for bad in [
        json!({"op": "terminal.open", "extra": 1}),
        json!({"op": "terminal.open", "cols": 0}),
        json!({"op": "terminal.open", "cols": 501}),
        json!({"op": "terminal.open", "rows": 201}),
        json!({"op": "terminal.open", "cols": "80"}),
        json!({"op": "terminal.open", "cols": -1}),
        json!({"op": "terminal.open", "cwd": ""}),
        json!({"op": "terminal.open", "cwd": 5}),
        json!({"op": "terminal.open", "shell": "sh"}),
        json!({"op": "terminal.open", "shell": ""}),
        json!({"op": "terminal.open", "v": 2}),
        json!({"op": "terminal.open", "id": ""}),
        json!({"op": "terminal.list", "tab": "t1"}),
        json!({"op": "terminal.close"}),
        json!({"op": "terminal.close", "tab": "b1"}),
        json!({"op": "terminal.close", "tab": "t"}),
        json!({"op": "terminal.close", "tab": "t0"}),
        json!({"op": "terminal.close", "tab": "t01"}),
        json!({"op": "terminal.close", "tab": "t1x"}),
        json!({"op": "terminal.close", "tab": "t12345678"}),
        json!({"op": "terminal.input", "tab": "t1"}),
        json!({"op": "terminal.input", "tab": "t1", "text": ""}),
        json!({"op": "terminal.input", "tab": "t1", "text": "a", "data": "AQID"}),
        json!({"op": "terminal.input", "tab": "t1", "data": "***"}),
        json!({"op": "terminal.input", "tab": "t1", "data": ""}),
        json!({"op": "terminal.input", "tab": "t1", "text": 5}),
        json!({"op": "terminal.resize", "tab": "t1", "cols": 80}),
        json!({"op": "terminal.resize", "tab": "t1", "cols": 0, "rows": 5}),
        json!({"op": "terminal.watch", "tab": "t1", "seq": -1}),
        json!({"op": "terminal.watch", "tab": "t1", "scroll": 1_000_001}),
        json!({"op": "terminal.watch", "seq": 0}),
        json!({"op": "terminal.text", "tab": "t1", "from": [0, 0]}),
        json!({"op": "terminal.text", "tab": "t1", "from": [0], "to": [0, 0]}),
        json!({"op": "terminal.text", "tab": "t1", "from": [0, 0, 0], "to": [0, 0]}),
        json!({"op": "terminal.text", "tab": "t1", "from": [-1, 0], "to": [0, 0]}),
        json!({"op": "terminal.text", "tab": "t1", "from": [0, 1001], "to": [0, 0]}),
        json!({"op": "terminal.text", "tab": "t1", "from": [1_000_001, 0], "to": [0, 0]}),
        json!({"op": "terminal.nothing"}),
    ] {
        assert!(!ok(bad.clone()), "{bad}");
    }
    // the size of an input: 64 KiB of text or of bytes, no more (the number is written out here: the test must not follow the constant)
    assert_eq!(ops::MAX_INPUT, 65_536);
    assert!(ok(json!({"op": "terminal.input", "tab": "t1", "text": "x".repeat(65_536)})));
    assert!(!ok(json!({"op": "terminal.input", "tab": "t1", "text": "x".repeat(65_537)})));
    let b64 = |n: usize| crate::base64::base64_encode(&vec![7u8; n]);
    assert!(ok(json!({"op": "terminal.input", "tab": "t1", "data": b64(65_536)})));
    assert!(!ok(json!({"op": "terminal.input", "tab": "t1", "data": b64(65_537)})));
}

#[test]
fn the_operations_are_listed_and_all_of_them_are_socket_only() {
    for op in ops::NAMES {
        assert!(crate::core::OPS.contains(&op), "{op} is in the host's list of operations");
        assert!(crate::admin::SOCKET_ONLY_OPS.contains(&op), "{op} is refused over POST /v1");
    }
    assert_eq!(ops::NAMES.len(), 7);
}

// ---------------------------------------------------------------------------------------------------------------------------------- a real pty

#[test]
fn the_size_of_a_pty_round_trips_and_a_read_with_nothing_to_read_waits() {
    let _serial = serial();
    let (master, _slave) = pty::open_pair(100, 30).unwrap();
    assert_eq!(master.size().unwrap(), (100, 30));
    master.set_size(321, 77).unwrap();
    assert_eq!(master.size().unwrap(), (321, 77));
    let mut buf = [0u8; 16];
    assert_eq!(master.read(&mut buf), pty::ReadResult::Wait);
    assert!(!master.wait_readable(20), "nothing was written");
}

static SERIAL: Mutex<()> = Mutex::new(());
static SPAWNER: Once = Once::new();

fn serial() -> MutexGuard<'static, ()> {
    SPAWNER.call_once(crate::procs::init_spawner);
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// The pty descriptors this process holds (masters and slaves).
fn pty_fds() -> usize {
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
        .filter(|target| target.to_string_lossy().starts_with("/dev/ptmx") || target.to_string_lossy().starts_with("/dev/pts/"))
        .count()
}

fn alive(pid: i64) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !stat.rsplit_once(") ").is_some_and(|(_, rest)| rest.starts_with('Z')), // a zombie is dead; its parent has not looked yet
        Err(_) => false,
    }
}

fn eventually(what: &str, within: Duration, f: impl Fn() -> bool) {
    let end = Instant::now() + within;
    while !f() {
        assert!(Instant::now() < end, "still not so after {within:?}: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

struct Rig {
    terminals: Arc<Terminals>,
    dir: PathBuf,
    scripts: Mutex<u32>,
    /// The environment the host "has" in a test: a few names of the real one (the shell scripts need `PATH`), the folder of the rig as `HOME`, and names that must or must not reach the shell.
    vars: Vec<(String, String)>,
    /// Set when the rig is dropped: ends the watchdog, which otherwise closes every terminal after 12 seconds so that a test whose watch never comes fails instead of hanging.
    finished: Arc<AtomicBool>,
    _serial: MutexGuard<'static, ()>,
}

fn plain_folder(path: &str) -> Result<String, &'static str> {
    std::fs::canonicalize(path).map(|c| c.to_string_lossy().into_owned()).map_err(|_| "folder_missing")
}

impl Rig {
    fn new() -> Rig {
        Rig::with_watchdog(12)
    }

    /// A rig whose watchdog closes every terminal after `seconds` (a test that starts several programs one after another needs more than the 12 seconds of the usual one when the machine's
    /// disks are busy: a program that is not in the cache takes seconds to start).
    fn with_watchdog(seconds: u64) -> Rig {
        let guard = serial();
        eventually("no pty is held from an earlier test", Duration::from_secs(5), || pty_fds() == 0);
        let dir = std::env::temp_dir().join(format!("lico_term_{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(dir).unwrap();
        let vars = vec![
            ("PATH".to_string(), std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into())),
            ("HOME".to_string(), dir.to_string_lossy().into_owned()),
            ("TERM".to_string(), "dumb".to_string()),
            ("FOO".to_string(), "bar".to_string()),
            ("LICO_SECRET".to_string(), "hidden".to_string()),
            ("LICO_SOCKET".to_string(), "/the/socket".to_string()),
        ];
        let (terminals, finished) = (Terminals::new(), Arc::new(AtomicBool::new(false)));
        let (watched, flag) = (terminals.clone(), finished.clone());
        std::thread::spawn(move || {
            for _ in 0..seconds * 10 {
                if flag.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            watched.close_all(); // every watch is released with `unknown_tab`: the test that waits for something that never comes fails
        });
        Rig { terminals, dir, scripts: Mutex::new(0), vars, finished, _serial: guard }
    }

    /// A shell script (`/bin/sh`) with `body`, executable, in the rig's folder.
    fn script(&self, body: &str) -> String {
        let mut n = self.scripts.lock().unwrap();
        *n += 1;
        let path = self.dir.join(format!("s{n}.sh"));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn call(&self, v: Value) -> Reply {
        let request = req(v);
        let op = request["op"].as_str().unwrap().to_string();
        assert!(ops::valid(&op, &request), "{request:?}");
        self.terminals.run(&op, &request, &ops::Host { folder: &plain_folder, vars: self.vars.clone() })
    }

    /// A watch that waits on its own thread.
    fn watching(&self, tab: &str, seq: u64) -> std::thread::JoinHandle<Reply> {
        let (terminals, vars, tab) = (self.terminals.clone(), self.vars.clone(), tab.to_string());
        std::thread::spawn(move || terminals.run("terminal.watch", &req(json!({"op": "terminal.watch", "tab": tab, "seq": seq})), &ops::Host { folder: &plain_folder, vars }))
    }

    fn open(&self, body: &str) -> String {
        let shell = self.script(body);
        let r = self.call(json!({"op": "terminal.open", "cwd": self.dir.to_string_lossy(), "shell": shell})).unwrap();
        r["tab"].as_str().unwrap().to_string()
    }

    /// Watches `tab` until `pred` is true of the answer (the rig's watchdog ends a watch that never comes).
    fn until(&self, tab: &str, what: &str, pred: impl Fn(&Map<String, Value>) -> bool) -> Map<String, Value> {
        let mut seq = 0;
        let mut last = String::new();
        let found = loop {
            match self.call(json!({"op": "terminal.watch", "tab": tab, "seq": seq})) {
                Ok(w) => {
                    seq = w["seq"].as_u64().unwrap();
                    last = screen(&w);
                    if pred(&w) {
                        break Some(w);
                    }
                }
                Err(_) => break None,
            }
        };
        found.unwrap_or_else(|| panic!("the terminal never showed {what}; its screen was:\n{last}"))
    }

    fn shows(&self, tab: &str, text: &str) -> Map<String, Value> {
        self.until(tab, text, |w| screen(w).contains(text))
    }

    fn type_in(&self, tab: &str, text: &str) {
        self.call(json!({"op": "terminal.input", "tab": tab, "text": text})).unwrap();
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
        self.terminals.close_all();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The text of the lines of a watch answer, one line after another.
fn screen(w: &Map<String, Value>) -> String {
    let lines = w["lines"].as_array().cloned().unwrap_or_default();
    lines.iter().map(|l| l.as_array().unwrap().iter().map(|r| r[0].as_str().unwrap()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_terminal_shows_what_the_program_writes_and_the_line_it_reads_comes_back_echoed() {
    let rig = Rig::new();
    let tab = rig.open("printf 'hello\\n'; read line; printf 'got:%s\\n' \"$line\"; exit 3");
    assert_eq!(tab, "t1");
    rig.shows(&tab, "hello");
    rig.type_in(&tab, "abc\r");
    let w = rig.until(&tab, "the program's answer and its end", |w| screen(w).contains("got:abc") && w["running"] == json!(false));
    let text = screen(&w);
    assert!(text.contains("hello\nabc\ngot:abc"), "the line discipline echoed the typed line before the program answered:\n{text}");
    assert_eq!(w["exit"], json!(3));
    assert_eq!((w["cols"].clone(), w["rows"].clone()), (json!(80), json!(24)));
    assert_eq!(w["cursor"], json!([3, 0]), "the cursor is after the last line");
    assert_eq!(w["scroll"], json!(0));
    assert!(w["lines"].as_array().unwrap().len() == 24 && w["wrapped"].as_array().unwrap().len() == 24);
    assert_eq!(w["modes"]["alt_screen"], json!(false));
}

#[test]
fn the_row_of_the_list_says_what_runs_where_and_what_ended() {
    let rig = Rig::new();
    let tab = rig.open("printf '\\033]0;the title\\007'; pwd; exit 5");
    let w = rig.until(&tab, "the end", |w| w["running"] == json!(false));
    assert_eq!(w["title"], json!("the title"));
    let list = rig.call(json!({"op": "terminal.list"})).unwrap();
    assert_eq!(list["max_terminals"], json!(8));
    let row = &list["terminals"][0];
    assert_eq!((row["tab"].clone(), row["running"].clone(), row["exit"].clone(), row["title"].clone()), (json!(tab), json!(false), json!(5), json!("the title")));
    assert_eq!((row["cols"].clone(), row["rows"].clone()), (json!(80), json!(24)));
    assert_eq!(row["cwd"], json!(rig.dir.to_string_lossy()));
    assert!(row["pid"].as_u64().unwrap() > 1 && row["started"].as_u64().unwrap() > 1_700_000_000);
    assert!(row["shell"].as_str().unwrap().ends_with("s1.sh"));
    assert!(screen(&w).starts_with(&format!("{}\n", rig.dir.to_string_lossy())), "the shell started in the folder asked for:\n{}", screen(&w));
}

#[test]
fn the_pty_has_the_size_asked_for_and_a_resize_reaches_the_program() {
    let rig = Rig::new();
    let shell = rig.script("stty size; read x; stty size; exit 0");
    let r = rig.call(json!({"op": "terminal.open", "cwd": rig.dir.to_string_lossy(), "shell": shell, "cols": 100, "rows": 30})).unwrap();
    assert_eq!((r["cols"].clone(), r["rows"].clone()), (json!(100), json!(30)));
    let tab = r["tab"].as_str().unwrap();
    let w = rig.shows(tab, "30 100");
    assert_eq!((w["cols"].clone(), w["rows"].clone()), (json!(100), json!(30)));
    assert_eq!(rig.call(json!({"op": "terminal.resize", "tab": tab, "cols": 90, "rows": 20})).unwrap()["cols"], json!(90));
    let w = rig.until(tab, "the new size", |w| w["cols"] == json!(90) && w["rows"] == json!(20));
    assert_eq!(w["lines"].as_array().unwrap().len(), 20, "the emulator has the new size too");
    rig.type_in(tab, "\r");
    rig.shows(tab, "20 90");
}

#[test]
fn a_resize_to_the_size_it_has_announces_no_change() {
    let rig = Rig::new();
    let tab = rig.open("exec sleep 30"); // writes nothing: the number moves only when the size does
    let seq_of = || rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0})).unwrap()["seq"].as_u64().unwrap();
    let before = seq_of();
    rig.call(json!({"op": "terminal.resize", "tab": tab, "cols": 80, "rows": 24})).unwrap();
    assert_eq!(seq_of(), before, "the same size: the number did not move");
    let waiting = rig.watching(&tab, before);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!waiting.is_finished(), "so a watch that waits for a change goes on waiting");
    rig.call(json!({"op": "terminal.resize", "tab": tab, "cols": 81, "rows": 24})).unwrap();
    assert!(waiting.join().unwrap().unwrap()["seq"].as_u64().unwrap() > before, "another size: the number moved and the watch was answered");
}

#[test]
fn a_program_that_ends_is_not_running_and_a_signal_is_128_plus_its_number() {
    let rig = Rig::new();
    let tab = rig.open("echo bye; exit 7");
    let w = rig.until(&tab, "the end", |w| w["running"] == json!(false) && screen(w).contains("bye"));
    assert_eq!(w["exit"], json!(7));
    assert_eq!(rig.call(json!({"op": "terminal.input", "tab": tab, "text": "x"})), Err("terminal_ended"));
    // it stays listed, with its screen, until it is closed
    assert_eq!(rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"].as_array().unwrap().len(), 1);
    assert!(screen(&rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0})).unwrap()).contains("bye"));
    let killed = rig.open("kill -9 $$");
    let w = rig.until(&killed, "the end", |w| w["running"] == json!(false));
    assert_eq!(w["exit"], json!(137));
    assert_eq!(rig.call(json!({"op": "terminal.close", "tab": tab})).unwrap()["closed"], json!(true));
    assert_eq!(rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0})), Err("unknown_tab"));
    assert_eq!(rig.call(json!({"op": "terminal.close", "tab": tab})), Err("unknown_tab"));
}

#[test]
fn a_close_ends_the_shell_and_what_it_started_and_frees_the_pty() {
    let rig = Rig::new();
    let marker = rig.dir.join("child.pid");
    let tab = rig.open(&format!("sleep 300 &\necho $! > {}\nwait", marker.display()));
    eventually("the shell wrote the pid of its background child", Duration::from_secs(5), || std::fs::read_to_string(&marker).is_ok_and(|s| s.trim().parse::<i64>().is_ok()));
    let child: i64 = std::fs::read_to_string(&marker).unwrap().trim().parse().unwrap();
    let shell = rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"][0]["pid"].as_i64().unwrap();
    assert!(alive(child) && alive(shell));
    assert!(pty_fds() >= 1);
    rig.call(json!({"op": "terminal.close", "tab": tab})).unwrap();
    eventually("the shell and its background child are gone", Duration::from_secs(4), || !alive(child) && !alive(shell));
    eventually("the pty descriptors are back to none", Duration::from_secs(4), || pty_fds() == 0);
}

#[test]
fn a_group_that_ignores_the_hangup_is_killed_after_a_second() {
    let rig = Rig::new();
    let marker = rig.dir.join("child.pid");
    let tab = rig.open(&format!("trap '' HUP\nsleep 300 &\necho $! > {}\nwait", marker.display()));
    eventually("the child's pid", Duration::from_secs(5), || std::fs::read_to_string(&marker).is_ok_and(|s| s.trim().parse::<i64>().is_ok()));
    let child: i64 = std::fs::read_to_string(&marker).unwrap().trim().parse().unwrap();
    let shell = rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"][0]["pid"].as_i64().unwrap();
    let closed_at = Instant::now();
    rig.call(json!({"op": "terminal.close", "tab": tab})).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(alive(child) && alive(shell), "the hang-up alone does not end a group that ignores it");
    eventually("the kill after a second", Duration::from_secs(4), || !alive(child) && !alive(shell));
    assert!(closed_at.elapsed() >= Duration::from_millis(900), "not before the second: {:?}", closed_at.elapsed());
}

#[test]
fn the_ninth_terminal_is_refused_and_a_closed_one_makes_room() {
    let rig = Rig::new();
    let tabs: Vec<String> = (0..8).map(|_| rig.open("exec sleep 30")).collect();
    assert_eq!(tabs.last().unwrap(), "t8");
    let shell = rig.script("exec sleep 30");
    let ninth = json!({"op": "terminal.open", "cwd": rig.dir.to_string_lossy(), "shell": shell});
    assert_eq!(rig.call(ninth.clone()), Err("tab_limit"));
    rig.call(json!({"op": "terminal.close", "tab": tabs[3]})).unwrap();
    assert_eq!(rig.call(ninth).unwrap()["tab"], json!("t9"), "an id is not used again");
}

#[test]
fn the_folder_is_the_hosts_check_of_a_folder_and_the_default_is_the_home() {
    let rig = Rig::new();
    let shell = rig.script("exec sleep 30");
    let asked = std::sync::Mutex::new(Vec::new());
    let refuse = |p: &str| {
        asked.lock().unwrap().push(p.to_string());
        Err("folder_protected")
    };
    let host = ops::Host { folder: &refuse, vars: rig.vars.clone() };
    let open = |v: Value| rig.terminals.run("terminal.open", &req(v), &host);
    assert_eq!(open(json!({"op": "terminal.open", "cwd": "/anywhere", "shell": shell})), Err("folder_protected"));
    assert_eq!(open(json!({"op": "terminal.open", "shell": shell})), Err("folder_protected"));
    assert_eq!(*asked.lock().unwrap(), vec!["/anywhere".to_string(), rig.dir.to_string_lossy().into_owned()], "no folder asked for: the home folder is the one checked");
    assert_eq!(rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"].as_array().unwrap().len(), 0, "nothing was started");
    // a folder that is not there (the check the rig uses is a plain one)
    assert_eq!(rig.call(json!({"op": "terminal.open", "cwd": rig.dir.join("none").to_string_lossy(), "shell": shell})), Err("folder_missing"));
    // a shell that is not there is its own error, and the terminal is not started
    assert_eq!(rig.call(json!({"op": "terminal.open", "cwd": rig.dir.to_string_lossy(), "shell": rig.dir.join("none.sh").to_string_lossy()})), Err("shell_missing"));
    assert_eq!(rig.call(json!({"op": "terminal.open", "cwd": rig.dir.to_string_lossy(), "shell": rig.dir.to_string_lossy()})), Err("shell_missing"), "a folder is no shell");
    assert_eq!(rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"].as_array().unwrap().len(), 0);
}

#[test]
fn the_shell_has_the_terminal_variables_and_the_socket_and_none_of_the_hosts_other_names() {
    let rig = Rig::new();
    // `cargo test` gives its own process the CARGO_* names; the host of this rig was given none of them, so the shell must have none (the shell starts from nothing but what the host has)
    let tab = rig.open("printf 'T=%s C=%s F=%s S=%s X=%s K=%s\\n' \"$TERM\" \"$COLORTERM\" \"$FOO\" \"$LICO_SOCKET\" \"${LICO_SECRET-gone}\" \"$(env | grep -c '^CARGO_')\"; exit 0");
    let w = rig.until(&tab, "the end", |w| w["running"] == json!(false));
    let text = screen(&w);
    assert!(text.contains("T=xterm-256color C=truecolor F= S=/the/socket X=gone K=0"), "FOO was a variable of the host, not on the short list: {text}");
}

#[test]
fn with_no_shell_asked_for_the_persons_shell_is_used_and_with_no_folder_the_home() {
    let rig = Rig::new();
    let mut vars = rig.vars.clone();
    vars.push(("SHELL".to_string(), rig.script("pwd; exit 4")));
    let host = ops::Host { folder: &plain_folder, vars };
    let r = rig.terminals.run("terminal.open", &req(json!({"op": "terminal.open"})), &host).unwrap();
    assert_eq!(r["shell"], json!(host.vars.last().unwrap().1), "$SHELL");
    assert_eq!(r["cwd"], json!(rig.dir.to_string_lossy()), "the home folder");
    let tab = r["tab"].as_str().unwrap();
    let w = rig.until(tab, "the end", |w| w["running"] == json!(false));
    assert_eq!(w["exit"], json!(4));
    assert!(screen(&w).starts_with(&*rig.dir.to_string_lossy()), "{}", screen(&w));
}

#[test]
fn a_watch_answers_at_once_for_seq_zero_waits_otherwise_and_many_may_wait_together() {
    let rig = Rig::new();
    let tab = rig.open("read a; printf 'one:%s\\n' \"$a\"; read b; printf 'two:%s\\n' \"$b\"; exec sleep 30");
    let first = rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0})).unwrap();
    let seq = first["seq"].as_u64().unwrap();
    assert!(seq >= 1);
    // the program may still be starting: wait until the number stops moving, then two watches wait together
    let mut seq = seq;
    loop {
        std::thread::sleep(Duration::from_millis(150));
        let now = rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0})).unwrap()["seq"].as_u64().unwrap();
        if now == seq {
            break;
        }
        seq = now;
    }
    let (a, b) = (rig.watching(&tab, seq), rig.watching(&tab, seq));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!a.is_finished() && !b.is_finished(), "nothing changed: both wait, with no timeout, and the second did not take the first's place");
    rig.type_in(&tab, "x\r");
    for w in [a, b] {
        let w = w.join().unwrap().unwrap();
        assert!(w["seq"].as_u64().unwrap() > seq);
    }
    rig.shows(&tab, "one:x");
}

#[test]
fn a_close_and_a_stop_release_every_watch() {
    let rig = Rig::new();
    let (t1, t2) = (rig.open("exec sleep 30"), rig.open("exec sleep 30"));
    let seq_of = |tab: &str| rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0})).unwrap()["seq"].as_u64().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let (s1, s2) = (seq_of(&t1), seq_of(&t2));
    let (a, b, c) = (rig.watching(&t1, s1), rig.watching(&t1, s1), rig.watching(&t2, s2));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!a.is_finished() && !b.is_finished() && !c.is_finished());
    rig.call(json!({"op": "terminal.close", "tab": t1})).unwrap();
    assert_eq!(a.join().unwrap(), Err("unknown_tab"));
    assert_eq!(b.join().unwrap(), Err("unknown_tab"));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!c.is_finished(), "the other terminal's watch is not released");
    crate::driver::Ops::stop(&*rig.terminals); // the host stops
    assert_eq!(c.join().unwrap(), Err("unknown_tab"));
    assert_eq!(rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"].as_array().unwrap().len(), 0);
}

#[test]
fn a_watch_tells_the_screen_scrolled_back_and_a_program_that_fills_the_screen_makes_a_scrollback() {
    let rig = Rig::new();
    let tab = rig.open("i=1; while [ $i -le 60 ]; do echo line$i; i=$((i+1)); done; exec sleep 30");
    let w = rig.shows(&tab, "line60");
    assert_eq!((w["scroll"].clone(), w["total"].clone(), w["top"].clone()), (json!(0), json!(24 + 37), json!(37)), "61 lines (60 and the cursor line) on 24 rows");
    assert!(screen(&w).starts_with("line38\n") || screen(&w).starts_with("line37\n"), "{}", screen(&w));
    let back = rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0, "scroll": 10})).unwrap();
    assert_eq!((back["scroll"].clone(), back["top"].clone()), (json!(10), json!(27)));
    assert_eq!(back["cursor"], Value::Null, "the cursor is not shown while the view is away from the bottom");
    let clamped = rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0, "scroll": 100_000})).unwrap();
    assert_eq!(clamped["scroll"], json!(37), "the scrollback is what it is");
    assert!(screen(&clamped).starts_with("line1\n"));
}

#[test]
fn what_the_emulator_is_asked_is_answered_to_the_program() {
    let rig = Rig::new();
    // the program asks where the cursor is, reads the six bytes of the answer without the line discipline, and shows them
    let tab = rig.open("printf '\\033[6n'; stty -echo -icanon; a=$(dd bs=1 count=6 2>/dev/null | od -An -c); stty sane; echo \"answer:$a\"; exec sleep 30");
    let w = rig.shows(&tab, "answer:");
    let text = screen(&w).split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(text.contains("answer: 033 [ 1 ; 1 R"), "{text}");
}

#[test]
fn input_comes_as_text_or_as_bytes_and_a_program_that_never_reads_makes_it_busy() {
    let rig = Rig::new();
    let tab = rig.open("stty raw -echo; printf 'ready\\r\\n'; a=$(dd bs=1 count=3 2>/dev/null | od -An -tx1); printf 'bytes:%s\\r\\n' \"$a\"; exec sleep 30");
    rig.shows(&tab, "ready");
    let r = rig.call(json!({"op": "terminal.input", "tab": tab, "data": "AP8Q"})).unwrap(); // 00 ff 10
    assert_eq!(r["written"], json!(3));
    rig.shows(&tab, "bytes: 00 ff 10");
    // now nobody reads: after what the pty holds, a write waits the patience (two seconds) and says busy
    let chunk = "x".repeat(ops::MAX_INPUT);
    let started = Instant::now();
    let mut busy = None;
    for _ in 0..80 {
        match rig.call(json!({"op": "terminal.input", "tab": tab, "text": chunk})) {
            Ok(r) => assert_eq!(r["written"], json!(ops::MAX_INPUT)),
            Err(e) => {
                busy = Some(e);
                break;
            }
        }
    }
    assert_eq!(busy, Some("busy"), "after {:?}", started.elapsed());
    assert!(started.elapsed() >= Duration::from_millis(1900), "it waited its patience first: {:?}", started.elapsed());
}

#[test]
fn text_returns_the_selection_between_two_places() {
    let rig = Rig::new();
    let tab = rig.open("printf 'alpha beta\\ngamma\\n'; exec sleep 30");
    rig.shows(&tab, "gamma");
    let text = |from: Value, to: Value| rig.call(json!({"op": "terminal.text", "tab": tab, "from": from, "to": to})).unwrap()["text"].clone();
    assert_eq!(text(json!([0, 6]), json!([1, 4])), json!("beta\ngamma"));
    assert_eq!(text(json!([1, 4]), json!([0, 6])), json!("beta\ngamma"), "either order");
    assert_eq!(text(json!([0, 0]), json!([0, 4])), json!("alpha"));
    assert_eq!(text(json!([500, 0]), json!([501, 0])), json!(""), "a line that does not exist");
    assert_eq!(rig.call(json!({"op": "terminal.text", "tab": "t99", "from": [0, 0], "to": [0, 1]})), Err("unknown_tab"));
}

#[test]
fn a_terminal_that_nobody_watches_does_not_grow_and_the_unknown_tab_is_named() {
    let rig = Rig::new();
    for op in [
        json!({"op": "terminal.close", "tab": "t42"}),
        json!({"op": "terminal.input", "tab": "t42", "text": "x"}),
        json!({"op": "terminal.resize", "tab": "t42", "cols": 10, "rows": 10}),
        json!({"op": "terminal.watch", "tab": "t42"}),
    ] {
        assert_eq!(rig.call(op.clone()), Err("unknown_tab"), "{op}");
    }
    assert_eq!(rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"], json!([]));
}

#[test]
fn the_socket_opens_a_terminal_and_the_http_door_never_does() {
    let rig = Rig::new();
    super::register();
    let dir = std::env::temp_dir().join(format!("lico_term_hubs_{}", crate::core::random_hex(6)));
    let shared = crate::driver_hubs_tests::shared(&dir);
    let shell = rig.script("echo from-the-host; exec sleep 30");
    let open = json!({"op": "terminal.open", "cwd": rig.dir.to_string_lossy(), "shell": shell});
    assert!(crate::ipc::valid_request("terminal.open", &req(open.clone())));
    // POST /v1: refused as a bad request, before anything starts
    for op in ["terminal.open", "terminal.list", "terminal.close", "terminal.input", "terminal.resize", "terminal.watch", "terminal.text"] {
        assert_eq!(crate::ipc::dispatch_request(&shared, op, &req(json!({"op": op})), true), Some(Err("bad_request")), "{op}");
    }
    assert_eq!(crate::ipc::dispatch_request(&shared, "terminal.list", &req(json!({"op": "terminal.list"})), false).unwrap().unwrap()["terminals"], json!([]), "nothing was started by the refused doors");
    // the socket: the same operations, with the host's own folder check
    let opened = crate::ipc::dispatch_request(&shared, "terminal.open", &req(open), false).unwrap().unwrap();
    let tab = opened["tab"].as_str().unwrap().to_string();
    let watch = crate::ipc::dispatch_request(&shared, "terminal.watch", &req(json!({"op": "terminal.watch", "tab": tab, "seq": 0})), false).unwrap().unwrap();
    assert_eq!(watch["tab"], json!(tab));
    // the host's check refuses a folder of the host's own state
    let own = shared.cfg.registry_dir.to_string_lossy().into_owned();
    std::fs::create_dir_all(&own).unwrap();
    assert_eq!(crate::ipc::dispatch_request(&shared, "terminal.open", &req(json!({"op": "terminal.open", "cwd": own, "shell": shell})), false), Some(Err("folder_protected")));
    assert_eq!(crate::ipc::dispatch_request(&shared, "terminal.close", &req(json!({"op": "terminal.close", "tab": tab})), false).unwrap().unwrap()["closed"], json!(true));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------------------------------------------- what a close and an end leave

fn threads_named(prefix: &str) -> usize {
    std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(|e| std::fs::read_to_string(e.ok()?.path().join("comm")).ok())
        .filter(|name| name.starts_with(prefix))
        .count()
}

fn marker_of(rig: &Rig) -> PathBuf {
    rig.dir.join("hup.marker")
}

#[test]
fn a_signal_reaches_a_group_only_while_the_pid_is_the_process_that_was_noted_and_leads_its_group() {
    use std::os::unix::process::CommandExt;
    let mut leader = std::process::Command::new("sleep").arg("30").process_group(0).spawn().unwrap(); // leads a group of its own
    let mut follower = std::process::Command::new("sleep").arg("30").spawn().unwrap(); // is in this test's group
    let (lp, fp) = (leader.id(), follower.id());
    let (ls, fs) = (crate::procs::start_time(lp).unwrap(), crate::procs::start_time(fp).unwrap());
    assert!(!crate::procs::signal_group(lp, "1", 9), "another start time: the pid is not the process that was noted");
    assert!(alive(i64::from(lp)));
    assert!(!crate::procs::signal_group(fp, &fs, 9), "it does not lead its group: a signal to that group would reach this test as well");
    assert!(alive(i64::from(fp)));
    assert!(crate::procs::signal_group(lp, &ls, 9));
    leader.wait().unwrap();
    assert!(!crate::procs::signal_group(lp, &ls, 9), "a process that is gone is not signalled");
    follower.kill().unwrap();
    follower.wait().unwrap();
}

#[test]
fn a_close_hangs_the_group_up_first_and_wakes_a_stopped_shell_to_hear_it() {
    let rig = Rig::new();
    let marker = marker_of(&rig);
    let tab = rig.open(&format!("trap 'echo hup > {}; exit 0' HUP\nsleep 300 &\nprintf 'ready\\n'\nwait", marker.display()));
    rig.shows(&tab, "ready");
    rig.call(json!({"op": "terminal.close", "tab": tab})).unwrap();
    eventually("the hang-up reached the shell (the kill comes after a second and leaves no marker)", Duration::from_millis(700), || marker.exists());
    // a shell that stopped itself gets the same hang-up: the group is woken with it
    std::fs::remove_file(&marker).unwrap();
    let stopped = rig.open(&format!("trap 'echo hup > {}; exit 0' HUP\nprintf 'ready\\n'\nkill -STOP $$\nsleep 300", marker.display()));
    rig.shows(&stopped, "ready");
    let pid = rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"][0]["pid"].as_i64().unwrap();
    eventually("the shell stopped", Duration::from_secs(3), || std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| s.rsplit_once(") ").is_some_and(|(_, r)| r.starts_with('T'))));
    rig.call(json!({"op": "terminal.close", "tab": stopped})).unwrap();
    eventually("the stopped shell heard the hang-up", Duration::from_millis(700), || marker.exists());
}

#[test]
fn input_to_a_shell_that_ended_is_refused_even_while_a_program_it_started_holds_the_pty() {
    let rig = Rig::new();
    let holder = rig.dir.join("holder.pid");
    let tab = rig.open(&format!("trap '' HUP\nsleep 8 &\necho $! > {}\nexit 1", holder.display()));
    rig.until(&tab, "the end", |w| w["running"] == json!(false));
    assert_eq!(rig.call(json!({"op": "terminal.input", "tab": tab, "text": "x"})), Err("terminal_ended"), "the pty is still open on the other side, and nothing reads it");
    if let Ok(pid) = std::fs::read_to_string(&holder) {
        let _ = std::process::Command::new("kill").args(["-9", pid.trim()]).status(); // the program that held the pty is not left to run out its eight seconds
    }
}

#[test]
fn the_reader_stops_after_the_shell_ended_and_a_pause_but_the_terminal_stays_listed() {
    let rig = Rig::new();
    eventually("no reader of an earlier test", Duration::from_secs(5), || threads_named("term-read") == 0);
    // a program the shell started holds the pty (it ignores the hang-up the shell's end brings), so the pty never reports its end: the reader stops after a pause with no output
    let holder = rig.dir.join("holder.pid");
    let tab = rig.open(&format!("trap '' HUP\nsleep 8 &\necho $! > {}\necho done\nexit 0", holder.display()));
    rig.until(&tab, "the end", |w| w["running"] == json!(false));
    assert!(threads_named("term-read") == 1, "the reader is still there just after the end");
    eventually("the reader and the waiter are gone, with nothing closed", Duration::from_secs(3), || threads_named("term-read") == 0 && threads_named("term-wait") == 0);
    assert_eq!(rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"].as_array().unwrap().len(), 1);
    let live = rig.open("exec sleep 30");
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(threads_named("term-read"), 1, "the reader of a running shell goes on (it is for {live})");
    if let Ok(pid) = std::fs::read_to_string(&holder) {
        let _ = std::process::Command::new("kill").args(["-9", pid.trim()]).status(); // the program that held the pty is not left to run out its eight seconds
    }
}

#[test]
fn a_close_releases_the_watches_at_once_even_when_the_shell_is_slow_to_end() {
    let rig = Rig::new();
    let tab = rig.open("trap '' HUP\nsleep 300 &\nprintf 'ready\\n'\nwait");
    let seq = rig.shows(&tab, "ready")["seq"].as_u64().unwrap();
    let shell = rig.call(json!({"op": "terminal.list"})).unwrap()["terminals"][0]["pid"].as_i64().unwrap();
    let waiting = rig.watching(&tab, seq);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!waiting.is_finished());
    rig.call(json!({"op": "terminal.close", "tab": tab})).unwrap();
    eventually("the watch is released by the close itself, not by the shell's end a second later", Duration::from_millis(500), || waiting.is_finished());
    assert_eq!(waiting.join().unwrap(), Err("unknown_tab"));
    eventually("the kill after the second ends the group, so that the test leaves nothing behind", Duration::from_secs(4), || !alive(shell));
}

mod agent_tests;
