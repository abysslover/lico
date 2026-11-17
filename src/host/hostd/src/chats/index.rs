//! The chat index (`M22-P02`): what every scope of `session.chats` reads, so that no call scans a home.
//!
//! One index per provider home (a managed account's home, or the provider's own). It holds the newest `MAX_SCAN_FILES` records of the home: for each, the size and modification time it
//! had when it was last read, and what its head says (session id, folder, title). A call refreshes the home it reads by an INCREMENTAL pass: the directories are listed, each record is
//! `stat`ed (a link is never followed and only `*.jsonl` counts), and a record is opened again only when its size or time changed or it is new; one that is gone is dropped. So an answer is
//! exact (a record written a moment ago is in it) and a repeated call costs one `stat` per record. The host builds the indexes of the homes that exist in the background when it starts
//! (`prewarm`), so the first call finds them ready. Memory only: nothing of a record's text is written to a second place.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde_json::Value;

use super::{claude_msg_text, clean_title, codex_msg_text, codex_stem_id, is_codex_system_text, norm_path, open_regular_nofollow, real_dir, Scanned, MAX_HEAD_BYTES, MAX_SCAN_FILES, MAX_SCAN_LINES};
use crate::core::Shared;
use crate::folders::{derive_auto_title, valid_path_shape};
use crate::provider_record::valid_provider_session;

/// How many `*.jsonl` files a listing of one home looks at at the most; a home with more says it left some out. It bounds the walk, not the memory (the index keeps `MAX_SCAN_FILES`).
const LIST_MAX: usize = 20_000;
/// How many homes the registry keeps before it lets go of those nobody is using.
const MAX_HOMES: usize = 64;
/// The newest records of a home whose turn counts the background build prepares (one page of the window).
const WARM_TURNS: usize = 100;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Kind {
    Claude,
    Codex,
}

impl Kind {
    fn of(provider: &str) -> Option<Kind> {
        match provider {
            "claude" => Some(Kind::Claude),
            "codex" => Some(Kind::Codex),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Kind::Claude => "claude",
            Kind::Codex => "codex",
        }
    }
}

/// A record's size and modification time as the listing saw them; a change of either (to the nanosecond) is what makes the head be read again.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Stamp {
    size: u64,
    secs: i64,
    nanos: i64,
}

type Listed = (PathBuf, Stamp);

struct Entry {
    stamp: Stamp,
    /// What the head said; `None` for a file that is no chat (no folder, a name that is no session id): it is remembered so that it is not opened again until it changes.
    rec: Option<Scanned>,
}

#[derive(Default)]
struct Home {
    entries: HashMap<OsString, Entry>, // keyed by the path's bytes (hashing a `PathBuf` walks its components, which cost more than the `stat` it saves)
    left_out: bool,
}

/// What one home gives a call: its records (any order) and whether older ones were left out of the index.
pub(super) struct Found {
    pub(super) found: Vec<Scanned>,
    pub(super) left_out: bool,
}

/// The newest `MAX_SCAN_FILES` of `listed` (newest first, a tie broken by the path so that the choice does not depend on the order the directory gave), and whether any were left out.
/// A listing that fits is returned as it is: its order does not matter to the index.
fn newest(mut listed: Vec<Listed>, listing_cut: bool) -> (Vec<Listed>, bool) {
    let over = listed.len() > MAX_SCAN_FILES;
    if over {
        listed.sort_by(|a, b| (b.1.secs, b.1.nanos).cmp(&(a.1.secs, a.1.nanos)).then_with(|| a.0.as_os_str().cmp(b.0.as_os_str())));
        listed.truncate(MAX_SCAN_FILES);
    }
    (listed, listing_cut || over)
}

