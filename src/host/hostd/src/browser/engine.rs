//! The engine file (M17-P06): where it lives, who may have written it, whether it is the pinned release, and the open file that is run. The file is opened ONCE, checked with
//! `fstat` on the open descriptor, hashed from that descriptor and run through that descriptor (`lico_sandbox::confine_fd`), so that what runs is what was hashed.
//! No environment variable and no flag skips the hash; the only other engine is the one the owner names in the owner-only file `browser.json` of the state directory.

use super::sha256;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The pinned release of the engine, the download page of its archive and the SHA-256 of the archive (for the person who downloads it), and the SHA-256 of the two files Lico checks.
pub use lico_engine_pack::{ARCHIVE_SHA256, ARCHIVE_URL, ENGINE_FILE, ENGINE_SHA256, RELEASE, WORKER_FILE, WORKER_SHA256};
/// The owner-only file of the state directory that names an engine that is not the pinned release.
pub const SETTING_FILE: &str = "browser.json";

extern "C" {
    fn getuid() -> u32;
}
const O_NOFOLLOW: i32 = 0o400_000;
const O_CLOEXEC: i32 = 0o2_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Nothing is installed where it should be: the message shows where to put it.
    Absent { dir: PathBuf },
    /// A file or a directory on the way may be written by someone else, is not a plain file or directory, or is not below the home directory.
    Unsafe(&'static str),
    /// The file is not the pinned release.
    Mismatch,
    /// The owner's setting file is damaged or not owner-only.
    Setting(&'static str),
    Io(&'static str),
}

/// The two hashes that are expected (the product's constants; a test gives its own).
pub struct Pins<'a> {
    pub engine: &'a str,
    pub worker: &'a str,
}

pub const PINNED: Pins<'static> = Pins { engine: ENGINE_SHA256, worker: WORKER_SHA256 };

/// What `locate` found: the open engine file (the one to run), its directory (the fence lets the program read it) and whether it is not the pinned release.
#[derive(Debug)]
pub struct Located {
    pub file: File,
    pub dir: PathBuf,
    pub unpinned: bool,
}

/// Where the host looks: the home directory, `XDG_DATA_HOME` if set, and the state directory (for the setting file).
#[derive(Clone, Debug)]
pub struct Where {
    pub home: PathBuf,
    pub xdg_data: Option<PathBuf>,
    pub state_dir: PathBuf,
}

impl Where {
    pub fn of_process(state_dir: PathBuf) -> Option<Where> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let xdg_data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute());
        home.is_absolute().then_some(Where { home, xdg_data, state_dir })
    }

    /// `DATA/lico/browser/v0.2.6`, DATA being `XDG_DATA_HOME` or `HOME/.local/share`.
    pub fn pinned_dir(&self) -> PathBuf {
        let data = self.xdg_data.clone().unwrap_or_else(|| self.home.join(".local/share"));
        data.join("lico/browser").join(RELEASE)
    }
}

fn mine(m: &std::fs::Metadata) -> bool {
    m.uid() == unsafe { getuid() }
}

/// Every directory from the one below `home` down to `dir` (itself included) is a real directory (no symbolic link), belongs to this user and cannot be written by the group or
/// others. The home directory itself is the owner's own business and is not checked.
pub fn check_chain(home: &Path, dir: &Path) -> Result<(), Problem> {
    let below = dir.strip_prefix(home).map_err(|_| Problem::Unsafe("the engine must be below the home directory"))?;
    let mut at = home.to_path_buf();
    for part in below.components() {
        at.push(part);
        let m = match std::fs::symlink_metadata(&at) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Problem::Absent { dir: dir.to_path_buf() }),
            Err(_) => return Err(Problem::Io("a directory on the way to the engine cannot be read")),
        };
        if m.file_type().is_symlink() {
            return Err(Problem::Unsafe("a directory on the way to the engine is a symbolic link"));
        }
        if !m.is_dir() {
            return Err(Problem::Unsafe("a part of the way to the engine is not a directory"));
        }
        if !mine(&m) {
            return Err(Problem::Unsafe("a directory on the way to the engine belongs to someone else"));
        }
        if m.permissions().mode() & 0o022 != 0 {
            return Err(Problem::Unsafe("a directory on the way to the engine can be written by others"));
        }
    }
    Ok(())
}

