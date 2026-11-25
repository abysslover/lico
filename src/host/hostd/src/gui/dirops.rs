//! The file operations of the Directories menu (`M24-P10a`): what a folder name may be and the making of a folder. Standard library only, no window: the menu calls these and says what they answer.

use std::path::{Path, PathBuf};

/// The longest name the file systems here keep, in bytes.
pub(crate) const MAX_NAME_BYTES: usize = 255;

/// Why `name` cannot be the name of a folder, in words for the person; None when it can (the name is taken without the spaces around it).
pub(crate) fn name_problem(name: &str) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        Some("the name is empty")
    } else if name == "." || name == ".." {
        Some("a folder cannot be called . or ..")
    } else if name.contains('/') || name.contains('\0') {
        Some("a name has no slash")
    } else if name.chars().any(char::is_control) {
        Some("a name has no line break or control character")
    } else if crate::folders::shows_nothing(name) {
        Some("the name shows nothing")
    } else if name.len() > MAX_NAME_BYTES {
        Some("the name is too long (at most 255 bytes)")
    } else {
        None
    }
}

/// Makes the folder `name` inside `parent`; the path of the new folder, or why it was not made. Nothing existing is touched.
pub(crate) fn make_dir(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if let Some(why) = name_problem(name) {
        return Err(why.into());
    }
    if !parent.is_dir() {
        return Err("the folder is gone".into());
    }
    let made = parent.join(name.trim());
    match std::fs::create_dir(&made) {
        Ok(()) => Ok(made),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err("that name is taken".into()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err("no permission to make a folder here".into()),
        Err(e) => Err(format!("the system said: {}", e.kind())),
    }
}

/// How many items the question of a delete counts before it says `more than`.
pub(crate) const COUNT_LIMIT: usize = 5_000;
/// The longest the count may take, so a slow disk cannot hold the window.
const COUNT_TIME: std::time::Duration = std::time::Duration::from_millis(250);

/// Renames `old` to `new` inside `parent`; the new path, or why it was not renamed. Nothing is ever replaced: a name that anything holds (a file, a folder, a link, a dangling link) is refused. The check and the move are two steps (the standard library has no move that refuses to replace).
pub(crate) fn rename(parent: &Path, old: &str, new: &str) -> Result<PathBuf, String> {
    if let Some(why) = name_problem(new) {
        return Err(why.into());
    }
    let new = new.trim();
    if new == old {
        return Err("that is its name already".into());
    }
    let from = parent.join(old);
    if std::fs::symlink_metadata(&from).is_err() {
        return Err("it is gone".into());
    }
    let to = parent.join(new);
    if std::fs::symlink_metadata(&to).is_ok() {
        return Err("that name is taken".into());
    }
    match std::fs::rename(&from, &to) {
        Ok(()) => Ok(to),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err("no permission to rename it here".into()),
        Err(e) => Err(format!("the system said: {}", e.kind())),
    }
}

/// Removes a file, a link or a folder with all that is in it. A link is removed as a link, never followed: what it points to is not touched, in the folder itself and in every folder below it.
pub(crate) fn remove(path: &Path) -> Result<(), String> {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return Err("it is gone".into()) };
    let done = if meta.file_type().is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    match done {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err("no permission to remove all of it".into()),
        Err(e) => Err(format!("the system said: {}", e.kind())),
    }
}

/// What the question of a delete says the thing is: a folder with its item count, a file with its size, a link with where it points; or that it is gone.
pub(crate) fn describe(path: &Path) -> Result<String, String> {
    describe_limited(path, COUNT_LIMIT)
}

/// `describe` with the count's bound given (the tests use a small one).
pub(crate) fn describe_limited(path: &Path, limit: usize) -> Result<String, String> {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return Err("it is gone".into()) };
    let kind = meta.file_type();
    if kind.is_symlink() {
        let to = std::fs::read_link(path).map(|p| p.display().to_string()).unwrap_or_default();
        Ok(format!("a link to {to}"))
    } else if kind.is_dir() {
        let (count, more) = count_items(path, limit);
        Ok(match (count, more) {
            (0, false) => "an empty folder".to_string(),
            (_, true) => format!("a folder with more than {limit} items"),
            (1, false) => "a folder with 1 item".to_string(),
            (n, false) => format!("a folder with {n} items"),
        })
    } else {
        Ok(format!("a file of {}", human_bytes(meta.len())))
    }
}

/// The items below `folder` (every folder, file and link, a link as one and never entered): the count, and whether it stopped at `limit` or at the time bound.
fn count_items(folder: &Path, limit: usize) -> (usize, bool) {
    let started = std::time::Instant::now();
    let (mut count, mut todo) = (0usize, vec![folder.to_path_buf()]);
    while let Some(dir) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            count += 1;
            if count > limit || started.elapsed() > COUNT_TIME {
                return (count.min(limit), true);
            }
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                todo.push(entry.path());
            }
        }
    }
    (count, false)
}

/// A size in words: bytes under a KiB, then one decimal of KiB, MiB or GiB.
pub(crate) fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 3] = ["KiB", "MiB", "GiB"];
    if n < 1024 {
        return format!("{n} bytes");
    }
    let (mut value, mut unit) = (n as f64 / 1024.0, 0);
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Whether `entry` (a path as the list shows it: the folder, then the name) may not be renamed or deleted: the root, the person's home folder and every folder above it. The entry's own last name is never resolved, so a link that points at one of them is only a link; the folder it stands in is, so the same folder reached by another way in is still protected.
pub(crate) fn protected(entry: &Path, home: Option<&Path>) -> bool {
    let (Some(parent), Some(name)) = (entry.parent(), entry.file_name()) else { return true };
    let at = parent.canonicalize().map_or_else(|_| entry.to_path_buf(), |p| p.join(name));
    home.and_then(|h| h.canonicalize().ok()).is_some_and(|h| h.starts_with(&at))
}

#[cfg(test)]
#[path = "dirops_tests.rs"]
mod tests;
