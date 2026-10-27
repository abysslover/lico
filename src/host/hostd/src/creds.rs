//! The credential metadata store: opaque references, masked labels and never a secret value.
//! One JSON file with mode 0600 in an owner-only directory, replaced atomically.

use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub fn file(dir: &Path) -> PathBuf {
    dir.join("credentials.json")
}

pub fn random_ref() -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn list(dir: &Path) -> std::io::Result<Vec<Value>> {
    match std::fs::read_to_string(file(dir)) {
        Ok(text) => Ok(serde_json::from_str::<Vec<Value>>(&text).map_err(std::io::Error::other)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

fn save(dir: &Path, entries: &[Value]) -> std::io::Result<()> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let temporary = dir.join(format!(".credentials.{}.tmp", std::process::id()));
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)?;
    out.write_all(serde_json::to_string_pretty(entries).map_err(std::io::Error::other)?.as_bytes())?;
    out.sync_all()?;
    std::fs::rename(&temporary, file(dir))
}

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

/// An exclusive lock for a read-modify-write of the metadata (released when the file closes).
fn locked(dir: &Path) -> std::io::Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let lock = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(dir.join(".credentials.lock"))?;
    if unsafe { flock(lock.as_raw_fd(), 2) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(lock)
}

pub fn add(dir: &Path, reference: &str, provider: &str, account: &str, kind: &str, masked_label: &str) -> std::io::Result<()> {
    let _lock = locked(dir)?;
    let mut entries = list(dir)?;
    entries.push(json!({
        "ref": reference, "provider": provider, "account": account, "kind": kind, "masked_label": masked_label,
        "created": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()).to_string(),
    }));
    save(dir, &entries)
}

pub fn find(dir: &Path, reference: &str) -> std::io::Result<Option<Value>> {
    Ok(list(dir)?.into_iter().find(|e| e["ref"] == reference))
}

pub fn remove(dir: &Path, reference: &str) -> std::io::Result<()> {
    let _lock = locked(dir)?;
    let entries: Vec<Value> = list(dir)?.into_iter().filter(|e| e["ref"] != reference).collect();
    save(dir, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    extern "C" {
        fn umask(mask: u32) -> u32;
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    // Own folder and own umask rather than those of `handoff`: this module uses nothing else of the crate, and the test selection reads `crate::` to decide
    // which tests a change needs, so a use of `handoff` here would send a change of `core.rs` to the credential tests.
    #[test]
    fn the_credentials_folder_is_private_whichever_of_its_two_creation_sites_makes_it_and_so_are_the_lock_and_the_list() {
        unsafe { umask(0o022) };
        let base = std::env::temp_dir().join(format!("lico-creds-modes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let by_lock = base.join("by_lock").join("credentials");
        drop(locked(&by_lock).unwrap());
        let by_save = base.join("by_save").join("credentials");
        save(&by_save, &[]).unwrap();
        let by_add = base.join("by_add").join("credentials");
        add(&by_add, "ref1", "claude", "a1", "api_key", "key ending 0001").unwrap();
        for dir in [&by_lock, &by_save, &by_add] {
            assert_eq!(mode_of(dir), 0o700, "{}", dir.display());
        }
        assert_eq!(mode_of(&by_add.join(".credentials.lock")), 0o600);
        assert_eq!(mode_of(&file(&by_add)), 0o600);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
