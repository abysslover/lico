//! The scan of a whole provider home for the scope `all` of `session.chats` (`M22-P01`).
//!
//! The other scopes open every record of a home and keep those of one folder. `all` keeps every folder, so it must not open every record: the files of a home are listed first by
//! their modification time (metadata only, no link followed), and only the newest `cap` are opened. A home with more than that has older records left unread, and the answer says so
//! (`truncated`), so a short list never looks whole. The list of files is itself bounded by `MAX_SCAN_FILES`, as in every scope.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::{real_dir, scan_claude_file, scan_codex_file, Scanned, MAX_SCAN_FILES};

/// How many records of one home `all` opens (below `MAX_SCAN_FILES`, the most a listing holds): measured in the report of `M22-P01`, a page of 100 of 2,000 records costs well under 0.2 s.
pub(super) const ALL_SCAN_MAX: usize = 2000;

/// What was read of one home, and whether older records were left unread.
pub(super) struct Newest {
    pub(super) found: Vec<Scanned>,
    pub(super) left_out: bool,
}

fn mtime_of(path: &Path) -> Option<u64> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    meta.file_type().is_file().then_some(())?;
    Some(meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()))
}

/// The newest `cap` of `files` (newest first, a tie broken by the path so that the choice does not depend on the order the directory gave), and whether any were left out.
fn newest_of(mut files: Vec<(u64, PathBuf)>, cap: usize, listing_cut: bool) -> (Vec<PathBuf>, bool) {
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let left_out = listing_cut || files.len() > cap;
    files.truncate(cap);
    (files.into_iter().map(|(_, p)| p).collect(), left_out)
}

/// A Claude home: `projects/<folder>/<session>.jsonl`.
pub(super) fn claude_home(home: &Path, cap: usize) -> Newest {
    let projects = home.join("projects");
    if !real_dir(&projects) {
        return Newest { found: Vec::new(), left_out: false };
    }
    let mut files = Vec::new();
    let mut listing_cut = false;
    'folders: for folder in std::fs::read_dir(&projects).into_iter().flatten().flatten() {
        let dir = folder.path();
        if !real_dir(&dir) {
            continue;
        }
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            if files.len() >= MAX_SCAN_FILES {
                listing_cut = true;
                break 'folders;
            }
            let path = entry.path();
            if let Some(m) = mtime_of(&path) {
                files.push((m, path));
            }
        }
    }
    let (chosen, left_out) = newest_of(files, cap, listing_cut);
    Newest { found: chosen.iter().filter_map(|p| scan_claude_file(p, &|_| true)).collect(), left_out }
}

fn list_codex(dir: &Path, depth: usize, files: &mut Vec<(u64, PathBuf)>, cut: &mut bool) {
    if depth > 5 || *cut || !real_dir(dir) {
        return;
    }
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if real_dir(&path) {
            list_codex(&path, depth + 1, files, cut);
        } else if let Some(m) = mtime_of(&path) {
            if files.len() >= MAX_SCAN_FILES {
                *cut = true;
                return;
            }
            files.push((m, path));
        }
    }
}

/// A Codex home: `sessions/<year>/<month>/<day>/rollout-*.jsonl`.
pub(super) fn codex_home(home: &Path, cap: usize) -> Newest {
    let mut files = Vec::new();
    let mut cut = false;
    list_codex(&home.join("sessions"), 0, &mut files, &mut cut);
    let (chosen, left_out) = newest_of(files, cap, cut);
    Newest { found: chosen.iter().filter_map(|p| scan_codex_file(p, &|_| true)).collect(), left_out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lico_newest_{tag}_{}", crate::core::random_hex(6)));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn claude_record(home: &Path, folder: &str, sid: &str, cwd: &str, mtime: u64) {
        let dir = home.join("projects").join(folder);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{sid}.jsonl"));
        fs::write(&path, format!("{{\"type\":\"user\",\"cwd\":\"{cwd}\",\"message\":{{\"role\":\"user\",\"content\":\"question {sid}\"}}}}\n")).unwrap();
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(mtime)).unwrap();
    }

    #[test]
    fn the_newest_are_kept_a_tie_is_broken_by_the_path_and_the_rest_is_said_to_be_left_out() {
        let files = vec![(10, PathBuf::from("/h/c")), (30, PathBuf::from("/h/a")), (20, PathBuf::from("/h/b")), (20, PathBuf::from("/h/a2"))];
        let (kept, left_out) = newest_of(files.clone(), 3, false);
        assert_eq!(kept, [PathBuf::from("/h/a"), PathBuf::from("/h/a2"), PathBuf::from("/h/b")]);
        assert!(left_out, "one file was not kept");
        let (kept, left_out) = newest_of(files.clone(), 4, false);
        assert_eq!((kept.len(), left_out), (4, false), "all of them fit: nothing is left out");
        assert!(newest_of(files, 4, true).1, "a listing that was cut says so even when what it found fits");
    }

    #[test]
    fn a_claude_home_gives_every_folder_newest_first_and_opens_no_more_than_the_cap() {
        let home = scratch("claude");
        claude_record(&home, "p-one", "cl-old", "/work/one", 1_000);
        claude_record(&home, "p-two", "cl-mid", "/work/two", 2_000);
        claude_record(&home, "p-one", "cl-new", "/work/one", 3_000);
        let all = claude_home(&home, 10);
        let ids: Vec<&str> = all.found.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(ids, ["cl-new", "cl-mid", "cl-old"], "newest first, whatever the folder");
        assert!(!all.left_out);
        assert_eq!(all.found[1].1, "/work/two", "each record keeps its own folder");
        let capped = claude_home(&home, 2);
        let ids: Vec<&str> = capped.found.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(ids, ["cl-new", "cl-mid"], "the oldest is the one that is left unread");
        assert!(capped.left_out);
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_link_in_a_home_is_not_followed_and_a_missing_home_is_empty() {
        let home = scratch("link");
        claude_record(&home, "p", "cl-real", "/work/x", 5_000);
        let elsewhere = scratch("elsewhere");
        claude_record(&elsewhere, "q", "cl-outside", "/work/y", 9_000);
        std::os::unix::fs::symlink(elsewhere.join("projects").join("q"), home.join("projects").join("linked")).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("projects").join("q").join("cl-outside.jsonl"), home.join("projects").join("p").join("cl-link.jsonl")).unwrap();
        let ids: Vec<String> = claude_home(&home, 10).found.into_iter().map(|s| s.0).collect();
        assert_eq!(ids, ["cl-real"], "neither a linked folder nor a linked file is read");
        let none = claude_home(&home.join("absent"), 10);
        assert!(none.found.is_empty() && !none.left_out);
        fs::remove_dir_all(&home).unwrap();
        fs::remove_dir_all(&elsewhere).unwrap();
    }
}
