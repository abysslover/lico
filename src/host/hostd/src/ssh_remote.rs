//! Agentless SSH remote execution helpers for isolated remote account sign-in (`M06-P02`)
//! and remote directory browsing & persistent `claude`/`codex` session execution (`M06-P03`).
//!
//! Remote account authentication state lives exclusively on the remote host under
//! `$HOME/.local/state/lico/accounts/<account_id>/{home,<provider>}` created with `umask 077` (`0700`).
//! Real remote `$HOME` binary directories (`~/.local/bin`, `~/.npm-global/bin`, `~/.cargo/bin`, `~/bin`)
//! are added to `PATH` before `HOME` is switched to the isolated account home so user-installed
//! `claude` and `codex` binaries resolve cleanly while keeping credentials isolated per account.

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::accounts::provider_variable;
use crate::core::{Reply, Shared};
use crate::folders::{refused_char, valid_effort, valid_model, valid_path_shape, Settings};
use crate::fsview::{decode_utf8_preview, is_html_name, sanitize_preview_text, uses_system_opener, MAX_ENTRIES, MAX_PREVIEW_BYTES};
use crate::session::{obj, Session};
use crate::ssh::{ssh_base_command, valid_ssh_host};

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

pub(crate) const REMOTE_OP_TIMEOUT_SECS: u64 = 4;
const REMOTE_PATH_SETUP: &str =
    "umask 077 && LICO_RH=\"$HOME\" && export PATH=\"$LICO_RH/.local/bin:$LICO_RH/.npm-global/bin:$LICO_RH/.cargo/bin:$LICO_RH/bin:/usr/local/bin:$PATH\"";

/// Escape an arbitrary string as a single POSIX shell token (`'...'`).
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Validates that an account identifier contains only safe ASCII identifier characters.
pub fn valid_account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// Restricts the local `ssh` client's environment to what OpenSSH needs (`HOME`, `PATH`, `LANG`,
/// `USER`, `LOGNAME`, `SSH_AUTH_SOCK`, `KRB5CCNAME`) so `DISPLAY` and unrelated local variables
/// never leak into remote execution.
pub fn sanitize_ssh_env(cmd: &mut Command) {
    cmd.env_clear();
    for key in ["HOME", "PATH", "LANG", "USER", "LOGNAME", "SSH_AUTH_SOCK", "KRB5CCNAME"] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
}

/// Executes a short non-interactive `sh -c <inner>` over `BatchMode=yes` SSH in its own process group,
/// killing the process group on timeout and bounding `stdout` to `max_out` bytes.
pub(crate) fn run_ssh_bounded(shared: &Shared, host: &str, inner: &str, max_out: usize, timeout_secs: u64) -> Result<(bool, String), &'static str> {
    let connect_secs = timeout_secs.clamp(1, 3);
    let mut cmd = ssh_base_command(shared, host, connect_secs)?;
    sanitize_ssh_env(&mut cmd);
    cmd.arg(format!("sh -c {}", shell_quote(inner)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);

    let mut child = cmd.spawn().map_err(|_| "ssh_unreachable")?;
    let out_pipe = child.stdout.take();
    let err_pipe = child.stderr.take();
    let (out_tx, out_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe {
            let _ = p.take(max_out as u64).read_to_end(&mut buf);
        }
        let _ = out_tx.send(String::from_utf8_lossy(&buf).into_owned());
    });
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe {
            let _ = p.take(8 * 1024).read_to_end(&mut buf);
        }
    });

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                unsafe {
                    kill(-(child.id() as i32), 9);
                }
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    match status {
        Some(st) if st.code() == Some(255) => Err("ssh_unreachable"),
        Some(st) => {
            let stdout = out_rx.recv_timeout(Duration::from_millis(200)).unwrap_or_default();
            Ok((st.success(), stdout))
        }
        None => Err("ssh_unreachable"),
    }
}

