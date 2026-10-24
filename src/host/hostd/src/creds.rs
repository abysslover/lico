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

pub fn add(dir: &Path, reference: &str, provider: &str, account: &str, kind: &str, masked_label: &str) -> std::io::Result<()> {
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
    let entries: Vec<Value> = list(dir)?.into_iter().filter(|e| e["ref"] != reference).collect();
    save(dir, &entries)
}
