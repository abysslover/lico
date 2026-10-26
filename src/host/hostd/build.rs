//! Bakes the commit this program is built from into it (`LICO_BUILT_COMMIT`), for the title of the window, Help > About and `lico --version` (src/ident.rs).
//! The commit is, in this order: the environment variable `LICO_COMMIT` (whoever builds from an exported tree sets it: seven to forty hex digits, optionally
//! followed by `-dirty`), else the short commit of the checkout (`-dirty` after it when the package has changes that are not committed), else `unknown`.
//! Std only.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let dir = std::env::var_os("CARGO_MANIFEST_DIR").map_or_else(|| PathBuf::from("."), PathBuf::from);
    println!("cargo:rerun-if-env-changed=LICO_COMMIT");
    // the sources, because a change in them is what `-dirty` looks at; a directory is scanned by cargo as a whole (the target directory is not in the package)
    for path in ["build.rs", "Cargo.toml", "Cargo.lock", "NOTICES.txt", "build-release.sh", "src"] {
        println!("cargo:rerun-if-changed={path}");
    }
    watch_the_checkout(&dir);
    let given = std::env::var("LICO_COMMIT").unwrap_or_default();
    let from_variable = if given.trim().is_empty() {
        None
    } else {
        let found = from_variable(&given);
        if found.is_none() {
            println!("cargo:warning=LICO_COMMIT is not 7 to 40 hex digits, optionally followed by -dirty; the commit of the checkout is used");
        }
        found
    };
    let commit = from_variable.or_else(|| from_checkout(&dir)).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=LICO_BUILT_COMMIT={commit}");
}

/// The text of a git command run in the package folder, or None when git is missing, this is no checkout or the command fails.
fn git(dir: &PathBuf, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("--no-optional-locks").args(args).current_dir(dir).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 7 to 40 hex digits, optionally followed by `-dirty`, as 7 lower-case digits and the mark.
fn from_variable(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_lowercase();
    let (digits, dirty) = text.strip_suffix("-dirty").map_or((text.as_str(), false), |d| (d, true));
    if !(7..=40).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{}{}", &digits[..7], if dirty { "-dirty" } else { "" }))
}

/// The short commit of the checkout; `-dirty` when the package has a change that is not committed (a change elsewhere in the tree is not in the program),
/// and also when git cannot say.
fn from_checkout(dir: &PathBuf) -> Option<String> {
    let commit = git(dir, &["rev-parse", "--short=7", "HEAD"]).filter(|c| !c.is_empty() && c.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let clean = git(dir, &["status", "--porcelain", "--", "."]).is_some_and(|changes| changes.is_empty());
    Some(if clean { commit } else { format!("{commit}-dirty") })
}

/// Cargo builds again when the commit moves: the HEAD of this checkout (a linked worktree has its own), the branch it is on, and the packed refs.
/// The paths come from git, because they are not under the package folder.
fn watch_the_checkout(dir: &PathBuf) {
    let path_of = |name: &str| git(dir, &["rev-parse", "--git-path", name]).filter(|p| !p.is_empty());
    let Some(head) = path_of("HEAD") else { return };
    println!("cargo:rerun-if-changed={head}");
    if let Some(packed) = path_of("packed-refs") {
        println!("cargo:rerun-if-changed={packed}");
    }
    let text = std::fs::read_to_string(dir.join(&head)).unwrap_or_default();
    if let Some(branch) = text.strip_prefix("ref: ").map(str::trim).and_then(|r| path_of(r)) {
        println!("cargo:rerun-if-changed={branch}");
    }
}