pub(crate) fn map_remote_err_pub(code: &str, fallback: &'static str) -> &'static str {
    map_remote_err(code, fallback)
}

fn map_remote_err(code: &str, fallback: &'static str) -> &'static str {
    match code.trim() {
        "folder_missing" => "folder_missing",
        "folder_not_directory" => "folder_not_directory",
        "folder_protected" => "folder_protected",
        "folder_unreadable" => "folder_unreadable",
        "file_missing" => "file_missing",
        "file_not_regular" => "file_not_regular",
        "file_unreadable" => "file_unreadable",
        _ => fallback,
    }
}

/// Lists one remote directory over SSH (`"~"` and `"~/..."` resolve against remote `$HOME`), returning canonical `cwd`,
/// `parent`, sorted `entries`, `truncated`, `show_hidden`, and `"ssh_host"`.
pub fn folder_list(shared: &Shared, host: &str, cwd: &str, show_hidden: bool) -> Reply {
    if !valid_ssh_host(host) || !valid_path_shape(cwd) {
        return Err("bad_request");
    }
    if cwd != "~" && !cwd.starts_with("~/") && !cwd.starts_with('/') {
        return Err("folder_not_absolute");
    }
    let py = concat!(
        "import json, os, stat, sys\n",
        "raw, show_hidden = sys.argv[1], sys.argv[2] == '1'\n",
        "target = os.path.expanduser(raw) if (raw == '~' or raw.startswith('~/')) else raw\n",
        "state_root = os.path.realpath(os.path.join(os.path.expanduser('~'), '.local', 'state', 'lico'))\n",
        "if not os.path.lexists(target) and not os.path.exists(target):\n",
        "    print('LICO_ERR=folder_missing'); sys.exit(0)\n",
        "try:\n",
        "    canon = os.path.realpath(target)\n",
        "except OSError:\n",
        "    print('LICO_ERR=folder_unreadable'); sys.exit(0)\n",
        "if not os.path.exists(canon):\n",
        "    print('LICO_ERR=folder_missing'); sys.exit(0)\n",
        "if not os.path.isdir(canon):\n",
        "    print('LICO_ERR=folder_not_directory'); sys.exit(0)\n",
        "if canon == state_root or canon.startswith(state_root + '/'):\n",
        "    print('LICO_ERR=folder_protected'); sys.exit(0)\n",
        "try:\n",
        "    it = list(os.scandir(canon))\n",
        "except OSError:\n",
        "    print('LICO_ERR=folder_unreadable'); sys.exit(0)\n",
        "items = []\n",
        "for e in it:\n",
        "    n = e.name\n",
        "    if not n or n in ('.', '..') or (not show_hidden and n.startswith('.')) or len(n) > 255:\n",
        "        continue\n",
        "    try:\n",
        "        if e.is_symlink():\n",
        "            try:\n",
        "                st = e.stat(follow_symlinks=True)\n",
        "                if stat.S_ISDIR(st.st_mode): items.append([n, 'dir', 0])\n",
        "                elif stat.S_ISREG(st.st_mode): items.append([n, 'file', int(st.st_size)])\n",
        "                else: items.append([n, 'link', 0])\n",
        "            except OSError:\n",
        "                items.append([n, 'link', 0])\n",
        "        elif e.is_dir(follow_symlinks=False): items.append([n, 'dir', 0])\n",
        "        elif e.is_file(follow_symlinks=False): items.append([n, 'file', int(e.stat(follow_symlinks=False).st_size)])\n",
        "    except OSError:\n",
        "        continue\n",
        "p = os.path.dirname(canon) if canon != '/' else None\n",
        "if p and (p == state_root or p.startswith(state_root + '/')):\n",
        "    p = None\n",
        "print('LICO_JSON=' + json.dumps({'cwd': canon, 'parent': p, 'items': items}))\n",
    );
    let inner = format!(
        "python3 -c {} {} {}",
        shell_quote(py),
        shell_quote(cwd),
        if show_hidden { "'1'" } else { "'0'" }
    );
    let (ok, stdout) = run_ssh_bounded(shared, host, &inner, 2 * 1024 * 1024, REMOTE_OP_TIMEOUT_SECS)?;
    if !ok {
        return Err("folder_unreadable");
    }
    for line in stdout.lines() {
        let trimmed = line.trim_end_matches('\r');
        if let Some(code) = trimmed.strip_prefix("LICO_ERR=") {
            return Err(map_remote_err(code, "folder_unreadable"));
        }
        if let Some(raw_json) = trimmed.strip_prefix("LICO_JSON=") {
            let parsed: Value = serde_json::from_str(raw_json).map_err(|_| "folder_unreadable")?;
            let canon = parsed["cwd"]
                .as_str()
                .filter(|s| s.starts_with('/') && valid_path_shape(s))
                .ok_or("folder_unreadable")?
                .to_string();
            let parent = parsed["parent"]
                .as_str()
                .filter(|s| s.starts_with('/') && *s != canon && valid_path_shape(s))
                .map(str::to_string);
            let mut rows: Vec<(String, &'static str, u64, bool, bool)> = Vec::new();
            for item in parsed["items"].as_array().into_iter().flatten() {
                let Some(arr) = item.as_array() else { continue };
                let Some(name) = arr.first().and_then(Value::as_str) else { continue };
                if name.is_empty()
                    || name == "."
                    || name == ".."
                    || (!show_hidden && name.starts_with('.'))
                    || name.chars().count() > 255
                    || name.chars().any(refused_char)
                {
                    continue;
                }
                let kind: &'static str = match arr.get(1).and_then(Value::as_str) {
                    Some("dir") => "dir",
                    Some("link") => "link",
                    Some("file") => "file",
                    _ => continue,
                };
                let size = arr.get(2).and_then(Value::as_u64).unwrap_or(0);
                let (html, ext) = if kind == "file" {
                    (is_html_name(name), uses_system_opener(name))
                } else {
                    (false, false)
                };
                rows.push((name.to_string(), kind, size, html, ext));
            }
            rows.sort_by(|a, b| {
                let da = u8::from(a.1 != "dir");
                let db = u8::from(b.1 != "dir");
                da.cmp(&db)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
                    .then_with(|| a.0.cmp(&b.0))
            });
            let truncated = rows.len() > MAX_ENTRIES;
            rows.truncate(MAX_ENTRIES);
            let entries: Vec<Value> = rows
                .into_iter()
                .map(|(name, kind, size, is_html, external)| {
                    json!({"name": name, "kind": kind, "size": size, "is_html": is_html, "external": external})
                })
                .collect();
            return Ok(obj(json!({
                "cwd": canon,
                "parent": parent,
                "entries": entries,
                "truncated": truncated,
                "show_hidden": show_hidden,
                "ssh_host": host,
            })));
        }
    }
    Err("folder_unreadable")
}

/// Reads up to `64 KiB` of a regular file on `host` over SSH (`O_NOFOLLOW | O_NONBLOCK` + `fstat`),
/// refusing symlinks, directories, special files, and private remote state roots.
pub fn folder_read(shared: &Shared, host: &str, path: &str) -> Reply {
    if !valid_ssh_host(host) || !valid_path_shape(path) {
        return Err("bad_request");
    }
    let p = Path::new(path);
    if !path.starts_with("~/") && !p.is_absolute() {
        return Err("folder_not_absolute");
    }
    if path == "/" || path == "~" || path.ends_with('/') {
        return Err("file_not_regular");
    }
    let Some(name) = p
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty() && *n != "." && *n != ".." && n.chars().count() <= 255 && !n.chars().any(refused_char))
    else {
        return Err("file_not_regular");
    };
    let py = concat!(
        "import json, os, stat, sys\n",
        "raw_in, max_b = sys.argv[1], int(sys.argv[2])\n",
        "raw = os.path.expanduser(raw_in) if raw_in.startswith('~/') else raw_in\n",
        "state_root = os.path.realpath(os.path.join(os.path.expanduser('~'), '.local', 'state', 'lico'))\n",
        "parent_raw = os.path.dirname(raw) or '/'\n",
        "name = os.path.basename(raw)\n",
        "if not os.path.exists(parent_raw):\n",
        "    print('LICO_ERR=folder_missing'); sys.exit(0)\n",
        "canon_parent = os.path.realpath(parent_raw)\n",
        "if not os.path.isdir(canon_parent):\n",
        "    print('LICO_ERR=folder_not_directory'); sys.exit(0)\n",
        "if canon_parent == state_root or canon_parent.startswith(state_root + '/'):\n",
        "    print('LICO_ERR=folder_protected'); sys.exit(0)\n",
        "target = os.path.join(canon_parent, name)\n",
        "try:\n",
        "    lst = os.lstat(target)\n",
        "except FileNotFoundError:\n",
        "    print('LICO_ERR=file_missing'); sys.exit(0)\n",
        "except OSError:\n",
        "    print('LICO_ERR=file_unreadable'); sys.exit(0)\n",
        "open_target = target\n",
        "if stat.S_ISLNK(lst.st_mode):\n",
        "    try:\n",
        "        open_target = os.path.realpath(target)\n",
        "        rst = os.lstat(open_target)\n",
        "    except FileNotFoundError:\n",
        "        print('LICO_ERR=file_missing'); sys.exit(0)\n",
        "    except OSError:\n",
        "        print('LICO_ERR=file_unreadable'); sys.exit(0)\n",
        "    if open_target == state_root or open_target.startswith(state_root + '/'):\n",
        "        print('LICO_ERR=folder_protected'); sys.exit(0)\n",
        "    if not stat.S_ISREG(rst.st_mode):\n",
        "        print('LICO_ERR=file_not_regular'); sys.exit(0)\n",
        "elif not stat.S_ISREG(lst.st_mode):\n",
        "    if stat.S_ISDIR(lst.st_mode):\n",
        "        cd = os.path.realpath(target)\n",
        "        if cd == state_root or cd.startswith(state_root + '/'):\n",
        "            print('LICO_ERR=folder_protected'); sys.exit(0)\n",
        "    print('LICO_ERR=file_not_regular'); sys.exit(0)\n",
        "flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)\n",
        "try:\n",
        "    fd = os.open(open_target, flags)\n",
        "    try:\n",
        "        fst = os.fstat(fd)\n",
        "        if not stat.S_ISREG(fst.st_mode):\n",
        "            print('LICO_ERR=file_not_regular'); sys.exit(0)\n",
        "        chunks, rem = [], max_b + 1\n",
        "        while rem > 0:\n",
        "            b = os.read(fd, rem)\n",
        "            if not b: break\n",
        "            chunks.append(b); rem -= len(b)\n",
        "        data = b''.join(chunks)\n",
        "        print('LICO_JSON=' + json.dumps({'path': target, 'cwd': canon_parent, 'size': int(fst.st_size), 'hex': data.hex()}))\n",
        "    finally:\n",
        "        os.close(fd)\n",
        "except OSError:\n",
        "    print('LICO_ERR=file_unreadable')\n",
    );
    let inner = format!("python3 -c {} {} '{}'", shell_quote(py), shell_quote(path), MAX_PREVIEW_BYTES);
    let (ok, stdout) = run_ssh_bounded(shared, host, &inner, 256 * 1024, REMOTE_OP_TIMEOUT_SECS)?;
    if !ok {
        return Err("file_unreadable");
    }
    for line in stdout.lines() {
        let trimmed = line.trim_end_matches('\r');
        if let Some(code) = trimmed.strip_prefix("LICO_ERR=") {
            return Err(map_remote_err(code, "file_unreadable"));
        }
        if let Some(raw_json) = trimmed.strip_prefix("LICO_JSON=") {
            let parsed: Value = serde_json::from_str(raw_json).map_err(|_| "file_unreadable")?;
            let full_str = parsed["path"]
                .as_str()
                .filter(|s| s.starts_with('/') && valid_path_shape(s))
                .ok_or("file_unreadable")?
                .to_string();
            let canon_parent = parsed["cwd"]
                .as_str()
                .filter(|s| s.starts_with('/') && valid_path_shape(s))
                .ok_or("file_unreadable")?
                .to_string();
            let size = parsed["size"].as_u64().unwrap_or(0);
            let hex_str = parsed["hex"].as_str().ok_or("file_unreadable")?;
            let mut raw = crate::payload::unhex(hex_str).ok_or("file_unreadable")?;
            let truncated = raw.len() > MAX_PREVIEW_BYTES;
            if truncated {
                raw.truncate(MAX_PREVIEW_BYTES);
            }
            let Some(decoded) = decode_utf8_preview(&raw, truncated) else {
                return Ok(obj(json!({
                    "path": full_str,
                    "cwd": canon_parent,
                    "name": name,
                    "size": size,
                    "binary": true,
                    "truncated": false,
                    "text": "",
                    "ssh_host": host,
                })));
            };
            let clean = sanitize_preview_text(decoded);
            return Ok(obj(json!({
                "path": full_str,
                "cwd": canon_parent,
                "name": name,
                "size": size,
                "binary": false,
                "truncated": truncated,
                "text": clean,
                "ssh_host": host,
            })));
        }
    }
    Err("file_unreadable")
}

/// Validates and canonicalizes a remote directory on `host` over SSH, refusing protected `~/.local/state/lico` paths.
pub fn check_remote_folder(shared: &Shared, host: &str, text: &str) -> Result<String, &'static str> {
    if !valid_ssh_host(host) || !valid_path_shape(text) {
        return Err("bad_request");
    }
    if text != "~" && !text.starts_with("~/") && !text.starts_with('/') {
        return Err("folder_not_absolute");
    }
    let py = concat!(
        "import os, sys\n",
        "raw_in = sys.argv[1]\n",
        "raw = os.path.expanduser(raw_in) if (raw_in == '~' or raw_in.startswith('~/')) else raw_in\n",
        "state_root = os.path.realpath(os.path.join(os.path.expanduser('~'), '.local', 'state', 'lico'))\n",
        "if not os.path.lexists(raw) and not os.path.exists(raw):\n",
        "    print('LICO_ERR=folder_missing'); sys.exit(0)\n",
        "canon = os.path.realpath(raw)\n",
        "if not os.path.exists(canon):\n",
        "    print('LICO_ERR=folder_missing'); sys.exit(0)\n",
        "if not os.path.isdir(canon):\n",
        "    print('LICO_ERR=folder_not_directory'); sys.exit(0)\n",
        "if canon == state_root or canon.startswith(state_root + '/'):\n",
        "    print('LICO_ERR=folder_protected'); sys.exit(0)\n",
        "try:\n",
        "    os.listdir(canon)\n",
        "except OSError:\n",
        "    print('LICO_ERR=folder_unreadable'); sys.exit(0)\n",
        "print('LICO_OK=' + canon)\n",
    );
    let inner = format!("python3 -c {} {}", shell_quote(py), shell_quote(text));
    let (ok, stdout) = run_ssh_bounded(shared, host, &inner, 8 * 1024, REMOTE_OP_TIMEOUT_SECS)?;
    if !ok {
        return Err("folder_unreadable");
    }
    for line in stdout.lines() {
        let trimmed = line.trim_end_matches('\r');
        if let Some(code) = trimmed.strip_prefix("LICO_ERR=") {
            return Err(map_remote_err(code, "folder_unreadable"));
        }
        if let Some(canon) = trimmed.strip_prefix("LICO_OK=") {
            if canon.starts_with('/') && valid_path_shape(canon) {
                return Ok(canon.to_string());
            }
        }
    }
    Err("folder_unreadable")
}

