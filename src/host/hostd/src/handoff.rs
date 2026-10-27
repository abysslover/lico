//! Long text one session sends another is kept as a file in the receiver's own folder (S03a):
//! `<handoff_dir>/<receiver id>/<YYYYMMDD>/<HHMMSS>_<sender>_<kind>.md`.
//!
//! Every folder is 0700 and every file 0600: the text may hold whatever a session saw. A file is written under a temporary name and linked into place, so a reader never sees
//! half of one, and a name that is taken gets `-2`, `-3` instead of being overwritten. The path is built from a session id that passed `registry::valid_session_id`, never from a
//! name, and a link is never followed. Files go after 30 days, and the oldest first when what is left is over 256 MiB (`doomed`, a pure plan; `sweep` carries it out).

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const KEEP_DAYS: u64 = 30;
pub const KEEP_BYTES: u64 = 256 << 20;
const KEEP_SECS: u64 = KEEP_DAYS * 86_400;
const NAMES: u32 = 1000; // `-2` ... `-1000` after a name that is taken; a thousand files in one second from one sender is a fault, not a use

pub struct Entry {
    pub path: PathBuf,
    pub modified: u64, // unix seconds
    pub size: u64,
}

/// The positions of the files to remove: those older than 30 days, then the oldest of the others until what is left is within 256 MiB.
pub fn doomed(entries: &[Entry], now: u64) -> Vec<usize> {
    let old: Vec<bool> = entries.iter().map(|e| now.saturating_sub(e.modified) > KEEP_SECS).collect();
    let mut gone: Vec<usize> = (0..entries.len()).filter(|&i| old[i]).collect();
    let mut rest: Vec<usize> = (0..entries.len()).filter(|&i| !old[i]).collect();
    rest.sort_by_key(|&i| (entries[i].modified, i));
    let mut total: u64 = rest.iter().map(|&i| entries[i].size).sum();
    for i in rest {
        if total <= KEEP_BYTES {
            break;
        }
        total -= entries[i].size;
        gone.push(i);
    }
    gone
}

fn invalid(what: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, what.to_string())
}

/// A folder of ours: made if it is not there (0700), and a link in its place is refused.
fn own_dir(path: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    if std::fs::symlink_metadata(path)?.is_dir() {
        Ok(())
    } else {
        Err(invalid("not a folder of the host's"))
    }
}

/// The folder a session receives files in; made when it is first needed.
pub fn inbox(root: &Path, session: &str) -> std::io::Result<PathBuf> {
    if !crate::registry::valid_session_id(session) {
        return Err(invalid("not a session id"));
    }
    own_dir(root)?;
    let path = root.join(session);
    own_dir(&path)?;
    Ok(path)
}

/// A session is deleted: its folder goes with it, and nobody else's.
pub fn remove_inbox(root: &Path, session: &str) {
    if !crate::registry::valid_session_id(session) {
        return;
    }
    let path = root.join(session);
    if std::fs::symlink_metadata(&path).map_or(false, |m| m.is_dir()) {
        let _ = std::fs::remove_dir_all(&path);
    }
}

/// Store `content` for `receiver` and give back the path. `date` and `time` are the local `YYYYMMDD` and `HHMMSS` of the transfer, `sender` seven characters of an id or `person`,
/// `kind` `message` or `selection`.
pub fn store(root: &Path, receiver: &str, date: &str, time: &str, sender: &str, kind: &str, content: &[u8]) -> std::io::Result<PathBuf> {
    let simple = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric());
    if !simple(date) || !simple(time) || !simple(sender) || !simple(kind) {
        return Err(invalid("not a name made by the host"));
    }
    let mut attempts = 0;
    loop {
        let folder = inbox(root, receiver)?.join(date);
        own_dir(&folder)?;
        let temporary = folder.join(format!(".tmp-{}", crate::core::random_hex(8)));
        let opened = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary);
        let mut file = match opened {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && attempts == 0 => {
                attempts += 1; // a sweep took the empty date folder between making it and filling it
                continue;
            }
            Err(e) => return Err(e),
        };
        let linked = file.write_all(content).and_then(|_| file.flush()).and_then(|_| {
            for n in 1..=NAMES {
                let name = if n == 1 { format!("{time}_{sender}_{kind}.md") } else { format!("{time}_{sender}_{kind}-{n}.md") };
                match std::fs::hard_link(&temporary, folder.join(&name)) {
                    Ok(()) => return Ok(folder.join(name)),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(invalid("every name is taken"))
        });
        let _ = std::fs::remove_file(&temporary);
        return linked;
    }
}