impl Home {
    /// The incremental pass: keep what is unchanged, read what is new or changed, drop what is gone. Returns how many heads were read.
    /// When nothing changed (the usual call) it only looks each record up: no head is opened and the index is not rebuilt.
    fn update(&mut self, listed: Vec<Listed>, listing_cut: bool, head: &dyn Fn(&Path) -> Option<Scanned>) -> usize {
        let (chosen, left_out) = newest(listed, listing_cut);
        self.left_out = left_out;
        let unchanged = chosen.iter().filter(|(path, stamp)| self.entries.get(path.as_os_str()).is_some_and(|e| e.stamp == *stamp)).count();
        if unchanged == chosen.len() && unchanged == self.entries.len() {
            return 0;
        }
        let mut old = std::mem::take(&mut self.entries);
        let mut reads = 0;
        let mut entries = HashMap::with_capacity(chosen.len());
        for (path, stamp) in chosen {
            let entry = match old.remove(path.as_os_str()) {
                Some(kept) if kept.stamp == stamp => kept,
                _ => {
                    reads += 1;
                    let rec = head(&path);
                    Entry { stamp, rec }
                }
            };
            entries.insert(path.into_os_string(), entry);
        }
        self.entries = entries;
        reads
    }

    fn read(&self) -> Found {
        Found { found: self.entries.values().filter_map(|e| e.rec.clone()).collect(), left_out: self.left_out }
    }
}

/// `*.jsonl` regular files of `dir` and, up to `max_depth` levels down, below it; only those at `files_from` levels down or deeper are listed. A link is never followed (a linked folder is
/// not entered, a linked file is not listed) and no file other than `*.jsonl` is looked at beyond its name.
fn walk(dir: &Path, depth: usize, files_from: usize, max_depth: usize, limit: usize, out: &mut Vec<Listed>, cut: &mut bool) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if *cut {
            return;
        }
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if depth < max_depth {
                walk(&entry.path(), depth + 1, files_from, max_depth, limit, out, cut);
            }
        } else if kind.is_file() && depth >= files_from && entry.file_name().as_encoded_bytes().ends_with(b".jsonl") {
            if out.len() >= limit {
                *cut = true;
                return;
            }
            if let Ok(meta) = entry.metadata() {
                if meta.file_type().is_file() {
                    out.push((entry.path(), Stamp { size: meta.len(), secs: meta.mtime(), nanos: meta.mtime_nsec() }));
                }
            }
        }
    }
}

/// The records of a home: a Claude home is `projects/<folder>/<session>.jsonl`, a Codex home `sessions/<year>/<month>/<day>/rollout-*.jsonl` (a few levels, as the provider keeps them).
fn list(kind: Kind, home: &Path, limit: usize) -> (Vec<Listed>, bool) {
    let (root, files_from, max_depth) = match kind {
        Kind::Claude => (home.join("projects"), 1, 1),
        Kind::Codex => (home.join("sessions"), 0, 5),
    };
    let (mut out, mut cut) = (Vec::new(), false);
    if real_dir(&root) {
        walk(&root, 0, files_from, max_depth, limit, &mut out, &mut cut);
    }
    (out, cut)
}

/// A Claude transcript's head: its session id (the file name), its folder and the first thing the person asked. `None` when it has no folder or its name is no session id.
fn head_claude(path: &Path) -> Option<Scanned> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jsonl")?;
    if !valid_provider_session(stem) {
        return None;
    }
    let (file, _, mtime) = open_regular_nofollow(path)?;
    let reader = BufReader::new(file.take(MAX_HEAD_BYTES));
    let mut found_cwd: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    for line in reader.lines().take(MAX_SCAN_LINES).flatten() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if found_cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(Value::as_str).filter(|s| s.starts_with('/') && valid_path_shape(s)) {
                found_cwd = Some(norm_path(c).to_string());
            }
        }
        if first_prompt.is_none() && v.get("type").and_then(Value::as_str) == Some("user") {
            if let Some(txt) = claude_msg_text(&v, true) {
                if derive_auto_title(&txt).is_some() {
                    first_prompt = Some(txt);
                }
            }
        }
        if found_cwd.is_some() && first_prompt.is_some() {
            break;
        }
    }
    let cwd = found_cwd?;
    let title = clean_title(first_prompt.as_deref().unwrap_or(""), "claude", stem);
    Some((stem.to_string(), cwd, title, mtime, path.to_path_buf()))
}