/// Checks whether `cwd` on `host` is inside a git repository (`folder.git` with `ssh_host`).
pub fn folder_git(shared: &Shared, host: &str, cwd: &str) -> Reply {
    if !valid_ssh_host(host) {
        return Err("bad_request");
    }
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Ok(obj(json!({"is_git": false, "repo": Value::Null, "ssh_host": host})));
    }
    let canon = match check_remote_folder(shared, host, trimmed) {
        Ok(c) => c,
        Err(code) => return Ok(obj(json!({"is_git": false, "repo": Value::Null, "reason": code, "ssh_host": host}))),
    };
    let inner = format!("git -C {} rev-parse --show-toplevel 2>/dev/null", shell_quote(&canon));
    match run_ssh_bounded(shared, host, &inner, 4 * 1024, REMOTE_OP_TIMEOUT_SECS) {
        Ok((true, out)) => {
            let repo = out.lines().next().map(str::trim).unwrap_or("");
            if repo.starts_with('/') && valid_path_shape(repo) {
                Ok(obj(json!({"is_git": true, "repo": repo, "ssh_host": host})))
            } else {
                Ok(obj(json!({"is_git": false, "repo": Value::Null, "ssh_host": host})))
            }
        }
        _ => Ok(obj(json!({"is_git": false, "repo": Value::Null, "ssh_host": host}))),
    }
}

