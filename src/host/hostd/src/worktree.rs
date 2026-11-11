//! Git worktree sessions (M02-P06, S01; WT-01, WT-02, SF-07).
//!
//! When a person starts a session in a git repository and asks for a new worktree and branch, Lico creates `<workspace_root>/worktrees/<session_id>` with
//! `git -C <repo> worktree add -b <branch> <wt_path>` and runs the session inside `<wt_path>`.
//! Safety rules (SF-07):
//! - Lico never pushes, never runs `git reset --hard`, and never modifies `<repo>`'s own working tree.
//! - Closing or deleting a session removes `<wt_path>` only when `worktree == Some("remove")` was explicitly requested.
//! - If `<wt_path>` has uncommitted or untracked changes (`git status --porcelain`), removal is refused with `worktree_dirty` unless `force_dirty` is true.
//! - Removing `<wt_path>` keeps the git branch (`refs/heads/<branch>`) intact.

use serde_json::{json, Value};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::core::{Reply, Shared};
use crate::session::{obj, Session};

pub const MAX_BRANCH: usize = 80;
const GIT_LIMIT: Duration = Duration::from_secs(5);

/// Shape of a git branch name before asking `git check-ref-format --branch`: non-empty, bounded, never a leading `-` (cannot be read as an option),
/// no hidden/control characters, no spaces, and none of git's forbidden sequences.
pub fn valid_branch_shape(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_BRANCH
        && !text.starts_with('-')
        && !text.starts_with('.')
        && !text.starts_with('/')
        && !text.ends_with('/')
        && !text.ends_with('.')
        && !text.ends_with(".lock")
        && !text.contains("..")
        && !text.contains("//")
        && !text.contains("@{")
        && !text.chars().any(|c| c.is_whitespace() || crate::folders::refused_char(c) || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
}

fn run_git(args: &[&str]) -> Option<(bool, String)> {
    let mut cmd = Command::new("git");
    cmd.args(args).env_clear().env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    for key in ["PATH", "LANG"] {
        if let Some(val) = std::env::var_os(key) {
            cmd.env(key, val);
        }
    }
    let mut child = cmd.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let deadline = Instant::now() + GIT_LIMIT;
    let status = loop {
        match child.try_wait().ok()? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let mut buf = Vec::new();
    let _ = stdout.take(64 * 1024).read_to_end(&mut buf);
    Some((status.success(), String::from_utf8_lossy(&buf).into_owned()))
}

/// Validates and normalizes a branch name via `valid_branch_shape` and `git check-ref-format --branch`.
pub fn check_branch(text: &str) -> Result<String, &'static str> {
    let trimmed = text.trim();
    if !valid_branch_shape(trimmed) {
        return Err("invalid_branch");
    }
    match run_git(&["check-ref-format", "--branch", trimmed]) {
        Some((true, out)) => {
            let clean = out.trim();
            if valid_branch_shape(clean) { Ok(clean.to_string()) } else { Err("invalid_branch") }
        }
        _ => Err("invalid_branch"),
    }
}

/// Canonical git repository root containing `folder`, or `None` when `folder` is not inside a git working tree.
pub fn git_toplevel(folder: &str) -> Option<String> {
    let (ok, out) = run_git(&["-C", folder, "rev-parse", "--show-toplevel"])?;
    if !ok {
        return None;
    }
    let canonical = std::fs::canonicalize(out.trim()).ok()?;
    canonical.to_str().filter(|s| crate::folders::valid_path_shape(s)).map(str::to_string)
}

/// Socket op `folder.git`: tells `lico gui`'s new-session screen whether `cwd` is inside a valid git repository.
pub fn folder_git_op(shared: &Shared, cwd: &str) -> Reply {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Ok(obj(json!({"is_git": false, "repo": Value::Null})));
    }
    let canonical = match crate::folders::check_folder(shared, trimmed) {
        Ok(c) => c,
        Err(code) => return Ok(obj(json!({"is_git": false, "repo": Value::Null, "reason": code}))),
    };
    match git_toplevel(&canonical) {
        Some(repo) => Ok(obj(json!({"is_git": true, "repo": repo}))),
        None => Ok(obj(json!({"is_git": false, "repo": Value::Null}))),
    }
}

pub fn worktree_dir(shared: &Shared, id: &str) -> PathBuf {
    shared.cfg.workspace_root.join("worktrees").join(id)
}

pub fn exists_in(workspace_root: &Path, id: &str) -> bool {
    crate::registry::valid_session_id(id)
        && std::fs::symlink_metadata(workspace_root.join("worktrees").join(id)).is_ok_and(|m| m.file_type().is_dir())
}