/// A Codex transcript's head: its thread id (the `session_meta` record, else the file name), its folder and the first thing the person asked.
fn head_codex(path: &Path) -> Option<Scanned> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jsonl")?;
    let (file, _, mtime) = open_regular_nofollow(path)?;
    let reader = BufReader::new(file.take(MAX_HEAD_BYTES));
    let mut meta_id: Option<String> = None;
    let mut found_cwd: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    for line in reader.lines().take(MAX_SCAN_LINES).flatten() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let vtype = v.get("type").and_then(Value::as_str).unwrap_or("");
        if vtype == "session_meta" {
            if let Some(id) = v.pointer("/payload/id").and_then(Value::as_str).filter(|s| valid_provider_session(s)) {
                meta_id = Some(id.to_string());
            }
            if let Some(c) = v.pointer("/payload/cwd").and_then(Value::as_str).filter(|s| s.starts_with('/') && valid_path_shape(s)) {
                found_cwd = Some(norm_path(c).to_string());
            }
        } else if found_cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(Value::as_str).filter(|s| s.starts_with('/') && valid_path_shape(s)) {
                found_cwd = Some(norm_path(c).to_string());
            }
        }
        if first_prompt.is_none() {
            if let Some((true, txt)) = codex_msg_text(&v) {
                if derive_auto_title(&txt).is_some() {
                    first_prompt = Some(txt);
                }
            } else if vtype == "event_msg" && v.pointer("/payload/type").and_then(Value::as_str) == Some("user_message") {
                if let Some(m) = v.pointer("/payload/message").and_then(Value::as_str) {
                    if !is_codex_system_text(m) && derive_auto_title(m).is_some() {
                        first_prompt = Some(m.to_string());
                    }
                }
            }
        }
        if found_cwd.is_some() && first_prompt.is_some() && meta_id.is_some() {
            break;
        }
    }
    let cwd = found_cwd?;
    let prov_sid = meta_id.or_else(|| codex_stem_id(stem))?;
    let title = clean_title(first_prompt.as_deref().unwrap_or(""), "codex", &prov_sid);
    Some((prov_sid, cwd, title, mtime, path.to_path_buf()))
}

fn slot(kind: Kind, home: &Path) -> Arc<Mutex<Home>> {
    static HOMES: OnceLock<Mutex<HashMap<(Kind, PathBuf), Arc<Mutex<Home>>>>> = OnceLock::new();
    let mut homes = HOMES.get_or_init(Default::default).lock().unwrap_or_else(PoisonError::into_inner);
    let key = (kind, home.to_path_buf());
    if homes.len() >= MAX_HOMES && !homes.contains_key(&key) {
        homes.retain(|_, slot| Arc::strong_count(slot) > 1); // a home nobody holds is built again when it is asked for
    }
    homes.entry(key).or_default().clone()
}

/// The records of one home, after the incremental pass. The home's index is behind a lock of its own: a call waits at most for one pass of the same home.
fn read(kind: Kind, home: &Path) -> Found {
    let slot = slot(kind, home);
    let mut index = slot.lock().unwrap_or_else(PoisonError::into_inner);
    let (listed, cut) = list(kind, home, LIST_MAX);
    match kind {
        Kind::Claude => index.update(listed, cut, &head_claude),
        Kind::Codex => index.update(listed, cut, &head_codex),
    };
    index.read()
}

pub(super) fn claude_home(home: &Path) -> Found {
    read(Kind::Claude, home)
}

pub(super) fn codex_home(home: &Path) -> Found {
    read(Kind::Codex, home)
}

