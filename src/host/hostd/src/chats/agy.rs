//! The conversations of `agy` (M23-P04b): the file `agy` keeps of the prompts typed in it, `~/.gemini/antigravity-cli/history.jsonl`, one JSON object per prompt with `display` (the text),
//! `timestamp` (milliseconds), `workspace` (the folder it ran in) and, for most lines, `conversationId`. The host reads it as it reads the records of the other providers: read-only, a
//! regular file of the person's own with no group or other permission bit, bounded, and a line that is not what it should be is skipped. A conversation is the lines of one id; the only
//! text of it that is kept is the first prompt's line, as its title.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{clean_title, norm_path, ChatItem};
use crate::folders::derive_auto_title;
use crate::provider_record::valid_provider_session;

/// The most bytes of the file that are read: the end of it (the newest prompts) when it is longer.
pub(super) const MAX_FILE: u64 = 8 * 1024 * 1024;
/// The longest line that is looked at, in bytes; a longer one is skipped.
pub(super) const MAX_LINE: usize = 16 * 1024;
/// How many conversations are listed: the newest.
pub(super) const MOST: usize = 200;
/// The longest folder kept, in bytes.
const MAX_FOLDER: usize = 1024;

const O_NOFOLLOW: i32 = 0o400_000;
const O_CLOEXEC: i32 = 0o2_000_000;

/// One conversation as the file tells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Conversation {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) cwd: Option<String>,
    pub(super) updated: u64,
}

/// Where `agy` keeps its prompts, under the person's home.
pub(super) fn history_path(home: &Path) -> PathBuf {
    home.join(".gemini").join("antigravity-cli").join("history.jsonl")
}

/// The person's home from the environment of the host, when it is an absolute path.
pub(super) fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute())
}

/// Whether a file may be read: a regular file of `me`'s own (a user id) with no group or other permission bit.
fn readable(meta: &std::fs::Metadata, me: u32) -> bool {
    meta.file_type().is_file() && meta.uid() == me && meta.permissions().mode() & 0o077 == 0
}

/// The file's text: the end of it when it is longer than `MAX_FILE` (a line cut at the start of what is read is no JSON object, so `load` skips it like any damaged line). Nothing when the
/// file is a link, a folder, someone else's or has a group or other permission bit.
fn read(path: &Path) -> Option<String> {
    let mut file = std::fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_CLOEXEC).open(path).ok()?;
    let meta = file.metadata().ok()?;
    let me = std::fs::metadata("/proc/self").ok()?.uid();
    if !readable(&meta, me) {
        return None;
    }
    let cut = meta.len() > MAX_FILE;
    if cut {
        file.seek(SeekFrom::Start(meta.len() - MAX_FILE)).ok()?;
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The time of a prompt in seconds: the file counts milliseconds, and a number that is plainly seconds is taken as it is.
fn seconds(timestamp: u64) -> u64 {
    if timestamp >= 100_000_000_000 {
        timestamp / 1000
    } else {
        timestamp
    }
}

/// A folder as a conversation keeps it: an absolute path without control characters, not longer than `MAX_FOLDER`, without a trailing slash.
fn folder(raw: &str) -> Option<String> {
    (raw.starts_with('/') && raw.len() <= MAX_FOLDER && !raw.chars().any(char::is_control)).then(|| norm_path(raw).to_string())
}

/// The conversations in `home`'s history of `agy`, newest first, at most `MOST`; empty when there is no file that may be read.
pub(super) fn load(home: &Path) -> Vec<Conversation> {
    let Some(text) = read(&history_path(home)) else { return Vec::new() };
    let mut found: HashMap<String, Conversation> = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty() && l.len() <= MAX_LINE) {
        let Ok(Value::Object(entry)) = serde_json::from_str::<Value>(line) else { continue };
        let (Some(display), Some(timestamp), Some(id)) = (entry.get("display").and_then(Value::as_str), entry.get("timestamp").and_then(Value::as_u64), entry.get("conversationId").and_then(Value::as_str)) else { continue };
        if !valid_provider_session(id) {
            continue;
        }
        let slash = entry.get("type").and_then(Value::as_str) == Some("slash_command");
        let time = seconds(timestamp);
        let place = entry.get("workspace").and_then(Value::as_str).and_then(folder);
        let conversation = found.entry(id.to_string()).or_insert_with(|| Conversation { id: id.to_string(), title: String::new(), cwd: None, updated: 0 });
        if conversation.title.is_empty() && !slash {
            conversation.title = derive_auto_title(display).unwrap_or_default();
        }
        if time >= conversation.updated {
            conversation.updated = time;
            conversation.cwd = place.or_else(|| conversation.cwd.take()); // a last line with no usable folder keeps the folder an earlier one had
        }
    }
    let mut list: Vec<Conversation> = found.into_values().collect();
    list.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
    list.truncate(MOST);
    list
}

