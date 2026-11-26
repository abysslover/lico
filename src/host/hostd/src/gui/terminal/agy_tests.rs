//! Whether `agy` is found on this machine (`M23-P04c`): a folder of the `PATH` or one of the two folders of the home, an executable regular file, nothing else. What a click on the picture or on
//! the card of a conversation does is in `newsession_wizard_tests.rs` and the real window (`tests/test_agy_tile.py`, `tests/test_agy_chats.py`).

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::agy::installed;

// ---- M23-P04c: the `agy` picture of New session ----

/// A folder under the system's temporary one that is removed when it goes out of scope.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        let dir = std::env::temp_dir().join(format!("lico_agyfind_{}", crate::core::random_hex(8)));
        std::fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }

    fn at(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A file named `agy` in `dir` with the permission bits `mode`.
fn program(dir: &Path, mode: u32) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("agy"), "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(dir.join("agy"), std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn agy_is_found_as_an_executable_file_in_a_folder_of_the_path_or_in_the_two_folders_of_the_home() {
    let d = Dir::new();
    let (bin, other) = (d.at("bin"), d.at("other"));
    program(&bin, 0o755);
    std::fs::create_dir_all(&other).unwrap();
    let path = std::env::join_paths([&other, &bin]).unwrap();
    assert!(installed(Some(&path), None), "in a folder of the PATH, not the first");
    assert!(!installed(Some(other.as_os_str()), None), "no program in the folder");
    assert!(!installed(None, None) && !installed(Some(OsStr::new("")), None), "no PATH, an empty one");
    for folder in [".local/bin", "bin"] {
        let home = Dir::new();
        program(&home.at(folder), 0o700);
        assert!(installed(None, Some(&home.0)), "{folder} of the home");
    }
    let home = Dir::new();
    program(&home.at(".local/share/bin"), 0o755);
    assert!(!installed(None, Some(&home.0)), "only those two folders of the home");
}

#[test]
fn a_file_that_cannot_run_a_folder_a_dangling_link_and_a_relative_entry_are_not_agy_and_a_link_to_a_program_is() {
    let d = Dir::new();
    program(&d.at("noexec"), 0o644);
    std::fs::create_dir_all(d.at("dir/agy")).unwrap();
    std::fs::create_dir_all(d.at("gone")).unwrap();
    std::os::unix::fs::symlink(d.at("nowhere"), d.at("gone/agy")).unwrap();
    for folder in ["noexec", "dir", "gone"] {
        assert!(!installed(Some(d.at(folder).as_os_str()), None), "{folder}");
    }
    program(&d.at("real"), 0o755);
    std::fs::create_dir_all(d.at("links")).unwrap();
    std::os::unix::fs::symlink(d.at("real/agy"), d.at("links/agy")).unwrap();
    assert!(installed(Some(d.at("links").as_os_str()), None), "a link to a program counts");
    // a relative entry of the PATH is the folder the window happens to be in: never looked in, though it resolves from here
    let up = "../".repeat(std::env::current_dir().unwrap().components().count() - 1);
    let rel = format!("{up}{}", d.at("real").to_string_lossy().trim_start_matches('/'));
    assert!(Path::new(&rel).join("agy").is_file(), "the relative path resolves from here: {rel}");
    assert!(!installed(Some(OsStr::new(&rel)), None), "{rel}");
}
