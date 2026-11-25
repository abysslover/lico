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

#[cfg(test)]
#[path = "dirops_tests.rs"]
mod tests;