/// A conversation as an item of the list: the provider `agy`, no account, no open session, the folder it ran in.
pub(super) fn item(c: Conversation) -> ChatItem {
    let title = clean_title(&c.title, "agy", &c.id);
    ChatItem { provider: "agy".into(), provider_session: c.id, account: None, account_label: None, ssh_host: None, cwd: c.cwd, global: false, title, updated_at: c.updated, open_session: None, open_state: None, file: None, turns: None, recorded: true, live: None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A private folder under the system's temporary one that is removed when it goes out of scope.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Scratch {
            let dir = std::env::temp_dir().join(format!("lico_agy_{}", crate::core::random_hex(8)));
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const A: &str = "11111111-aaaa-4bbb-8ccc-000000000001";
    const B: &str = "22222222-aaaa-4bbb-8ccc-000000000002";

    /// A home with the file of `lines` (each a JSON value or a raw line), mode 0600.
    fn home_with(lines: &[String]) -> (Scratch, PathBuf) {
        let dir = Scratch::new();
        let file = history_path(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, lines.join("\n") + "\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        (dir, file)
    }

    fn line(display: &str, ts: u64, ws: &str, id: Option<&str>) -> String {
        let mut v = json!({"display": display, "timestamp": ts, "workspace": ws});
        if let Some(id) = id {
            v["conversationId"] = json!(id);
        }
        v.to_string()
    }

    #[test]
    fn a_conversation_is_the_lines_of_one_id_with_the_first_prompt_as_title_the_last_folder_and_the_last_time_newest_first() {
        let (home, _) = home_with(&[
            line("first question of A", 1_700_000_001_000, "/w/one", Some(A)),
            line("only question of B", 1_700_000_050_000, "/w/two", Some(B)),
            line("second question of A", 1_700_000_100_000, "/w/one/inner", Some(A)),
        ]);
        let got = load(home.path());
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].id.as_str(), got[0].title.as_str(), got[0].cwd.as_deref(), got[0].updated), (A, "first question of A", Some("/w/one/inner"), 1_700_000_100), "the newest first: the first prompt is the title, the last folder and time");
        assert_eq!((got[1].id.as_str(), got[1].title.as_str(), got[1].cwd.as_deref(), got[1].updated), (B, "only question of B", Some("/w/two"), 1_700_000_050));
    }

    #[test]
    fn a_time_that_is_plainly_in_seconds_is_taken_as_seconds() {
        let (home, _) = home_with(&[line("q", 1_700_000_123, "/w", Some(A))]);
        assert_eq!(load(home.path())[0].updated, 1_700_000_123);
    }

    #[test]
    fn a_slash_command_is_never_the_title_and_a_conversation_of_nothing_else_has_the_id_for_a_title() {
        let mut cmd = json!({"display": "/model", "timestamp": 1_700_000_001_000u64, "workspace": "/w", "conversationId": A, "type": "slash_command"});
        let (home, _) = home_with(&[cmd.to_string(), line("the real question", 1_700_000_002_000, "/w", Some(A)), {
            cmd["conversationId"] = json!(B);
            cmd.to_string()
        }]);
        let got = load(home.path());
        let a = got.iter().find(|c| c.id == A).unwrap();
        assert_eq!(a.title, "the real question", "the slash command came first and is not the title");
        let b = got.iter().find(|c| c.id == B).unwrap();
        assert_eq!(item(b.clone()).title, "agy (22222222)", "no prompt at all: the provider and the first eight characters of the id");
        // the file's own label counts, not only a text that starts with a slash
        let labelled = json!({"display": "gemini-pro", "timestamp": 1_700_000_001_000u64, "workspace": "/w", "conversationId": A, "type": "slash_command"});
        let (home, _) = home_with(&[labelled.to_string(), line("then a real prompt", 1_700_000_002_000, "/w", Some(A))]);
        assert_eq!(load(home.path())[0].title, "then a real prompt", "a line the file calls a slash command is no prompt, whatever its text");
    }

    #[test]
    fn a_last_line_with_no_usable_folder_keeps_the_folder_an_earlier_one_had_and_a_conversation_may_have_none() {
        let no_folder = json!({"display": "q2", "timestamp": 1_700_000_002_000u64, "conversationId": A}).to_string();
        let (home, _) = home_with(&[line("q1", 1_700_000_001_000, "/w/kept", Some(A)), no_folder, line("q3", 1_700_000_003_000, "relative", Some(A)), line("only", 1_700_000_004_000, "relative", Some(B))]);
        let by: HashMap<String, Option<String>> = load(home.path()).into_iter().map(|c| (c.id, c.cwd)).collect();
        assert_eq!(by[A].as_deref(), Some("/w/kept"), "the later lines have no folder that is usable: the one it had stays");
        assert_eq!(by[B], None, "and a conversation that never had one has none");
    }

    #[test]
    fn only_a_regular_file_of_the_asking_user_with_no_group_or_other_bit_may_be_read() {
        let dir = Scratch::new();
        let file = dir.path().join("f");
        std::fs::write(&file, "x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let meta = std::fs::metadata(&file).unwrap();
        assert!(readable(&meta, meta.uid()));
        assert!(!readable(&meta, meta.uid() + 1), "someone else's file");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(!readable(&std::fs::metadata(&file).unwrap(), meta.uid()), "a group bit");
        assert!(!readable(&std::fs::metadata(dir.path()).unwrap(), meta.uid()), "a folder");
    }

    #[test]
    fn a_line_without_a_plain_id_or_a_damaged_or_long_or_foreign_line_is_skipped() {
        let long = line(&"x".repeat(MAX_LINE), 1_700_000_001_000, "/w", Some(A));
        assert!(long.len() > MAX_LINE);
        let (home, _) = home_with(&[
            line("no id", 1_700_000_001_000, "/w", None),
            line("dashed", 1_700_000_001_000, "/w", Some("-rf")),
            line("path", 1_700_000_001_000, "/w", Some("../../etc/passwd")),
            line("space", 1_700_000_001_000, "/w", Some("a b")),
            line("semicolon", 1_700_000_001_000, "/w", Some("a;b")),
            line("too long", 1_700_000_001_000, "/w", Some(&"a".repeat(65))),
            "{damaged".into(),
            "[1,2,3]".into(),
            "\"a string\"".into(),
            json!({"display": 7, "timestamp": 1, "workspace": "/w", "conversationId": A}).to_string(),
            json!({"display": "no time", "workspace": "/w", "conversationId": A}).to_string(),
            long,
            line("kept", 1_700_000_009_000, "/w", Some(B)),
        ]);
        let got = load(home.path());
        assert_eq!(got.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec![B], "only the good line is a conversation");
    }

    #[test]
    fn a_folder_that_is_not_an_absolute_path_without_control_characters_is_none_and_a_trailing_slash_goes() {
        let (home, _) = home_with(&[
            line("relative", 1_700_000_001_000, "rel/dir", Some(A)),
            line("control", 1_700_000_002_000, "/w/a\u{7}b", Some(B)),
            line("slash", 1_700_000_003_000, "/w/c/", Some("33333333-aaaa-4bbb-8ccc-000000000003")),
            line("huge", 1_700_000_004_000, &format!("/{}", "d".repeat(MAX_FOLDER)), Some("44444444-aaaa-4bbb-8ccc-000000000004")),
        ]);
        let by: HashMap<String, Option<String>> = load(home.path()).into_iter().map(|c| (c.id, c.cwd)).collect();
        assert_eq!(by[A], None);
        assert_eq!(by[B], None);
        assert_eq!(by["33333333-aaaa-4bbb-8ccc-000000000003"].as_deref(), Some("/w/c"));
        assert_eq!(by["44444444-aaaa-4bbb-8ccc-000000000004"], None);
    }

    #[test]
    fn the_title_is_cleaned_as_the_others_are_and_cut() {
        let (home, _) = home_with(&[line(&format!("  fix the bug {}\nsecond line of the prompt", "z".repeat(400)), 1_700_000_001_000, "/w", Some(A))]);
        let title = load(home.path())[0].title.clone();
        assert!(title.starts_with("fix the bug z"), "{title:?}");
        assert!(title.chars().count() <= crate::folders::MAX_NAME && !title.chars().any(char::is_control) && !title.contains("second line"), "the first line only, as the other titles are: {title:?}");
    }

    #[test]
    fn a_file_that_is_a_link_or_has_a_group_or_other_bit_or_is_missing_or_a_folder_reads_as_empty() {
        let (home, file) = home_with(&[line("q", 1_700_000_001_000, "/w", Some(A))]);
        assert_eq!(load(home.path()).len(), 1);
        for mode in [0o640, 0o604, 0o660, 0o666, 0o644] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
            assert!(load(home.path()).is_empty(), "{mode:o}");
        }
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert_eq!(load(home.path()).len(), 1, "read-only for the owner is fine");
        // a link to a good file
        let real = home.path().join("real.jsonl");
        std::fs::copy(&file, &real).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&real, &file).unwrap();
        assert!(load(home.path()).is_empty(), "a link is not followed");
        std::fs::remove_file(&file).unwrap();
        assert!(load(home.path()).is_empty(), "no file");
        std::fs::create_dir(&file).unwrap();
        assert!(load(home.path()).is_empty(), "a folder is no file");
        assert!(load(Path::new("/nonexistent-home-for-agy")).is_empty());
    }

    #[test]
    fn only_the_end_of_a_file_over_the_bound_is_read_and_the_line_cut_at_its_start_is_no_conversation() {
        let dir = Scratch::new();
        let file = history_path(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        // the old conversation A first, then filler lines of B that push A out of the last MAX_FILE bytes
        let filler = line(&"f".repeat(2000), 1_700_000_050_000, "/w", Some(B));
        let n = (MAX_FILE as usize / (filler.len() + 1)) + 20;
        let mut text = line("old question", 1_700_000_001_000, "/w", Some(A)) + "\n";
        for _ in 0..n {
            text.push_str(&filler);
            text.push('\n');
        }
        std::fs::write(&file, text).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(std::fs::metadata(&file).unwrap().len() > MAX_FILE);
        let got = load(dir.path());
        assert_eq!(got.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec![B], "the old line is beyond the bound; the cut line at the start of what is read is not a conversation of its own");
    }

    #[test]
    fn at_most_the_newest_two_hundred_conversations_are_listed() {
        let lines: Vec<String> = (0..MOST + 30).map(|i| line(&format!("q{i}"), 1_700_000_000_000 + i as u64 * 1000, "/w", Some(&format!("c{i:04}")))).collect();
        let (home, _) = home_with(&lines);
        let got = load(home.path());
        assert_eq!(got.len(), MOST);
        assert_eq!(got[0].id, format!("c{:04}", MOST + 29), "the newest first");
        assert_eq!(got[MOST - 1].id, "c0030", "and the oldest of them is the 201st from the end");
    }

    #[test]
    fn an_item_is_the_provider_agy_with_no_account_no_open_session_and_the_folder() {
        let it = item(Conversation { id: A.into(), title: "t".into(), cwd: Some("/w".into()), updated: 9 });
        assert_eq!((it.provider.as_str(), it.provider_session.as_str(), it.title.as_str(), it.cwd.as_deref(), it.updated_at), ("agy", A, "t", Some("/w"), 9));
        assert!(it.account.is_none() && it.account_label.is_none() && it.ssh_host.is_none() && it.open_session.is_none() && !it.global && it.recorded);
    }

    #[test]
    fn the_history_is_where_agy_keeps_it_under_the_home() {
        assert_eq!(history_path(Path::new("/h")), PathBuf::from("/h/.gemini/antigravity-cli/history.jsonl"));
    }
}
