//! Release acceptance: the committed tree is cloned twice into differently named places and built twice with the repository's release script; the artifacts must be the same
//! bytes and hold nothing of where they were built (the repository path, the clone path, the home folder, `/private/`). The Python `suite acceptance` is the model.

use crate::sha256;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run(command: &mut Command, what: &str) -> Result<(), String> {
    let out = command.output().map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed:\n{}", String::from_utf8_lossy(&out.stderr).lines().rev().take(15).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")))
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Builds `script` (a path inside the repository, called as `sh script OUTDIR`) in two clean clones of `repo` below `scratch` and returns the digest of `artifact`.
pub fn run_acceptance(repo: &Path, script: &str, artifact: &str, scratch: &Path) -> Result<String, String> {
    let repo = repo.canonicalize().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(scratch).map_err(|e| e.to_string())?;
    let scratch = scratch.canonicalize().map_err(|e| e.to_string())?; // the build runs inside the clone: a relative path would be taken from there
    let scratch = scratch.as_path();
    let mut digests = Vec::new();
    for number in 1..=2 {
        let clone: PathBuf = scratch.join(format!("clone-{number}-in-a-differently-named-place"));
        let out = scratch.join(format!("out-{number}"));
        let _ = std::fs::remove_dir_all(&clone);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(scratch).map_err(|e| e.to_string())?;
        run(Command::new("git").args(["clone", "-q", "--no-hardlinks"]).arg(&repo).arg(&clone), "git clone")?;
        let mut build = Command::new("sh");
        build.arg(clone.join(script)).arg(&out).current_dir(&clone).env_remove("CARGO_TARGET_DIR").env("CARGO_NET_OFFLINE", "true");
        run(&mut build, &format!("the build in clone {number}"))?;
        let bytes = std::fs::read(out.join(artifact)).map_err(|e| format!("clone {number}: no {artifact}: {e}"))?;
        let home = std::env::var("HOME").unwrap_or_default();
        let mut forbidden: Vec<String> = vec![repo.to_string_lossy().to_string(), clone.to_string_lossy().to_string(), "/private/".to_string()];
        if let Some(parent) = repo.parent() {
            forbidden.push(parent.to_string_lossy().to_string());
        }
        if !home.is_empty() && home != "/" {
            forbidden.push(home);
        }
        for f in &forbidden {
            if contains(&bytes, f.as_bytes()) {
                return Err(format!("the artifact of clone {number} holds where it was built: {f}"));
            }
        }
        digests.push(sha256::hex(&bytes));
        let _ = std::fs::remove_dir_all(&clone);
        let _ = std::fs::remove_dir_all(out.join("target"));
    }
    if digests[0] != digests[1] {
        return Err(format!("the two builds differ: {} and {}", digests[0], digests[1]));
    }
    Ok(digests[0].clone())
}