/// Validates `Settings` for `session.create`: delegates to `folders::prepare` for local targets and
/// validates over SSH (`check_remote_folder`, refusing local `worktree_branch`) when the target account is SSH remote.
pub fn prepare_session(shared: &Shared, kind: &str, account: Option<&str>, mut chosen: Settings) -> Result<Settings, &'static str> {
    let ssh_host = shared.accounts.lock().unwrap().target_ssh_host(kind, account);
    let Some(host) = ssh_host else {
        return crate::folders::prepare(shared, kind, chosen);
    };
    if chosen.worktree_branch.is_some() {
        return Err("option_unavailable");
    }
    if let Some(ref rid) = chosen.resume_id {
        if !matches!(kind, "claude" | "codex") {
            return Err("option_unavailable");
        }
        if !crate::provider_record::valid_provider_session(rid) {
            return Err("bad_request");
        }
    }
    if let Some(folder) = &chosen.cwd {
        if kind == "synthetic" {
            return Err("option_unavailable");
        }
        chosen.cwd = Some(check_remote_folder(shared, &host, folder)?);
    }
    if chosen.model.is_some() || chosen.effort.is_some() {
        if kind != "claude" {
            return Err("option_unavailable");
        }
        if chosen.model.as_deref().is_some_and(|m| !valid_model(m))
            || chosen.effort.as_deref().is_some_and(|e| !valid_effort(e))
        {
            return Err("bad_request");
        }
    }
    chosen.name = chosen.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    Ok(chosen)
}

