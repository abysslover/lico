//! The contract tests of `lico-convo` (SPEC.md). The record lives in a scratch folder under the build's temporary directory; nothing else on the disk is touched.

use lico_convo::*;
use lico_llm_wire::{Message, ToolCall};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("convo").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn mode(p: &std::path::Path) -> u32 {
    fs::metadata(p).unwrap().permissions().mode() & 0o777
}

fn user(t: &str) -> Message {
    Message::User(t.into())
}

fn answer(t: &str, calls: Vec<ToolCall>) -> Message {
    Message::Assistant { text: t.into(), calls }
}

fn call(id: &str, name: &str, args: Value) -> ToolCall {
    ToolCall { id: id.into(), name: name.into(), arguments: args }
}

fn tool(id: &str, name: &str, content: &str) -> Message {
    Message::Tool { call_id: id.into(), name: name.into(), content: content.into() }
}

fn sample() -> Vec<Message> {
    vec![
        user("hello 한국어 😀 \"quoted\" \\ back \n newline \u{0} nul \u{1b}[31m"),
        answer("one moment", vec![call("call_1", "sessions", json!({"q": "한국", "n": [1, 2, {"deep": null}], "f": 1.5}))]),
        tool("call_1", "sessions", "[]"),
        answer("done", vec![]),
    ]
}

// ---- open, append, read -----------------------------------------------------------------------------------------------------------

#[test]
fn every_kind_of_message_survives_a_close_and_open_and_the_count_is_reported() {
    let dir = scratch("roundtrip");
    let path = dir.join("rec").join("a.jsonl");
    let (mut c, opened) = Convo::open(&path).unwrap();
    assert_eq!(opened, Opened { loaded: 0, repaired_bytes: 0, created: true });
    for m in sample() {
        c.append(&m).unwrap();
    }
    assert_eq!(c.messages(), &sample()[..]);
    drop(c);
    let (c, opened) = Convo::open(&path).unwrap();
    assert_eq!(opened, Opened { loaded: 4, repaired_bytes: 0, created: false });
    assert_eq!(c.messages(), &sample()[..]);
}

#[test]
fn the_directory_is_owner_only_and_so_is_the_file_and_an_existing_directory_is_left_alone() {
    let dir = scratch("modes");
    Convo::open(&dir.join("x").join("y").join("a.jsonl")).unwrap();
    assert_eq!((mode(&dir.join("x")), mode(&dir.join("x/y")), mode(&dir.join("x/y/a.jsonl"))), (0o700, 0o700, 0o600));
    let existing = dir.join("open");
    fs::create_dir_all(&existing).unwrap();
    fs::set_permissions(&existing, fs::Permissions::from_mode(0o755)).unwrap();
    Convo::open(&existing.join("b.jsonl")).unwrap();
    assert_eq!(mode(&existing), 0o755, "a directory that was there is not changed");
    assert_eq!(mode(&existing.join("b.jsonl")), 0o600);
}

#[test]
fn a_symlink_a_directory_and_a_file_open_to_others_are_refused_and_nothing_is_changed() {
    let dir = scratch("refuse");
    let real = dir.join("real.jsonl");
    Convo::open(&real).unwrap();
    symlink(&real, dir.join("link.jsonl")).unwrap();
    assert!(matches!(Convo::open(&dir.join("link.jsonl")), Err(Error::NotARegularFile)));
    fs::create_dir_all(dir.join("adir")).unwrap();
    assert!(matches!(Convo::open(&dir.join("adir")), Err(Error::NotARegularFile)));
    for loose in [0o644u32, 0o640, 0o604, 0o660, 0o666, 0o700 | 0o001] {
        let p = dir.join(format!("loose_{loose:o}.jsonl"));
        fs::write(&p, "").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(loose)).unwrap();
        assert!(matches!(Convo::open(&p), Err(Error::NotOwnerOnly)), "{loose:o}");
        assert_eq!(mode(&p), loose, "the file is not tightened behind the owner's back");
    }
    for ok in [0o600u32, 0o400, 0o700] {
        let p = dir.join(format!("ok_{ok:o}.jsonl"));
        fs::write(&p, "").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(ok)).unwrap();
        if ok != 0o400 {
            assert!(Convo::open(&p).is_ok(), "{ok:o}");
        }
    }
}

#[test]
fn a_system_message_is_refused_and_nothing_is_written() {
    let dir = scratch("system");
    let path = dir.join("a.jsonl");
    let (mut c, _) = Convo::open(&path).unwrap();
    assert_eq!(c.append(&Message::System("be brief".into())), Err(Error::SystemNotStored));
    assert_eq!(fs::metadata(&path).unwrap().len(), 0);
    assert!(c.messages().is_empty());
}