/// The homes of this machine that hold chats the host lists: the managed accounts' (not an ssh account's: that home is on the other machine) and the provider's own where it is on.
fn local_homes(shared: &Shared) -> Vec<(Kind, PathBuf)> {
    let mut homes = Vec::new();
    {
        let accounts = shared.accounts.lock().unwrap_or_else(PoisonError::into_inner);
        for account in accounts.list_snapshot() {
            if let (None, Some(kind), Some(h)) = (&account.ssh_host, Kind::of(&account.provider), accounts.homes(&account.id)) {
                homes.push((kind, h.provider_home));
            }
        }
    }
    for kind in [Kind::Claude, Kind::Codex] {
        if crate::native::enabled(shared, kind.word()) {
            if let Some(home) = crate::native::home(shared, kind.word()) {
                homes.push((kind, home));
            }
        }
    }
    homes
}

/// Build the indexes of the homes that exist, on a thread of its own, newest page of turn counts included, so that the first call of the window finds them ready. A call never waits for this
/// thread longer than for one pass of one home (their locks are the same).
pub fn prewarm(shared: &Arc<Shared>) {
    let shared = shared.clone();
    let _ = std::thread::Builder::new().name("chat-index".into()).spawn(move || {
        for (kind, home) in local_homes(&shared) {
            let mut found = read(kind, &home).found;
            found.sort_by(|a, b| b.3.cmp(&a.3));
            for (.., file) in found.iter().take(WARM_TURNS) {
                let _ = super::turns::count(kind.word(), file);
            }
        }
    });
}