fn folders_in(path: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(path) else { return Vec::new() };
    read.flatten().map(|e| e.path()).filter(|p| std::fs::symlink_metadata(p).map_or(false, |m| m.is_dir())).collect()
}

/// Carry out `doomed` for everything under `root`, and remove the date folders that are left empty.
pub fn sweep(root: &Path, now: u64) {
    let (mut entries, mut dates) = (Vec::new(), Vec::new());
    for inbox in folders_in(root) {
        for date in folders_in(&inbox) {
            let Ok(read) = std::fs::read_dir(&date) else { continue };
            for file in read.flatten() {
                let Ok(meta) = std::fs::symlink_metadata(file.path()) else { continue };
                if meta.is_file() {
                    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                    entries.push(Entry { path: file.path(), modified, size: meta.len() });
                }
            }
            dates.push(date);
        }
    }
    for i in doomed(&entries, now) {
        let _ = std::fs::remove_file(&entries[i].path);
    }
    for date in dates {
        let _ = std::fs::remove_dir(date); // refused while it holds a file
    }
}

#[cfg(test)]
extern "C" {
    fn umask(mask: u32) -> u32;
}

/// The test binary runs with the umask most people have, whatever the shell it was started from: under 077 a folder or file made with a mode that is too wide would
/// come out private all the same, and a test of its mode could not fail.
#[cfg(test)]
pub(crate) fn usual_umask() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        unsafe { umask(0o022) };
    });
}

#[cfg(test)]
pub(crate) struct Scratch(pub PathBuf);

#[cfg(test)]
impl Scratch {
    /// A folder under the temporary directory that goes with the test.
    pub(crate) fn new(tag: &str) -> Scratch {
        usual_umask();
        static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!("lico-handoff-{tag}-{}-{}", std::process::id(), COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap();
        Scratch(path)
    }
}

#[cfg(test)]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const DAY: u64 = 86_400;
    const NOW: u64 = 100 * DAY;
    const A: &str = "s0123456789abcdef";
    const B: &str = "sfedcba9876543210";