/// Opens `dir/name` once, without following a link, and checks the OPEN file: a regular file of this user that others cannot write, with an execute bit when `executable`.
fn open_checked(dir: &Path, name: &str, executable: bool) -> Result<File, Problem> {
    let file = match OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_CLOEXEC).open(dir.join(name)) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Problem::Absent { dir: dir.to_path_buf() }),
        Err(_) => return Err(Problem::Unsafe("the engine file cannot be opened (a symbolic link, or no right to read it)")),
    };
    let m = file.metadata().map_err(|_| Problem::Io("the engine file cannot be inspected"))?;
    if !m.is_file() {
        return Err(Problem::Unsafe("the engine is not a plain file"));
    }
    if !mine(&m) {
        return Err(Problem::Unsafe("the engine file belongs to someone else"));
    }
    if m.permissions().mode() & 0o022 != 0 {
        return Err(Problem::Unsafe("the engine file can be written by others"));
    }
    if executable && m.permissions().mode() & 0o100 == 0 {
        return Err(Problem::Unsafe("the engine file is not executable"));
    }
    Ok(file)
}

fn hash_of(file: &mut File) -> Result<String, Problem> {
    file.seek(SeekFrom::Start(0)).map_err(|_| Problem::Io("the engine file cannot be read"))?;
    sha256::hex_of(&mut *file).map_err(|_| Problem::Io("the engine file cannot be read"))
}

/// The pinned engine: the directory and every parent below home are safe, the open `obscura` is hashed from its descriptor and the `obscura-worker` next to it as well.
pub fn open_pinned(home: &Path, dir: &Path, pins: &Pins<'_>) -> Result<File, Problem> {
    check_chain(home, dir)?;
    let mut engine = open_checked(dir, ENGINE_FILE, true)?;
    if hash_of(&mut engine)? != pins.engine {
        return Err(Problem::Mismatch);
    }
    let mut worker = open_checked(dir, WORKER_FILE, false)?;
    if hash_of(&mut worker)? != pins.worker {
        return Err(Problem::Mismatch);
    }
    Ok(engine)
}

/// An engine the owner named: safe to the same degree, but not hashed (the tab says so).
pub fn open_unpinned(home: &Path, dir: &Path) -> Result<File, Problem> {
    check_chain(home, dir)?;
    open_checked(dir, ENGINE_FILE, true)
}

/// The owner's setting: `{"engine_dir": "/absolute/path"}` in an owner-only regular file. `Ok(None)` when there is no file.
pub fn read_setting(state_dir: &Path) -> Result<Option<PathBuf>, Problem> {
    let path = state_dir.join(SETTING_FILE);
    let file = match OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_CLOEXEC).open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Problem::Setting("the engine setting cannot be opened")),
    };
    let m = file.metadata().map_err(|_| Problem::Setting("the engine setting cannot be inspected"))?;
    if !m.is_file() || !mine(&m) || m.permissions().mode() & 0o077 != 0 {
        return Err(Problem::Setting("the engine setting must be a plain file of this user that only the owner can use"));
    }
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::Read::take(&file, 4096), &mut text).map_err(|_| Problem::Setting("the engine setting cannot be read"))?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| Problem::Setting("the engine setting is not JSON"))?;
    let Some(dir) = value.get("engine_dir").and_then(|v| v.as_str()) else {
        // a setting that only says `auto_install` or `stealth` names no engine
        return if value.get("auto_install").is_some() || value.get("stealth").is_some() { Ok(None) } else { Err(Problem::Setting("the engine setting has no engine_dir")) };
    };
    let dir = PathBuf::from(dir);
    if !dir.is_absolute() || dir.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::CurDir)) {
        return Err(Problem::Setting("engine_dir must be an absolute path without . or .."));
    }
    Ok(Some(dir))
}

/// One flag of the owner's setting file: `Some(b)` when the file is a plain owner-only file of this user, is JSON and holds `name` as a boolean; `None` otherwise (a missing, damaged
/// or group- or world-readable file says nothing).
fn setting_flag(state_dir: &Path, name: &str) -> Option<bool> {
    let file = OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_CLOEXEC).open(state_dir.join(SETTING_FILE)).ok()?;
    let m = file.metadata().ok()?;
    if !m.is_file() || !mine(&m) || m.permissions().mode() & 0o077 != 0 {
        return None;
    }
    let mut text = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::Read::take(&file, 4096), &mut text);
    serde_json::from_str::<serde_json::Value>(&text).ok()?.get(name)?.as_bool()
}