/// Resolves or creates the working directory for an SSH remote session (`s.settings.cwd`, or remote
/// `0700` scratch workspace `$HOME/.local/state/lico/workspaces/<session_id>`).
pub fn ensure_remote_workspace(shared: &Shared, host: &str, session: &Arc<Mutex<Session>>) -> Result<String, &'static str> {
    let (id, cwd) = {
        let s = session.lock().unwrap();
        (s.id.clone(), s.settings.cwd.clone())
    };
    if let Some(folder) = cwd {
        return check_remote_folder(shared, host, &folder);
    }
    if !crate::registry::valid_session_id(&id) {
        return Err("bad_request");
    }
    let inner = format!(
        "umask 077 && d=\"$HOME/.local/state/lico/workspaces/\"{} && mkdir -p \"$d\" && cd \"$d\" && pwd -P",
        shell_quote(&id)
    );
    let (ok, stdout) = run_ssh_bounded(shared, host, &inner, 4 * 1024, REMOTE_OP_TIMEOUT_SECS)?;
    if !ok {
        return Err("busy");
    }
    let dir = stdout.lines().next().map(|s| s.trim_end_matches('\r')).unwrap_or("");
    if dir.starts_with('/') && valid_path_shape(dir) {
        Ok(dir.to_string())
    } else {
        Err("busy")
    }
}