    fn entry(age: u64, size: u64) -> Entry {
        Entry { path: PathBuf::new(), modified: NOW - age, size }
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_file_goes_when_it_is_older_than_thirty_days_and_not_a_second_before() {
        let entries = [entry(KEEP_SECS, 1), entry(KEEP_SECS + 1, 1), entry(0, 1), entry(29 * DAY, 1), entry(31 * DAY, 1)];
        let mut gone = doomed(&entries, NOW);
        gone.sort();
        assert_eq!(gone, [1, 4]);
        assert_eq!(doomed(&[], NOW), Vec::<usize>::new());
        assert_eq!(doomed(&[Entry { path: PathBuf::new(), modified: NOW + 5, size: 1 }], NOW), Vec::<usize>::new(), "a clock that went back removes nothing");
    }

    #[test]
    fn over_the_size_limit_the_oldest_go_first_until_the_rest_fits() {
        let mib = 1 << 20;
        let entries = [entry(3 * DAY, 100 * mib), entry(1 * DAY, 100 * mib), entry(5 * DAY, 100 * mib), entry(2 * DAY, 100 * mib)];
        assert_eq!(doomed(&entries, NOW), [2, 0], "400 MiB: the two oldest go, 200 MiB stay");
        let exact = [entry(1, 128 * mib), entry(2, 128 * mib)];
        assert_eq!(doomed(&exact, NOW), Vec::<usize>::new(), "exactly 256 MiB fits");
        let over = [entry(1, 128 * mib), entry(2, 128 * mib + 1)];
        assert_eq!(doomed(&over, NOW), [1]);
        let one = [entry(1, 300 * mib)];
        assert_eq!(doomed(&one, NOW), [0], "a file that alone is over the limit goes too");
        let mixed = [entry(40 * DAY, 200 * mib), entry(2 * DAY, 200 * mib), entry(1 * DAY, 100 * mib)];
        assert_eq!(doomed(&mixed, NOW), [0, 1], "the old file goes for its age and is not counted in the total: 300 MiB are left, so the older of those goes too");
    }

    #[test]
    fn a_stored_file_is_private_whole_and_never_replaces_another() {
        let scratch = Scratch::new("store");
        let root = scratch.0.join("handoff"); // not made yet: the first folder asserted on is the one `store` makes
        let first = store(&root, A, "20261027", "101500", "s111111", "message", b"one").unwrap();
        let second = store(&root, A, "20261027", "101500", "s111111", "message", b"two").unwrap();
        let third = store(&root, A, "20261027", "101500", "s111111", "message", b"three").unwrap();
        assert_eq!(first, root.join(A).join("20261027").join("101500_s111111_message.md"));
        assert_eq!(second.file_name().unwrap(), "101500_s111111_message-2.md");
        assert_eq!(third.file_name().unwrap(), "101500_s111111_message-3.md");
        assert_eq!([&first, &second, &third].map(|p| std::fs::read(p).unwrap()), [b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]);
        assert_eq!([mode(&root), mode(&root.join(A)), mode(&root.join(A).join("20261027")), mode(&first)], [0o700, 0o700, 0o700, 0o600]);
        let left: Vec<String> = std::fs::read_dir(first.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        assert_eq!(left.len(), 3, "no temporary file stays: {left:?}");
        assert!(!root.join(B).exists(), "another receiver's folder is not made");
    }

    #[test]
    fn nothing_is_stored_for_a_name_the_host_did_not_make_or_through_a_link() {
        let root = Scratch::new("refuse");
        for receiver in ["", "..", "s1", "../s0123456789abcde", "s0123456789abcdeg"] {
            assert!(store(&root.0, receiver, "20261027", "101500", "person", "message", b"x").is_err(), "{receiver:?}");
        }
        for (date, time, sender, kind) in [("../x", "1", "p", "m"), ("1", "a/b", "p", "m"), ("1", "1", "", "m"), ("1", "1", "p", "m.md"), ("", "1", "p", "m")] {
            assert!(store(&root.0, A, date, time, sender, kind, b"x").is_err(), "{date:?} {time:?} {sender:?} {kind:?}");
        }
        let elsewhere = Scratch::new("elsewhere");
        std::os::unix::fs::symlink(&elsewhere.0, root.0.join(A)).unwrap();
        assert!(store(&root.0, A, "20261027", "101500", "person", "message", b"x").is_err(), "an inbox that is a link");
        assert!(std::fs::read_dir(&elsewhere.0).unwrap().next().is_none(), "nothing was written through it");
        remove_inbox(&root.0, A);
        assert!(std::fs::symlink_metadata(root.0.join(A)).is_ok(), "a link is not removed as a folder, and its target is not touched");
    }

    #[test]
    fn deleting_a_session_removes_its_inbox_and_no_other() {
        let root = Scratch::new("remove");
        let kept = store(&root.0, B, "20261027", "101500", "person", "message", b"keep").unwrap();
        store(&root.0, A, "20261027", "101500", "person", "message", b"drop").unwrap();
        remove_inbox(&root.0, "../" );
        remove_inbox(&root.0, A);
        assert!(!root.0.join(A).exists() && kept.exists());
        assert_eq!(inbox(&root.0, A).unwrap(), root.0.join(A), "it comes back when it is needed again");
    }

    #[test]
    fn a_sweep_removes_the_old_files_and_the_date_folders_it_empties() {
        let root = Scratch::new("sweep");
        let old = store(&root.0, A, "20260101", "101500", "person", "message", b"old").unwrap();
        let fresh = store(&root.0, A, "20261027", "101500", "person", "message", b"fresh").unwrap();
        let other = store(&root.0, B, "20260101", "101500", "person", "message", b"other").unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        for path in [&old, &other] {
            let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(now - 31 * DAY)).unwrap();
        }
        sweep(&root.0, now);
        assert!(!old.exists() && !other.exists() && fresh.exists());
        assert!(!old.parent().unwrap().exists() && !other.parent().unwrap().exists(), "the empty date folders go");
        assert!(root.0.join(A).is_dir() && root.0.join(B).is_dir(), "an inbox stays: the session still has it");
        sweep(&root.0, now);
        sweep(&root.0.join("nothing here"), now);
        assert!(fresh.exists());
    }
}