/// Whether the owner turned the self-install off (`{"auto_install": false}` in the owner-only setting file; nothing else turns it on or off, and a missing or unreadable file leaves it on).
pub fn auto_install_off(state_dir: &Path) -> bool {
    setting_flag(state_dir, "auto_install") == Some(false)
}

/// Whether the owner turned the engine's stealth mode on (`{"stealth": true}` in the owner-only setting file; the default is off, and a missing, damaged or group- or world-readable
/// file leaves it off). The owner's own choice (the chat, 2026-11-17): Lico may use it, and does not unless this says so.
pub fn stealth_on(state_dir: &Path) -> bool {
    setting_flag(state_dir, "stealth") == Some(true)
}

/// Finds the engine to run: the owner's setting if there is one (unpinned), else the pinned release.
pub fn locate(at: &Where, pins: &Pins<'_>) -> Result<Located, Problem> {
    if let Some(dir) = read_setting(&at.state_dir)? {
        let file = open_unpinned(&at.home, &dir)?;
        return Ok(Located { file, dir, unpinned: true });
    }
    let dir = at.pinned_dir();
    let file = open_pinned(&at.home, &dir, pins)?;
    Ok(Located { file, dir, unpinned: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::symlink;

    fn home(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lico_browser_engine_{name}_{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn mkdirs(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        let mut at = PathBuf::new();
        for part in path.components() {
            at.push(part);
        }
    }

    fn dir_mode(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    fn put(dir: &Path, name: &str, content: &[u8], mode: u32) -> PathBuf {
        let p = dir.join(name);
        let mut f = File::create(&p).unwrap();
        f.write_all(content).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        p
    }

    fn pins_of(engine: &[u8], worker: &[u8]) -> (String, String) {
        (sha256::hex_of(engine).unwrap(), sha256::hex_of(worker).unwrap())
    }

    /// A pinned install in a private home: HOME/.local/share/lico/browser/v0.2.6 with both files.
    fn installed(name: &str, engine: &[u8], worker: &[u8]) -> (PathBuf, PathBuf) {
        let h = home(name);
        let dir = Where { home: h.clone(), xdg_data: None, state_dir: h.join("state") }.pinned_dir();
        mkdirs(&dir);
        let mut at = h.clone();
        for part in dir.strip_prefix(&h).unwrap().components() {
            at.push(part);
            dir_mode(&at, 0o700);
        }
        put(&dir, ENGINE_FILE, engine, 0o700);
        put(&dir, WORKER_FILE, worker, 0o700);
        (h, dir)
    }

    #[test]
    fn the_pinned_constants_are_the_ones_of_the_release_that_was_measured() {
        assert_eq!(RELEASE, "v0.2.6");
        assert!(ARCHIVE_URL.ends_with("/v0.2.6/obscura-x86_64-linux.tar.gz") && ARCHIVE_URL.starts_with("https://github.com/"));
        for hash in [ARCHIVE_SHA256, ENGINE_SHA256, WORKER_SHA256] {
            assert!(hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')), "{hash}");
        }
        assert_eq!((ENGINE_FILE, WORKER_FILE, SETTING_FILE), ("obscura", "obscura-worker", "browser.json"));
    }

    #[test]
    fn the_pinned_dir_follows_xdg_data_home_else_the_home_directory() {
        let a = Where { home: "/h".into(), xdg_data: None, state_dir: "/s".into() };
        assert_eq!(a.pinned_dir(), PathBuf::from("/h/.local/share/lico/browser/v0.2.6"));
        let b = Where { home: "/h".into(), xdg_data: Some("/h/data".into()), state_dir: "/s".into() };
        assert_eq!(b.pinned_dir(), PathBuf::from("/h/data/lico/browser/v0.2.6"));
    }

    #[test]
    fn a_pinned_engine_whose_hashes_match_is_opened_and_one_that_differs_is_refused() {
        let (h, dir) = installed("pinned", b"engine bytes", b"worker bytes");
        let (pe, pw) = pins_of(b"engine bytes", b"worker bytes");
        assert!(open_pinned(&h, &dir, &Pins { engine: &pe, worker: &pw }).is_ok());
        let (other, _) = pins_of(b"another engine", b"");
        assert_eq!(open_pinned(&h, &dir, &Pins { engine: &other, worker: &pw }).unwrap_err(), Problem::Mismatch, "a different engine");
        assert_eq!(open_pinned(&h, &dir, &Pins { engine: &pe, worker: &other }).unwrap_err(), Problem::Mismatch, "a different worker next to it");
        // one changed byte in the worker is enough
        put(&dir, WORKER_FILE, b"worker byteS", 0o700);
        assert_eq!(open_pinned(&h, &dir, &Pins { engine: &pe, worker: &pw }).unwrap_err(), Problem::Mismatch);
    }

    #[test]
    fn nothing_installed_is_absent_and_names_the_directory() {
        let h = home("absent");
        let at = Where { home: h.clone(), xdg_data: None, state_dir: h.join("state") };
        match locate(&at, &PINNED) {
            Err(Problem::Absent { dir }) => assert_eq!(dir, at.pinned_dir()),
            other => panic!("{other:?}"),
        }
        // the directory exists and the file does not
        let (h2, dir) = installed("absent2", b"e", b"w");
        std::fs::remove_file(dir.join(ENGINE_FILE)).unwrap();
        assert!(matches!(open_pinned(&h2, &dir, &PINNED), Err(Problem::Absent { .. })));
    }

    #[test]
    fn a_directory_or_file_that_others_can_write_or_a_link_is_refused_at_every_level() {
        let (pe, pw) = pins_of(b"e", b"w");
        let pins = Pins { engine: &pe, worker: &pw };
        // the directory itself, and each parent below home, group- or world-writable
        for level in 0..5 {
            let (h, dir) = installed(&format!("writable{level}"), b"e", b"w");
            let parts: Vec<_> = dir.strip_prefix(&h).unwrap().components().collect();
            let mut at = h.clone();
            for part in &parts[..=level] {
                at.push(part);
            }
            dir_mode(&at, 0o770);
            assert!(matches!(open_pinned(&h, &dir, &pins), Err(Problem::Unsafe(_))), "level {level} group-writable");
            dir_mode(&at, 0o707);
            assert!(matches!(open_pinned(&h, &dir, &pins), Err(Problem::Unsafe(_))), "level {level} world-writable");
            dir_mode(&at, 0o755);
            assert!(open_pinned(&h, &dir, &pins).is_ok(), "level {level}: readable by others is fine, writable is not");
        }
        // the file itself
        let (h, dir) = installed("filemode", b"e", b"w");
        std::fs::set_permissions(dir.join(ENGINE_FILE), std::fs::Permissions::from_mode(0o770)).unwrap();
        assert!(matches!(open_pinned(&h, &dir, &pins), Err(Problem::Unsafe(_))));
        std::fs::set_permissions(dir.join(ENGINE_FILE), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(open_pinned(&h, &dir, &pins), Err(Problem::Unsafe(_))), "no execute bit");
        std::fs::set_permissions(dir.join(ENGINE_FILE), std::fs::Permissions::from_mode(0o500)).unwrap();
        assert!(open_pinned(&h, &dir, &pins).is_ok(), "the control: owner read and execute is accepted");
        std::fs::set_permissions(dir.join(WORKER_FILE), std::fs::Permissions::from_mode(0o660)).unwrap();
        assert!(matches!(open_pinned(&h, &dir, &pins), Err(Problem::Unsafe(_))), "the worker too");
        // a symbolic link as the file, and as a directory on the way
        let (h, dir) = installed("linkfile", b"e", b"w");
        let real = put(&dir, "real", b"e", 0o700);
        std::fs::remove_file(dir.join(ENGINE_FILE)).unwrap();
        symlink(&real, dir.join(ENGINE_FILE)).unwrap();
        assert!(matches!(open_pinned(&h, &dir, &pins), Err(Problem::Unsafe(_))), "a link in place of the file");
        let (h, dir) = installed("linkdir", b"e", b"w");
        let moved = h.join("moved");
        std::fs::rename(dir.parent().unwrap(), &moved).unwrap();
        symlink(&moved, dir.parent().unwrap()).unwrap();
        assert!(matches!(open_pinned(&h, &dir, &pins), Err(Problem::Unsafe(_))), "a link in place of a directory on the way");
    }

    #[test]
    fn stealth_is_off_unless_the_owner_only_setting_says_true() {
        let h = home("stealth");
        let state = h.join("state");
        std::fs::create_dir_all(&state).unwrap();
        assert!(!stealth_on(&state), "no file: off");
        for (content, mode, on, why) in [
            ("{\"stealth\": true}", 0o600, true, "owner-only and true"),
            ("{\"stealth\": true, \"auto_install\": false}", 0o600, true, "with another flag"),
            ("{\"stealth\": false}", 0o600, false, "false"),
            ("{\"stealth\": \"yes\"}", 0o600, false, "not a boolean"),
            ("{}", 0o600, false, "not there"),
            ("not json", 0o600, false, "not JSON"),
            ("{\"stealth\": true}", 0o640, false, "group-readable: ignored"),
            ("{\"stealth\": true}", 0o644, false, "world-readable: ignored"),
        ] {
            put(&state, SETTING_FILE, content.as_bytes(), mode);
            assert_eq!(stealth_on(&state), on, "{why}");
        }
        put(&state, SETTING_FILE, b"{\"stealth\": true}", 0o600);
        assert_eq!(read_setting(&state).unwrap(), None, "a setting that only says stealth names no engine");
        assert!(!auto_install_off(&state), "stealth does not touch the self-install");
        put(&state, SETTING_FILE, b"{\"auto_install\": false}", 0o600);
        assert!(!stealth_on(&state) && auto_install_off(&state), "and the self-install does not touch stealth");
    }

    #[test]
    fn an_engine_outside_the_home_directory_is_refused() {
        let h = home("outside");
        let elsewhere = home("elsewhere");
        put(&elsewhere, ENGINE_FILE, b"e", 0o700);
        assert!(matches!(check_chain(&h, &elsewhere), Err(Problem::Unsafe(_))));
        assert!(matches!(open_unpinned(&h, &elsewhere), Err(Problem::Unsafe(_))));
    }

    #[test]
    fn the_owners_setting_names_an_unpinned_engine_and_must_be_owner_only() {
        let h = home("setting");
        let state = h.join("state");
        std::fs::create_dir_all(&state).unwrap();
        let engine_dir = h.join("my-engine");
        std::fs::create_dir_all(&engine_dir).unwrap();
        dir_mode(&engine_dir, 0o700);
        put(&engine_dir, ENGINE_FILE, b"not the release", 0o700);
        assert_eq!(read_setting(&state).unwrap(), None, "no file, no setting");
        let at = Where { home: h.clone(), xdg_data: None, state_dir: state.clone() };
        put(&state, SETTING_FILE, format!("{{\"engine_dir\": {:?}}}", engine_dir.to_str().unwrap()).as_bytes(), 0o600);
        let found = locate(&at, &PINNED).unwrap();
        assert!(found.unpinned && found.dir == engine_dir, "the setting wins and is marked");
        for (content, mode, why) in [
            (format!("{{\"engine_dir\": {:?}}}", engine_dir.to_str().unwrap()), 0o640, "group-readable"),
            (format!("{{\"engine_dir\": {:?}}}", engine_dir.to_str().unwrap()), 0o644, "world-readable"),
            ("not json".to_string(), 0o600, "not JSON"),
            ("{}".to_string(), 0o600, "no engine_dir"),
            ("{\"engine_dir\": \"relative/dir\"}".to_string(), 0o600, "relative"),
            ("{\"engine_dir\": \"/a/../b\"}".to_string(), 0o600, "with .."),
        ] {
            put(&state, SETTING_FILE, content.as_bytes(), mode);
            assert!(matches!(read_setting(&state), Err(Problem::Setting(_))), "{why}");
            assert!(locate(&at, &PINNED).is_err(), "{why}: a damaged setting is an error, not a fall back to the pinned engine");
        }
    }

    #[test]
    fn the_open_file_is_what_was_hashed_even_when_the_path_is_replaced_after() {
        let (h, dir) = installed("swap", b"the real engine", b"w");
        let (pe, pw) = pins_of(b"the real engine", b"w");
        let opened = open_pinned(&h, &dir, &Pins { engine: &pe, worker: &pw }).unwrap();
        put(&dir, "other", b"a different program", 0o700);
        std::fs::rename(dir.join("other"), dir.join(ENGINE_FILE)).unwrap();
        let mut again = opened;
        assert_eq!(hash_of(&mut again).unwrap(), pe, "the descriptor still names the file that was checked");
    }
}