/// How many chats the provider's own home holds, for `account.list` (`None` when there is no such home).
pub fn count_of(shared: &Shared, provider: &str) -> Option<usize> {
    let (kind, home) = (Kind::of(provider)?, crate::native::home(shared, provider)?);
    Some(read(kind, &home).found.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;
    use std::time::{Duration, UNIX_EPOCH};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lico_index_{tag}_{}", crate::core::random_hex(6)));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn at(path: &Path, secs: u64) {
        fs::File::options().write(true).open(path).unwrap().set_modified(UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
    }

    fn claude_record(home: &Path, folder: &str, sid: &str, cwd: &str, prompt: &str, mtime: u64) -> PathBuf {
        let dir = home.join("projects").join(folder);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{sid}.jsonl"));
        fs::write(&path, format!("{{\"type\":\"user\",\"cwd\":\"{cwd}\",\"message\":{{\"role\":\"user\",\"content\":\"{prompt}\"}}}}\n")).unwrap();
        at(&path, mtime);
        path
    }

    fn ids(read: &Found) -> Vec<String> {
        let mut found = read.found.clone();
        found.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));
        found.into_iter().map(|s| s.0).collect()
    }

    /// An update with a counter of the heads it was asked to read.
    fn counted_update(home: &mut Home, dir: &Path, kind: Kind) -> usize {
        let (listed, cut) = list(kind, dir, LIST_MAX);
        let opened = Cell::new(0usize);
        let reads = home.update(listed, cut, &|p| {
            opened.set(opened.get() + 1);
            match kind {
                Kind::Claude => head_claude(p),
                Kind::Codex => head_codex(p),
            }
        });
        assert_eq!(reads, opened.get(), "the count says what was opened");
        reads
    }

    #[test]
    fn the_newest_are_kept_a_tie_is_broken_by_the_path_and_the_rest_is_said_to_be_left_out() {
        let stamp = |secs| Stamp { size: 1, secs, nanos: 0 };
        let many: Vec<Listed> = (0..MAX_SCAN_FILES + 3).map(|n| (PathBuf::from(format!("/h/{n:05}")), stamp(10 + n as i64))).collect();
        let (kept, left_out) = newest(many, false);
        assert_eq!(kept.len(), MAX_SCAN_FILES);
        assert!(left_out, "three files were not kept");
        assert_eq!(kept[0].0, PathBuf::from(format!("/h/{:05}", MAX_SCAN_FILES + 2)), "the newest first");
        assert!(kept.iter().all(|(p, _)| p != &PathBuf::from("/h/00000")), "the oldest are the ones left out");
        let tie: Vec<Listed> = (0..=MAX_SCAN_FILES).rev().map(|n| (PathBuf::from(format!("/h/{n:05}")), stamp(5))).collect();
        let (kept, left_out) = newest(tie, false);
        assert!(left_out && kept.len() == MAX_SCAN_FILES);
        assert!(kept.iter().all(|(p, _)| p != &PathBuf::from(format!("/h/{MAX_SCAN_FILES:05}"))), "a tie is broken by the path: the last path is the one left out");
        let few = vec![(PathBuf::from("/h/b"), stamp(5)), (PathBuf::from("/h/a"), stamp(5))];
        assert_eq!(newest(few.clone(), false).1, false, "all of them fit: nothing is left out");
        assert!(newest(few, true).1, "a listing that was cut says so even when what it found fits");
    }

    #[test]
    fn an_unchanged_record_is_not_opened_again_a_grown_one_is_a_new_one_is_and_a_removed_one_goes() {
        let home = scratch("incremental");
        let old = claude_record(&home, "p", "cl-old", "/work/one", "old question", 1_000);
        claude_record(&home, "p", "cl-mid", "/work/two", "mid question", 2_000);
        let mut index = Home::default();
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 2, "the first pass opens both");
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 0, "nothing changed: nothing is opened");
        assert_eq!(ids(&index.read()), ["cl-mid", "cl-old"]);

        claude_record(&home, "q", "cl-new", "/work/one", "new question", 3_000);
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 1, "only the new record is opened");
        assert_eq!(ids(&index.read()), ["cl-new", "cl-mid", "cl-old"]);

        claude_record(&home, "p", "cl-old", "/work/one", "an older question that was edited and is longer now", 1_000);
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 1, "a record whose size changed is opened again");
        let title = index.read().found.into_iter().find(|s| s.0 == "cl-old").unwrap().2;
        assert!(title.starts_with("an older question"), "{title}");

        let same_size = fs::read_to_string(&old).unwrap().replace("an older", "a newer!");
        assert_eq!(same_size.len(), fs::read_to_string(&old).unwrap().len());
        fs::write(&old, same_size).unwrap();
        at(&old, 1_001);
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 1, "a record whose time changed is opened again, whatever its size");

        fs::remove_file(&old).unwrap();
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 0);
        assert_eq!(ids(&index.read()), ["cl-new", "cl-mid"], "a removed record goes");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_file_that_is_no_chat_is_remembered_and_not_opened_again_until_it_changes() {
        let home = scratch("nochat");
        let dir = home.join("projects").join("p");
        fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("cl-broken.jsonl");
        fs::write(&broken, "not json at all\n").unwrap();
        claude_record(&home, "p", "cl-ok", "/work/x", "a question", 5_000);
        let mut index = Home::default();
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 2);
        assert_eq!(ids(&index.read()), ["cl-ok"], "a file without a folder is no chat");
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 0, "and it is not read again while it is the same");
        fs::write(&broken, "{\"cwd\":\"/work/x\",\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"now it is one\"}}\n").unwrap();
        assert_eq!(counted_update(&mut index, &home, Kind::Claude), 1);
        assert_eq!(ids(&index.read()).len(), 2, "once it has a folder it is a chat");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn only_jsonl_files_are_records_and_a_link_is_never_followed() {
        let home = scratch("jsonl");
        claude_record(&home, "p", "cl-real", "/work/x", "a question", 5_000);
        let dir = home.join("projects").join("p");
        for n in 0..50 {
            fs::write(dir.join(format!("note-{n}.txt")), "x").unwrap();
        }
        fs::write(home.join("projects").join("stray.jsonl"), "{}\n").unwrap(); // a file straight in `projects`: no folder level, no record
        let nested = dir.join("cl-real").join("subagents");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("agent-1.jsonl"), "{}\n").unwrap(); // deeper than a Claude record lives: not listed
        let elsewhere = scratch("elsewhere");
        claude_record(&elsewhere, "q", "cl-outside", "/work/y", "outside", 9_000);
        std::os::unix::fs::symlink(elsewhere.join("projects").join("q"), home.join("projects").join("linked")).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("projects").join("q").join("cl-outside.jsonl"), dir.join("cl-link.jsonl")).unwrap();
        let (listed, cut) = list(Kind::Claude, &home, LIST_MAX);
        let names: Vec<String> = listed.iter().map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["cl-real.jsonl"], "neither a linked folder, a linked file, another extension nor another depth is listed");
        assert!(!cut);
        let none = list(Kind::Claude, &home.join("absent"), LIST_MAX);
        assert!(none.0.is_empty() && !none.1, "a missing home is empty");
        fs::remove_dir_all(&home).unwrap();
        fs::remove_dir_all(&elsewhere).unwrap();
    }

    #[test]
    fn a_codex_home_is_walked_down_the_dated_folders_and_the_listing_stops_at_its_bound() {
        let home = scratch("codex");
        let day = home.join("sessions").join("2026").join("11").join("12");
        fs::create_dir_all(&day).unwrap();
        for n in 0..5 {
            fs::write(day.join(format!("rollout-2026-11-12T10-00-0{n}-th-{n}.jsonl")), "{}\n").unwrap();
        }
        fs::write(day.join("history.db"), "x").unwrap();
        let too_deep = home.join("sessions").join("a").join("b").join("c").join("d").join("e").join("f");
        fs::create_dir_all(&too_deep).unwrap();
        fs::write(too_deep.join("rollout-deep.jsonl"), "{}\n").unwrap();
        let (listed, cut) = list(Kind::Codex, &home, LIST_MAX);
        assert_eq!((listed.len(), cut), (5, false), "five records; the other file and the one below the depth bound are not listed");
        let (listed, cut) = list(Kind::Codex, &home, 3);
        assert_eq!((listed.len(), cut), (3, true), "a listing that hits its bound says so");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn the_heads_give_the_session_the_folder_and_the_first_question_and_a_codex_thread_may_be_named_by_its_file() {
        let home = scratch("heads");
        let c = claude_record(&home, "p", "cl-1", "/work/a/", "what is the plan", 1_000);
        let (sid, cwd, title, mtime, path) = head_claude(&c).unwrap();
        assert_eq!((sid.as_str(), cwd.as_str(), mtime, &path), ("cl-1", "/work/a", 1_000, &c), "the folder has no trailing slash");
        assert!(title.starts_with("what is the plan"), "{title}");
        let bad = home.join("projects").join("p").join("-bad.jsonl");
        fs::write(&bad, fs::read(&c).unwrap()).unwrap();
        assert!(head_claude(&bad).is_none(), "a name that is no session id is no chat");

        let day = home.join("sessions");
        fs::create_dir_all(&day).unwrap();
        let x = day.join("rollout-2026-11-12T10-00-00-019a7654-1234-7000-8000-abcdef123456.jsonl");
        fs::write(&x, "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work/b\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"fix the sidebar\"}}\n").unwrap();
        let (sid, cwd, title, ..) = head_codex(&x).unwrap();
        assert_eq!((sid.as_str(), cwd.as_str()), ("019a7654-1234-7000-8000-abcdef123456", "/work/b"), "no id in the meta record: the file name has it");
        assert!(title.starts_with("fix the sidebar"), "{title}");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_home_is_read_through_its_registry_slot_and_a_second_read_sees_the_change() {
        let home = scratch("slot");
        claude_record(&home, "p", "cl-a", "/work/a", "first", 1_000);
        assert_eq!(ids(&claude_home(&home)), ["cl-a"]);
        claude_record(&home, "p", "cl-b", "/work/a", "second", 2_000);
        let again = claude_home(&home);
        assert_eq!(ids(&again), ["cl-b", "cl-a"]);
        assert!(!again.left_out);
        fs::remove_dir_all(&home).unwrap();
    }
}