/// Builds the non-interactive `ssh` command that runs `claude auth login` or `codex login [--device-auth]`
/// (or `codex login status`) on `host` inside the remote machine's `0700` isolated account home.
pub fn remote_login_command(
    shared: &Shared,
    host: &str,
    account_id: &str,
    provider: &str,
    words: &[&str],
) -> Result<Command, &'static str> {
    if !valid_account_id(account_id) {
        return Err("bad_request");
    }
    let var = provider_variable(provider).ok_or("bad_request")?;
    let mut cmd = ssh_base_command(shared, host, 8)?;
    sanitize_ssh_env(&mut cmd);

    let mut argv = Vec::new();
    match provider {
        "claude" => {
            argv.push("claude".to_string());
            argv.push("auth".to_string());
        }
        "codex" => {
            argv.push("codex".to_string());
        }
        _ => return Err("bad_request"),
    }
    for w in words {
        argv.push(shell_quote(w));
    }
    let exec_line = argv.join(" ");
    let acct_q = shell_quote(account_id);
    let prov_q = shell_quote(provider);
    let inner = format!(
        "{REMOTE_PATH_SETUP} && \
         LICO_AC=\"$LICO_RH/.local/state/lico/accounts/\"{acct_q} && \
         mkdir -p \"$LICO_AC/home\" \"$LICO_AC/\"{prov_q} && \
         cd \"$LICO_AC/home\" && \
         HOME=\"$LICO_AC/home\" {var}=\"$LICO_AC/\"{prov_q} NO_COLOR=1 BROWSER=/usr/bin/true exec {exec_line}"
    );
    cmd.arg(format!("sh -c {}", shell_quote(&inner)));
    Ok(cmd)
}