pub fn exists(shared: &Shared, id: &str) -> bool {
    exists_in(&shared.cfg.workspace_root, id)
}

/// Verifies at start/resume time that a worktree session's directory is still its own real directory under `<workspace_root>/worktrees/<id>`.
pub fn check_worktree_dir(shared: &Shared, id: &str, folder: &str) -> Result<PathBuf, &'static str> {
    if !crate::registry::valid_session_id(id) {
        return Err("bad_request");
    }
    let expected = worktree_dir(shared, id);
    let meta = std::fs::symlink_metadata(&expected).map_err(|_| "folder_missing")?;
    if !meta.file_type().is_dir() {
        return Err("folder_protected");
    }
    let canonical = std::fs::canonicalize(&expected).map_err(|_| "folder_unreadable")?;
    let given = std::fs::canonicalize(folder).map_err(|_| "folder_missing")?;
    if canonical != given || std::fs::read_dir(&canonical).is_err() {
        return Err("folder_protected");
    }
    Ok(canonical)
}

/// Creates a new git worktree and branch `<branch>` at `<workspace_root>/worktrees/<id>` from `repo`.
pub fn create(shared: &Shared, repo: &str, id: &str, branch: &str) -> Result<PathBuf, &'static str> {
    create_in(&shared.cfg.workspace_root, repo, id, branch)
}

fn create_in(workspace_root: &Path, repo: &str, id: &str, branch: &str) -> Result<PathBuf, &'static str> {
    if !crate::registry::valid_session_id(id) {
        return Err("bad_request");
    }
    let branch = check_branch(branch)?;
    let ref_name = format!("refs/heads/{branch}");
    if matches!(run_git(&["-C", repo, "show-ref", "--verify", "--quiet", &ref_name]), Some((true, _))) {
        return Err("branch_exists");
    }
    let parent = workspace_root.join("worktrees");
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&parent).map_err(|_| "worktree_failed")?;
    let wt = parent.join(id);
    if std::fs::symlink_metadata(&wt).is_ok() {
        return Err("busy");
    }
    let wt_str = wt.to_str().ok_or("worktree_failed")?;
    match run_git(&["-C", repo, "worktree", "add", "-b", &branch, wt_str]) {
        Some((true, _)) => {}
        _ => {
            if std::fs::symlink_metadata(&wt).is_ok_and(|m| m.file_type().is_dir()) {
                let _ = std::fs::remove_dir_all(&wt);
                let _ = run_git(&["-C", repo, "worktree", "prune"]);
            }
            return Err("worktree_failed");
        }
    }
    let _ = std::fs::set_permissions(&wt, std::fs::Permissions::from_mode(0o700));
    std::fs::canonicalize(&wt).map_err(|_| "worktree_failed")
}

/// True when `wt` has modified, staged, or untracked files (or when git cannot inspect `wt`).
pub fn is_dirty(wt: &Path) -> bool {
    let Some(wt_str) = wt.to_str() else { return true };
    match run_git(&["-C", wt_str, "status", "--porcelain", "--untracked-files=normal"]) {
        Some((true, out)) => !out.trim().is_empty(),
        _ => true,
    }
}

/// Checks before closing or purging a session whether the caller asked to remove its worktree and whether uncommitted changes block removal.
/// Returns `Ok(true)` when `remove_checked` should run once the session's process has been stopped.
pub fn check_before_remove(shared: &Shared, s: &Session, policy: Option<&str>, force_dirty: bool) -> Result<bool, &'static str> {
    check_before_remove_in(&shared.cfg.workspace_root, &s.id, s.settings.worktree_repo.is_some(), policy, force_dirty)
}

fn check_before_remove_in(workspace_root: &Path, id: &str, owns_worktree: bool, policy: Option<&str>, force_dirty: bool) -> Result<bool, &'static str> {
    if policy != Some("remove") || !owns_worktree || !exists_in(workspace_root, id) {
        return Ok(false);
    }
    let wt = workspace_root.join("worktrees").join(id);
    if !force_dirty && is_dirty(&wt) {
        return Err("worktree_dirty");
    }
    Ok(true)
}

/// Removes `<workspace_root>/worktrees/<id>` after `check_before_remove` approved it, keeping `<branch>` and leaving `<repo>` untouched (SF-07).
pub fn remove_checked(shared: &Shared, repo: Option<&str>, id: &str) {
    remove_checked_in(&shared.cfg.workspace_root, repo, id);
}