#[test]
fn the_file_holds_only_the_three_line_shapes_with_only_their_keys() {
    let dir = scratch("keys");
    let path = dir.join("a.jsonl");
    let (mut c, _) = Convo::open(&path).unwrap();
    for m in sample() {
        c.append(&m).unwrap();
    }
    c.append(&answer("plain", vec![])).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.ends_with('\n') && !text.contains("\n\n"));
    let keys = |v: &Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 5);
    assert_eq!((keys(&lines[0]), lines[0]["v"].clone(), lines[0]["t"].clone()), (vec!["t", "text", "v"].into_iter().map(String::from).collect::<Vec<_>>(), json!(1), json!("user")));
    assert_eq!(keys(&lines[1]), ["calls", "t", "text", "v"]);
    assert_eq!(keys(&lines[1]["calls"][0]), ["arguments", "id", "name"]);
    assert_eq!(keys(&lines[2]), ["call_id", "content", "name", "t", "v"]);
    assert_eq!(keys(&lines[3]), ["t", "text", "v"], "an answer without calls has no calls key");
    assert_eq!(keys(&lines[4]), ["t", "text", "v"]);
}

// ---- C2: a torn last line, C1/C6: damage ---------------------------------------------------------------------------------------------

#[test]
fn a_torn_last_line_is_cut_at_every_byte_and_costs_at_most_itself() {
    let dir = scratch("torn");
    let path = dir.join("a.jsonl");
    {
        let (mut c, _) = Convo::open(&path).unwrap();
        for m in sample().into_iter().take(3) {
            c.append(&m).unwrap();
        }
    }
    let full = fs::read(&path).unwrap();
    let last_start = full[..full.len() - 1].iter().rposition(|b| *b == b'\n').unwrap() + 1;
    for cut in last_start..full.len() {
        let p = dir.join(format!("cut_{cut}.jsonl"));
        fs::write(&p, &full[..cut]).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        let (mut c, opened) = Convo::open(&p).unwrap();
        let complete_without_newline = cut == full.len() - 1;
        if complete_without_newline {
            assert_eq!((opened.loaded, opened.repaired_bytes), (3, 0), "a whole message that only lacks its newline is kept");
        } else {
            assert_eq!((opened.loaded, opened.repaired_bytes), (2, (cut - last_start) as u64), "cut at {cut}");
        }
        c.append(&user("after the repair")).unwrap();
        drop(c);
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.ends_with('\n') && text.lines().count() == opened.loaded + 1, "cut at {cut}: {text:?}");
        let (c, again) = Convo::open(&p).unwrap();
        assert_eq!((again.loaded, again.repaired_bytes, c.messages().last()), (opened.loaded + 1, 0, Some(&user("after the repair"))));
    }
}

#[test]
fn damage_in_the_middle_is_never_guessed_at_and_is_named_by_its_line() {
    let dir = scratch("corrupt");
    let good = |t: &str| format!("{{\"v\":1,\"t\":\"user\",\"text\":\"{t}\"}}\n");
    let cases: Vec<(&str, String, usize)> = vec![
        ("garbage", format!("{}not json\n{}", good("a"), good("b")), 2),
        ("blank line", format!("{}\n{}", good("a"), good("b")), 2),
        ("version 2", format!("{}{{\"v\":2,\"t\":\"user\",\"text\":\"x\"}}\n{}", good("a"), good("b")), 2),
        ("unknown kind", format!("{{\"v\":1,\"t\":\"system\",\"text\":\"x\"}}\n{}", good("b")), 1),
        ("unknown key", format!("{}{{\"v\":1,\"t\":\"user\",\"text\":\"x\",\"secret\":\"k\"}}\n{}", good("a"), good("b")), 2),
        ("missing field", format!("{}{{\"v\":1,\"t\":\"tool\",\"call_id\":\"c\",\"name\":\"n\"}}\n{}", good("a"), good("b")), 2),
        ("wrong type", format!("{}{{\"v\":1,\"t\":\"user\",\"text\":5}}\n{}", good("a"), good("b")), 2),
        ("not an object", format!("{}[1,2]\n{}", good("a"), good("b")), 2),
        ("call with an extra key", format!("{}{{\"v\":1,\"t\":\"assistant\",\"text\":\"\",\"calls\":[{{\"id\":\"a\",\"name\":\"n\",\"arguments\":{{}},\"x\":1}}]}}\n{}", good("a"), good("b")), 2),
        ("bad call", format!("{}{{\"v\":1,\"t\":\"assistant\",\"text\":\"\",\"calls\":[{{\"id\":\"a\"}}]}}\n{}", good("a"), good("b")), 2),
    ];
    for (name, body, line) in cases {
        let p = dir.join(format!("{}.jsonl", name.replace(' ', "_")));
        fs::write(&p, &body).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(Convo::open(&p), Err(Error::Corrupt { line: l }) if l == line), "{name}");
        assert_eq!(fs::read_to_string(&p).unwrap(), body, "{name}: a damaged file is left as it is");
    }
}