/// Builds the streaming `ssh` command for a remote `claude` session bound to `(account_id, host)`.
pub fn remote_claude_command(
    shared: &Shared,
    host: &str,
    account_id: &str,
    remote_cwd: &str,
    mode: &str,
    resume: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<Command, &'static str> {
    if !valid_account_id(account_id) || !remote_cwd.starts_with('/') || !valid_path_shape(remote_cwd) {
        return Err("bad_request");
    }
    let mut cmd = ssh_base_command(shared, host, 8)?;
    sanitize_ssh_env(&mut cmd);

    let mut argv = vec!["claude".to_string()];
    for flag in [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--replay-user-messages",
        "--setting-sources",
        "user",
        "--permission-mode",
        if mode == "default" { "default" } else { "bypassPermissions" },
        "--permission-prompt-tool",
        "stdio",
    ] {
        argv.push(shell_quote(flag));
    }
    if let Some(r) = resume {
        argv.push("'--resume'".to_string());
        argv.push(shell_quote(r));
    }
    if let Some(m) = model {
        argv.push("'--model'".to_string());
        argv.push(shell_quote(m));
    }
    if let Some(e) = effort {
        argv.push("'--effort'".to_string());
        argv.push(shell_quote(e));
    }
    let exec_line = argv.join(" ");
    let acct_q = shell_quote(account_id);
    let cwd_q = shell_quote(remote_cwd);
    let inner = format!(
        "{REMOTE_PATH_SETUP} && \
         LICO_AC=\"$LICO_RH/.local/state/lico/accounts/\"{acct_q} && \
         mkdir -p \"$LICO_AC/home\" \"$LICO_AC/claude\" && \
         cd {cwd_q} && \
         HOME=\"$LICO_AC/home\" CLAUDE_CONFIG_DIR=\"$LICO_AC/claude\" LANG=C.UTF-8 \
         NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost exec {exec_line}"
    );
    cmd.arg(format!("sh -c {}", shell_quote(&inner)));
    Ok(cmd)
}

/// Builds the streaming `ssh` command for a remote `codex app-server` session bound to `(account_id, host)`.
pub fn remote_codex_command(
    shared: &Shared,
    host: &str,
    account_id: &str,
    remote_cwd: &str,
) -> Result<Command, &'static str> {
    if !valid_account_id(account_id) || !remote_cwd.starts_with('/') || !valid_path_shape(remote_cwd) {
        return Err("bad_request");
    }
    let mut cmd = ssh_base_command(shared, host, 8)?;
    sanitize_ssh_env(&mut cmd);

    let exec_line = "codex 'app-server'";
    let acct_q = shell_quote(account_id);
    let cwd_q = shell_quote(remote_cwd);
    let inner = format!(
        "{REMOTE_PATH_SETUP} && \
         LICO_AC=\"$LICO_RH/.local/state/lico/accounts/\"{acct_q} && \
         mkdir -p \"$LICO_AC/home\" \"$LICO_AC/codex\" && \
         cd {cwd_q} && \
         HOME=\"$LICO_AC/home\" CODEX_HOME=\"$LICO_AC/codex\" LANG=C.UTF-8 exec {exec_line}"
    );
    cmd.arg(format!("sh -c {}", shell_quote(&inner)));
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_and_remote_validators_reject_unsafe_inputs() {
        assert_eq!(shell_quote("simple"), "'simple'");
        assert_eq!(shell_quote("a b'c"), "'a b'\\''c'");
        assert!(valid_account_id("a012345abcdef"));
        assert!(!valid_account_id(""));
        assert!(!valid_account_id("../bad"));
        assert!(!valid_account_id("a;rm"));
        assert_eq!(map_remote_err("folder_protected\n", "folder_unreadable"), "folder_protected");
        assert_eq!(map_remote_err("unexpected", "file_unreadable"), "file_unreadable");
    }
}