fn remove_checked_in(workspace_root: &Path, repo: Option<&str>, id: &str) {
    if !exists_in(workspace_root, id) {
        return;
    }
    let wt = workspace_root.join("worktrees").join(id);
    if let (Some(repo), Some(wt_str)) = (repo, wt.to_str()) {
        let _ = run_git(&["-C", repo, "worktree", "remove", "--force", wt_str]);
    }
    if std::fs::symlink_metadata(&wt).is_ok_and(|m| m.file_type().is_dir()) {
        let _ = std::fs::remove_dir_all(&wt);
        if let Some(repo) = repo {
            let _ = run_git(&["-C", repo, "worktree", "prune"]);
        }
    }
}

pub fn annotate(shared: &Shared, entry: &mut Value, s: &Session) {
    if let (Some(repo), Some(branch)) = (&s.settings.worktree_repo, &s.settings.worktree_branch) {
        entry["worktree"] = json!({
            "repo": repo,
            "branch": branch,
            "exists": exists(shared, &s.id),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &Path) -> String {
        std::fs::create_dir_all(dir).unwrap();
        let s = dir.to_str().unwrap();
        assert!(run_git(&["-C", s, "init", "-b", "main"]).unwrap().0);
        assert!(run_git(&["-C", s, "config", "user.email", "test@example.invalid"]).unwrap().0);
        assert!(run_git(&["-C", s, "config", "user.name", "Test"]).unwrap().0);
        std::fs::write(dir.join("README.md"), "root\n").unwrap();
        assert!(run_git(&["-C", s, "add", "README.md"]).unwrap().0);
        assert!(run_git(&["-C", s, "commit", "-m", "initial"]).unwrap().0);
        std::fs::canonicalize(dir).unwrap().to_str().unwrap().to_string()
    }

    #[test]
    fn branch_validation_accepts_normal_names_and_rejects_options_hidden_chars_and_bad_refs() {
        assert_eq!(check_branch("feat/login-fix").unwrap(), "feat/login-fix");
        assert_eq!(check_branch("  fix_123  ").unwrap(), "fix_123");
        for bad in ["", "   ", "-f", "--orphan", ".hidden", "a..b", "bad space", "bad~1", "bad:ref", "end.lock", "end/", "a\u{200b}b", &"x".repeat(81)] {
            assert!(check_branch(bad).is_err(), "accepted bad branch: {bad:?}");
        }
    }

    #[test]
    fn create_dirty_check_and_remove_keep_the_branch_and_never_touch_the_parent_repo() {
        let base = std::env::temp_dir().join(format!("lico_wt_test_{}", crate::core::random_hex(6)));
        let repo = init_repo(&base.join("repo"));
        std::fs::write(Path::new(&repo).join("parent_untracked.txt"), "keep me\n").unwrap();
        let ws = base.join("workspaces");
        let id = "s0123456789abcdef";
        let wt = create_in(&ws, &repo, id, "feat/wt-one").expect("create worktree");
        assert!(wt.join("README.md").is_file() && exists_in(&ws, id));
        assert_eq!(check_in_err(create_in(&ws, &repo, "s89abcdef01234567", "feat/wt-one")), "branch_exists");

        // Clean worktree: allowed without force_dirty, default policy ("keep") does not remove.
        assert_eq!(check_before_remove_in(&ws, id, true, None, false), Ok(false));
        assert_eq!(check_before_remove_in(&ws, id, true, Some("keep"), false), Ok(false));
        assert_eq!(check_before_remove_in(&ws, id, true, Some("remove"), false), Ok(true));

        // Dirty worktree (untracked file): refused without force_dirty, allowed with force_dirty.
        std::fs::write(wt.join("draft.txt"), "uncommitted\n").unwrap();
        assert!(is_dirty(&wt));
        assert_eq!(check_before_remove_in(&ws, id, true, Some("remove"), false), Err("worktree_dirty"));
        assert_eq!(check_before_remove_in(&ws, id, true, Some("remove"), true), Ok(true));

        remove_checked_in(&ws, Some(&repo), id);
        assert!(!exists_in(&ws, id) && !wt.exists());
        // Branch is kept, parent repo working tree (including its own untracked file) is untouched (SF-07).
        assert!(run_git(&["-C", &repo, "show-ref", "--verify", "--quiet", "refs/heads/feat/wt-one"]).unwrap().0);
        assert_eq!(std::fs::read_to_string(Path::new(&repo).join("parent_untracked.txt")).unwrap(), "keep me\n");
        let _ = std::fs::remove_dir_all(&base);
    }

    fn check_in_err(r: Result<PathBuf, &'static str>) -> &'static str {
        r.expect_err("expected error")
    }
}