#[test]
fn random_file_content_never_panics() {
    let dir = scratch("random");
    let mut seed = 0x1234_5678_9ABC_DEF1u64;
    let valid = b"{\"v\":1,\"t\":\"user\",\"text\":\"hi\"}\n{\"v\":1,\"t\":\"assistant\",\"text\":\"x\"}\n".to_vec();
    for i in 0..300 {
        let n = (seed % 300) as usize;
        let mut bytes: Vec<u8> = if i % 2 == 0 { valid.clone() } else { Vec::new() };
        for _ in 0..n {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let b = (seed >> 24) as u8;
            if i % 3 == 0 && !bytes.is_empty() {
                let at = (seed as usize >> 8) % bytes.len();
                bytes[at] = b;
            } else {
                bytes.push(b);
            }
        }
        let p = dir.join(format!("r{i}.jsonl"));
        fs::write(&p, &bytes).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        let _ = Convo::open(&p);
    }
}

// ---- limits ----------------------------------------------------------------------------------------------------------------------------

#[test]
fn a_line_over_the_limit_and_a_file_over_the_limit_are_refused_exactly_at_their_edges() {
    let dir = scratch("limits");
    let path = dir.join("a.jsonl");
    let (mut c, _) = Convo::open(&path).unwrap();
    assert_eq!(c.append(&user(&"x".repeat(MAX_LINE_BYTES))), Err(Error::TooLarge));
    assert_eq!(fs::metadata(&path).unwrap().len(), 0, "a refused message writes nothing");
    let line_overhead = serde_json::to_string(&json!({"v": 1, "t": "user", "text": ""})).unwrap().len() + 1;
    c.append(&user(&"y".repeat(MAX_LINE_BYTES - line_overhead))).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().len(), MAX_LINE_BYTES as u64, "a line of exactly the limit is accepted");
    assert_eq!(c.append(&user(&"y".repeat(MAX_LINE_BYTES - line_overhead + 1))), Err(Error::TooLarge), "one byte over the line limit");
    let big = user(&"z".repeat(MAX_LINE_BYTES - line_overhead));
    for _ in 0..6 {
        c.append(&big).unwrap();
    }
    assert_eq!(fs::metadata(&path).unwrap().len(), 7 * MAX_LINE_BYTES as u64);
    let rest = MAX_FILE_BYTES - 7 * MAX_LINE_BYTES as u64;
    let fill = user(&"w".repeat(rest as usize - line_overhead));
    c.append(&fill).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().len(), MAX_FILE_BYTES, "the file may be exactly the limit");
    assert_eq!(c.append(&user("one more")), Err(Error::Full));
    drop(c);
    assert!(Convo::open(&path).is_ok(), "a file of exactly the limit opens");
    let over = dir.join("over.jsonl");
    fs::write(&over, vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
    fs::set_permissions(&over, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(Convo::open(&over), Err(Error::Full)));
}

// ---- fit -------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_estimate_is_bytes_over_three_and_a_half_rounded_up_plus_four() {
    assert_eq!(estimate_tokens(&user("")), 4);
    assert_eq!(estimate_tokens(&user("abcdefg")), 6, "7 bytes: 2 tokens + 4");
    assert_eq!(estimate_tokens(&user("abcdefgh")), 7, "8 bytes: 3 tokens (rounded up) + 4");
    assert_eq!(estimate_tokens(&user("한")), 4 + 1, "3 bytes: 1 token");
    assert_eq!(estimate_tokens(&tool("ab", "cd", "efg")), 4 + 2, "7 bytes of id, name and content: 2 tokens + 4");
    let with_call = answer("ab", vec![call("i", "nm", json!({"k": 1}))]);
    let bytes = 2 + 1 + 2 + "{\"k\":1}".len();
    assert_eq!(estimate_tokens(&with_call) as usize, 4 + (bytes * 2).div_ceil(7));
    assert_eq!(estimate_tokens(&Message::System("abcdefg".into())), 6);
    assert_eq!(estimate_tokens(&user("same")), estimate_tokens(&user("same")));
}

/// Three turns: A (user, answer), B (user, answer with a call, its result, answer), C (user, answer).
fn turns() -> Vec<Message> {
    vec![
        user(&"a".repeat(70)),
        answer(&"a".repeat(70), vec![]),
        user(&"b".repeat(70)),
        answer("", vec![call("c1", "sessions", json!({}))]),
        tool("c1", "sessions", &"r".repeat(70)),
        answer(&"b".repeat(70), vec![]),
        user(&"c".repeat(70)),
        answer(&"c".repeat(70), vec![]),
    ]
}

