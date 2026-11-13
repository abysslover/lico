//! SSH chat scanning and `.jsonl` transcript seeding for resumed `claude` and `codex`
//! sessions (`M07-P03`).

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::codex::{clip, sanitize};
use crate::core::Shared;
use crate::folders::{derive_auto_title, valid_path_shape};
use crate::provider_record::valid_provider_session;
use crate::session::Session;
use crate::ssh::valid_ssh_host;
use crate::ssh_remote::{map_remote_err_pub, run_ssh_bounded, shell_quote, REMOTE_OP_TIMEOUT_SECS};

use super::{
    claude_msg_text, clean_title, codex_msg_text, is_codex_system_text, norm_path,
    open_regular_nofollow, real_dir, unbound_codex_home, ChatItem, MAX_LINES_PER_TURN,
    MAX_SCAN_FILES, MAX_SEED_BYTES, MAX_SEED_TURNS,
};

pub(super) fn scan_ssh_chats(
    shared: &Shared,
    host: &str,
    raw_cwd: Option<&str>,
    remote_accounts: &[(String, String, String)],
) -> Result<(Option<String>, Vec<ChatItem>), &'static str> {
    if !valid_ssh_host(host) {
        return Err("bad_request");
    }
    if let Some(c) = raw_cwd {
        if !valid_path_shape(c) {
            return Err("bad_request");
        }
        if c != "~" && !c.starts_with("~/") && !c.starts_with('/') {
            return Err("folder_not_absolute");
        }
    }
    let accts_json = serde_json::to_string(
        &remote_accounts
            .iter()
            .map(|(id, prov, lbl)| json!([id, prov, lbl]))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".into());
    let cwd_arg = raw_cwd.unwrap_or("");
    let py = concat!(
        "import json, os, stat, sys\n",
        "raw_cwd, accts = sys.argv[1], json.loads(sys.argv[2])\n",
        "home = os.path.expanduser('~')\n",
        "state_root = os.path.realpath(os.path.join(home, '.local', 'state', 'lico'))\n",
        "ws_root = os.path.join(state_root, 'workspaces')\n",
        "canon_cwd = None\n",
        "if raw_cwd:\n",
        "    exp = os.path.expanduser(raw_cwd) if (raw_cwd == '~' or raw_cwd.startswith('~/')) else raw_cwd\n",
        "    if not os.path.lexists(exp) and not os.path.exists(exp):\n",
        "        print('LICO_ERR=folder_missing'); sys.exit(0)\n",
        "    canon_cwd = os.path.realpath(exp)\n",
        "    if not os.path.exists(canon_cwd): print('LICO_ERR=folder_missing'); sys.exit(0)\n",
        "    if not os.path.isdir(canon_cwd): print('LICO_ERR=folder_not_directory'); sys.exit(0)\n",
        "    if canon_cwd == state_root or canon_cwd.startswith(state_root + '/'):\n",
        "        print('LICO_ERR=folder_protected'); sys.exit(0)\n",
        "    try: os.listdir(canon_cwd)\n",
        "    except OSError: print('LICO_ERR=folder_unreadable'); sys.exit(0)\n",
        "def match_dir(c):\n",
        "    if not c or not c.startswith('/'): return False\n",
        "    nc = c.rstrip('/') or '/'\n",
        "    if canon_cwd is not None: return nc == (canon_cwd.rstrip('/') or '/')\n",
        "    return nc == ws_root or nc.startswith(ws_root + '/')\n",
        "flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)\n",
        "def open_reg(p):\n",
        "    try:\n",
        "        st = os.lstat(p)\n",
        "        if stat.S_ISLNK(st.st_mode) or not stat.S_ISREG(st.st_mode): return None\n",
        "        fd = os.open(p, flags)\n",
        "        fst = os.fstat(fd)\n",
        "        if not stat.S_ISREG(fst.st_mode): os.close(fd); return None\n",
        "        buf = os.read(fd, 65536); os.close(fd)\n",
        "        return buf.decode('utf-8', 'replace'), int(fst.st_mtime)\n",
        "    except OSError: return None\n",
        "items = []\n",
        "for aid, prov, lbl in accts:\n",
        "    base = os.path.join(state_root, 'accounts', aid, prov)\n",
        "    if prov == 'claude':\n",
        "        proj = os.path.join(base, 'projects')\n",
        "        if not os.path.isdir(proj) or os.path.islink(proj): continue\n",
        "        for d in os.listdir(proj)[:500]:\n",
        "            dp = os.path.join(proj, d)\n",
        "            if not os.path.isdir(dp) or os.path.islink(dp): continue\n",
        "            for fn in os.listdir(dp)[:500]:\n",
        "                if not fn.endswith('.jsonl'): continue\n",
        "                sid = fn[:-6]\n",
        "                got = open_reg(os.path.join(dp, fn))\n",
        "                if not got: continue\n",
        "                txt, mt = got; fc, fp = None, ''\n",
        "                for ln in txt.splitlines()[:50]:\n",
        "                    try: v = json.loads(ln)\n",
        "                    except Exception: continue\n",
        "                    if fc is None and isinstance(v.get('cwd'), str):\n",
        "                        if not match_dir(v['cwd']): fc = ''; break\n",
        "                        fc = v['cwd']\n",
        "                    if not fp and v.get('type') == 'user' and not v.get('isSidechain') and not v.get('isMeta'):\n",
        "                        mc = (v.get('message') or {}).get('content')\n",
        "                        if isinstance(mc, str) and mc.strip(): fp = mc\n",
        "                        elif isinstance(mc, list):\n",
        "                            ts = [b.get('text','') for b in mc if isinstance(b,dict) and b.get('type')=='text' and b.get('text','').strip()]\n",
        "                            if ts: fp = '\\n'.join(ts)\n",
        "                if fc: items.append({'p': 'claude', 's': sid, 'a': aid, 'l': lbl, 'c': fc, 't': fp[:200], 'u': mt})\n",
        "    elif prov == 'codex':\n",
        "        sess = os.path.join(base, 'sessions')\n",
        "        if not os.path.isdir(sess) or os.path.islink(sess): continue\n",
        "        stack = [(sess, 0)]\n",
        "        while stack:\n",
        "            cur, dep = stack.pop()\n",
        "            if dep > 5: continue\n",
        "            try: names = os.listdir(cur)\n",
        "            except OSError: continue\n",
        "            for fn in names[:500]:\n",
        "                p = os.path.join(cur, fn)\n",
        "                try: st = os.lstat(p)\n",
        "                except OSError: continue\n",
        "                if stat.S_ISLNK(st.st_mode): continue\n",
        "                if stat.S_ISDIR(st.st_mode): stack.append((p, dep + 1)); continue\n",
        "                if not fn.endswith('.jsonl') or not stat.S_ISREG(st.st_mode): continue\n",
        "                got = open_reg(p)\n",
        "                if not got: continue\n",
        "                txt, mt = got; mid, fc, fp = None, None, ''\n",
        "                for ln in txt.splitlines()[:50]:\n",
        "                    try: v = json.loads(ln)\n",
        "                    except Exception: continue\n",
        "                    vt = v.get('type')\n",
        "                    if vt == 'session_meta':\n",
        "                        pl = v.get('payload') or {}\n",
        "                        if isinstance(pl.get('id'), str): mid = pl['id']\n",
        "                        if isinstance(pl.get('cwd'), str):\n",
        "                            if not match_dir(pl['cwd']): fc = ''; break\n",
        "                            fc = pl['cwd']\n",
        "                    elif not fp and vt == 'response_item':\n",
        "                        pl = v.get('payload') or {}\n",
        "                        if pl.get('type') == 'message' and pl.get('role') == 'user':\n",
        "                            for b in (pl.get('content') or []):\n",
        "                                if isinstance(b, dict) and isinstance(b.get('text'), str) and not b['text'].lstrip().startswith(('<environment_context>', '<permissions instructions>', '<user_instructions>', '<INSTRUCTIONS>', '# AGENTS.md instructions')):\n",
        "                                    if b['text'].strip(): fp = b['text']; break\n",
        "                sid = mid or (fn[:-6][-36:] if fn.startswith('rollout-') and len(fn[:-6]) > 36 else fn[:-6])\n",
        "                if fc and sid: items.append({'p': 'codex', 's': sid, 'a': aid, 'l': lbl, 'c': fc, 't': fp[:200], 'u': mt})\n",
        "print('LICO_JSON=' + json.dumps({'cwd': canon_cwd, 'items': items}))\n",
    );
    let inner = format!("python3 -c {} {} {}", shell_quote(py), shell_quote(cwd_arg), shell_quote(&accts_json));
    let (ok, stdout) = run_ssh_bounded(shared, host, &inner, 512 * 1024, REMOTE_OP_TIMEOUT_SECS)?;
    if !ok {
        return Err("ssh_unreachable");
    }
    for line in stdout.lines() {
        let trimmed = line.trim_end_matches('\r');
        if let Some(code) = trimmed.strip_prefix("LICO_ERR=") {
            return Err(map_remote_err_pub(code, "folder_unreadable"));
        }
        if let Some(raw_json) = trimmed.strip_prefix("LICO_JSON=") {
            let parsed: Value = serde_json::from_str(raw_json).map_err(|_| "folder_unreadable")?;
            let canon = parsed["cwd"]
                .as_str()
                .filter(|s| s.starts_with('/') && valid_path_shape(s))
                .map(|s| norm_path(s).to_string());
            let is_glob = raw_cwd.is_none();
            let mut out = Vec::new();
            for entry in parsed["items"].as_array().into_iter().flatten() {
                let Some(prov) = entry["p"].as_str().filter(|p| matches!(*p, "claude" | "codex")) else { continue };
                let Some(sid) = entry["s"].as_str().filter(|s| valid_provider_session(s)) else { continue };
                let aid = entry["a"].as_str().map(str::to_string);
                let lbl = entry["l"].as_str().map(str::to_string);
                let c = entry["c"].as_str().filter(|s| valid_path_shape(s)).map(|s| norm_path(s).to_string());
                let raw_t = entry["t"].as_str().unwrap_or("");
                let u = entry["u"].as_u64().unwrap_or(0);
                out.push(ChatItem {
                    provider: prov.to_string(),
                    provider_session: sid.to_string(),
                    account: aid,
                    account_label: lbl,
                    ssh_host: Some(host.to_string()),
                    cwd: if is_glob { None } else { canon.clone().or(c) },
                    global: is_glob,
                    title: clean_title(raw_t, prov, sid),
                    updated_at: u,
                    open_session: None,
                    open_state: None,
                    file: None,
                    turns: None,
                    recorded: true,
                    live: None,
                });
            }
            return Ok((canon, out));
        }
    }
    Err("folder_unreadable")
}