fn total(ms: &[Message]) -> u32 {
    ms.iter().map(estimate_tokens).sum()
}

#[test]
fn whole_turns_are_dropped_oldest_first_and_the_newest_is_always_kept() {
    let m = turns();
    let (a, b, c) = (total(&m[0..2]), total(&m[2..6]), total(&m[6..8]));
    assert_eq!(fit(&m, a + b + c), Fitted { start: 0, dropped_turns: 0, tokens: a + b + c, too_big: false }, "everything fits at the exact budget");
    assert_eq!(fit(&m, a + b + c - 1), Fitted { start: 2, dropped_turns: 1, tokens: b + c, too_big: false }, "one token short: the oldest turn goes");
    assert_eq!(fit(&m, b + c), Fitted { start: 2, dropped_turns: 1, tokens: b + c, too_big: false });
    assert_eq!(fit(&m, c), Fitted { start: 6, dropped_turns: 2, tokens: c, too_big: false }, "a call and its result go together: turn B is dropped whole");
    assert_eq!(fit(&m, c + 1).start, 6);
    let tight = fit(&m, c - 1);
    assert_eq!(tight, Fitted { start: 6, dropped_turns: 2, tokens: c, too_big: true }, "the newest turn alone is over the budget: kept, and said");
    assert_eq!(fit(&m, 0), Fitted { start: 6, dropped_turns: 2, tokens: c, too_big: true });
}

#[test]
fn no_cut_ever_separates_a_call_from_its_result() {
    let m = turns();
    for budget in 0..(total(&m) + 5) {
        let f = fit(&m, budget);
        assert!(matches!(m[f.start], Message::User(_)), "a window starts at a person's message: budget {budget} start {}", f.start);
        assert_eq!(f.tokens, total(&m[f.start..]));
        assert!(f.tokens <= budget || f.too_big);
        assert!(f.start == 0 || f.dropped_turns > 0);
    }
}

#[test]
fn fit_of_nothing_a_single_turn_and_leading_answers_is_defined() {
    assert_eq!(fit(&[], 100), Fitted { start: 0, dropped_turns: 0, tokens: 0, too_big: false });
    let one = vec![user("hi"), answer("yo", vec![])];
    assert_eq!(fit(&one, 1000).start, 0);
    assert!(fit(&one, 1).too_big && fit(&one, 1).start == 0 && fit(&one, 1).dropped_turns == 0);
    let leading = vec![answer("orphan", vec![]), user("q"), answer("a", vec![])];
    assert_eq!(fit(&leading, 1000), Fitted { start: 0, dropped_turns: 0, tokens: total(&leading), too_big: false }, "what comes before the first message of the person is the first turn");
    assert_eq!(fit(&leading, total(&leading[1..])).start, 1);
}

// ---- the public names, the limits and what the crate does not use ----------------------------------------------------------------------

#[test]
fn the_public_names_are_exactly_these_and_the_limits_have_their_values() {
    let source = include_str!("../src/lib.rs");
    let mut found: Vec<String> = Vec::new();
    for line in source.lines() {
        if line.starts_with("pub ") {
            let rest = line.trim_start_matches("pub ").trim_start_matches("const ").trim_start_matches("fn ").trim_start_matches("struct ").trim_start_matches("enum ");
            found.push(rest.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap().to_string());
        }
    }
    found.sort();
    let mut expected: Vec<String> = ["MAX_LINE_BYTES", "MAX_FILE_BYTES", "Error", "Opened", "Convo", "Fitted", "estimate_tokens", "fit"].iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(found, expected);
    assert_eq!(MAX_LINE_BYTES, 1024 * 1024);
    assert_eq!(MAX_FILE_BYTES, 8 * 1024 * 1024);
}

#[test]
fn the_crate_depends_on_the_wire_types_and_serde_json_only_and_touches_no_network_process_or_environment() {
    let manifest = include_str!("../Cargo.toml");
    let deps: Vec<&str> = manifest.split("[dependencies]").nth(1).unwrap().lines().skip(1).take_while(|l| !l.starts_with('[')).filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(deps.len(), 2, "{deps:?}");
    assert!(deps.iter().any(|d| d.starts_with("lico-llm-wire ")) && deps.iter().any(|d| d.starts_with("serde_json ")), "{deps:?}");
    for entry in fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = entry.unwrap().path();
        let code = fs::read_to_string(&path).unwrap();
        for (n, line) in code.lines().enumerate() {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            for word in ["std::net", "std::process", "std::env", "std::thread", "std::time", "unsafe", "Instant", "SystemTime", "reasoning", "api_key", "Authorization"] {
                assert!(!line.contains(word), "{}:{}: `{word}` in {line}", path.display(), n + 1);
            }
        }
    }
}