fn locate_codex_file(home: &Path, resume_id: &str) -> Option<PathBuf> {
    fn search(dir: &Path, depth: usize, left: &mut usize, rid: &str, fallback: &mut Option<PathBuf>) -> Option<PathBuf> {
        if depth > 5 || *left == 0 || !real_dir(dir) {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            if *left == 0 {
                return None;
            }
            let p = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
            if meta.file_type().is_dir() {
                if let Some(found) = search(&p, depth + 1, left, rid, fallback) {
                    return Some(found);
                }
            } else if meta.file_type().is_file() {
                *left -= 1;
                let Some(name) = p.file_name().and_then(|n| n.to_str()) else { continue };
                let Some(stem) = name.strip_suffix(".jsonl") else { continue };
                if stem == rid || stem.ends_with(&format!("-{rid}")) {
                    return Some(p);
                }
                if fallback.is_none() {
                    if let Some((file, _, _)) = open_regular_nofollow(&p) {
                        let mut first = String::new();
                        if BufReader::new(file.take(8192)).read_line(&mut first).is_ok() {
                            if let Ok(v) = serde_json::from_str::<Value>(&first) {
                                if v.pointer("/payload/id").and_then(Value::as_str) == Some(rid) {
                                    *fallback = Some(p);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
    let mut left = MAX_SCAN_FILES;
    let mut fallback = None;
    search(&home.join("sessions"), 0, &mut left, resume_id, &mut fallback).or(fallback)
}

fn read_turns_from_jsonl(provider: &str, reader: impl Read) -> Vec<(bool, String)> {
    let mut turns: Vec<(bool, String)> = Vec::new();
    let mut fallback_events: Vec<(bool, String)> = Vec::new();
    for line in BufReader::new(reader).lines().take(2000).flatten() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if provider == "claude" {
            match v.get("type").and_then(Value::as_str) {
                Some("user") => {
                    if let Some(t) = claude_msg_text(&v, true) {
                        turns.push((true, t));
                    }
                }
                Some("assistant") => {
                    if let Some(t) = claude_msg_text(&v, false) {
                        turns.push((false, t));
                    }
                }
                _ => {}
            }
        } else if let Some(pair) = codex_msg_text(&v) {
            turns.push(pair);
        } else if v.get("type").and_then(Value::as_str) == Some("event_msg") {
            let ptype = v.pointer("/payload/type").and_then(Value::as_str).unwrap_or("");
            if ptype == "user_message" {
                if let Some(m) = v.pointer("/payload/message").and_then(Value::as_str) {
                    if !is_codex_system_text(m) && !m.trim().is_empty() {
                        fallback_events.push((true, m.to_string()));
                    }
                }
            } else if ptype == "agent_message" {
                if let Some(m) = v.pointer("/payload/message").and_then(Value::as_str) {
                    if !m.trim().is_empty() {
                        fallback_events.push((false, m.to_string()));
                    }
                }
            }
        }
    }
    if turns.is_empty() {
        fallback_events
    } else {
        turns
    }
}

fn read_ssh_turns(shared: &Shared, host: &str, account_id: &str, provider: &str, resume_id: &str) -> Vec<(bool, String)> {
    let py = concat!(
        "import os, stat, sys\n",
        "aid, prov, rid = sys.argv[1], sys.argv[2], sys.argv[3]\n",
        "base = os.path.realpath(os.path.join(os.path.expanduser('~'), '.local', 'state', 'lico', 'accounts', aid, prov))\n",
        "flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)\n",
        "target = None\n",
        "if prov == 'claude':\n",
        "    proj = os.path.join(base, 'projects')\n",
        "    if os.path.isdir(proj) and not os.path.islink(proj):\n",
        "        for d in os.listdir(proj)[:500]:\n",
        "            cand = os.path.join(proj, d, rid + '.jsonl')\n",
        "            if os.path.isfile(cand) and not os.path.islink(cand): target = cand; break\n",
        "else:\n",
        "    sess = os.path.join(base, 'sessions')\n",
        "    stack = [(sess, 0)] if (os.path.isdir(sess) and not os.path.islink(sess)) else []\n",
        "    while stack and not target:\n",
        "        cur, dep = stack.pop()\n",
        "        if dep > 5: continue\n",
        "        for fn in os.listdir(cur)[:500]:\n",
        "            p = os.path.join(cur, fn)\n",
        "            try: st = os.lstat(p)\n",
        "            except OSError: continue\n",
        "            if stat.S_ISLNK(st.st_mode): continue\n",
        "            if stat.S_ISDIR(st.st_mode): stack.append((p, dep + 1))\n",
        "            elif fn == rid + '.jsonl' or fn.endswith('-' + rid + '.jsonl'): target = p; break\n",
        "if target:\n",
        "    fd = os.open(target, flags)\n",
        "    st = os.fstat(fd)\n",
        "    if stat.S_ISREG(st.st_mode):\n",
        "        if st.st_size > 262144: os.lseek(fd, st.st_size - 262144, os.SEEK_SET)\n",
        "        sys.stdout.buffer.write(os.read(fd, 262144))\n",
        "    os.close(fd)\n",
    );
    let inner = format!(
        "python3 -c {} {} {} {}",
        shell_quote(py),
        shell_quote(account_id),
        shell_quote(provider),
        shell_quote(resume_id)
    );
    let Ok((true, stdout)) = run_ssh_bounded(shared, host, &inner, MAX_SEED_BYTES as usize, REMOTE_OP_TIMEOUT_SECS) else {
        return Vec::new();
    };
    read_turns_from_jsonl(provider, stdout.as_bytes())
}

/// Pre-seeds recent sanitized user and assistant lines from `resume_id`'s `.jsonl` transcript into `s`
/// so a session resumed from `[ Chats ]` immediately displays its prior conversation context and title.
pub fn seed_resumed_transcript(shared: &Shared, s: &mut Session, resume_id: &str) {
    if !valid_provider_session(resume_id) {
        return;
    }
    let remote = s
        .account
        .as_deref()
        .and_then(|acc| shared.accounts.lock().unwrap().ssh_host(acc).map(|h| (acc.to_string(), h)));
    let turns = if let Some((acc_id, host)) = remote {
        read_ssh_turns(shared, &host, &acc_id, s.provider, resume_id)
    } else if s.provider == "claude" {
        crate::provider_record::home_of(shared, s.account.as_deref())
            .and_then(|h| crate::provider_record::locate(&h, resume_id).ok())
            .map(|f| read_turns_from_jsonl("claude", f.take(MAX_SEED_BYTES)))
            .unwrap_or_default()
    } else {
        let home = match s.account.as_deref() {
            Some(acc) => shared.accounts.lock().unwrap().homes(acc).map(|h| h.provider_home),
            None => unbound_codex_home(),
        };
        home.and_then(|h| locate_codex_file(&h, resume_id))
            .and_then(|p| open_regular_nofollow(&p))
            .map(|(f, _, _)| read_turns_from_jsonl("codex", f.take(MAX_SEED_BYTES)))
            .unwrap_or_default()
    };

    if s.auto_title.is_none() {
        s.auto_title = turns.iter().filter(|(u, _)| *u).find_map(|(_, first_user)| derive_auto_title(first_user));
    }
    s.append(format!("[provider reconnected] resumed {resume_id}").into_bytes());
    let start_idx = turns.len().saturating_sub(MAX_SEED_TURNS);
    for (is_user, text) in &turns[start_idx..] {
        for (idx, raw_line) in text.lines().take(MAX_LINES_PER_TURN).enumerate() {
            if *is_user {
                let prefix = if idx == 0 { ">> " } else { ">>   " };
                s.append(clip(&sanitize(&format!("{prefix}{raw_line}")), 4096).into_bytes());
            } else {
                s.append(crate::leads::agent_record(raw_line, 4096).into_bytes());
            }
        }
    }
    s.last_active_at = crate::session::now_secs();
}
