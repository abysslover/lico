//! Agentless SSH host discovery (`~/.ssh/config` + `Include`) and bounded non-interactive probing (`M06-P01`).
//!
//! Host aliases are validated strictly (`^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$`), never begin with `-`, and are always
//! passed after `--` to `Command::new(&shared.cfg.ssh_bin)` with `-T -o BatchMode=yes` so an untrusted or unconfigured
//! host can neither inject `ssh` options nor block indefinitely on interactive password prompts.

use std::collections::HashSet;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::core::{Reply, Shared};
use crate::provider_record::{O_NOFOLLOW, O_NONBLOCK};
use crate::session::obj;

pub const MAX_SSH_CONFIG_BYTES: u64 = 256 * 1024;
pub const MAX_SSH_HOSTS: usize = 128;
const MAX_INCLUDE_DEPTH: usize = 4;
const MAX_INCLUDE_FILES: usize = 32;
pub const SSH_PROBE_TIMEOUT_SECS: u64 = 6;

pub const REMOTE_PATH_EXPORT: &str =
    "export PATH=\"$HOME/.local/bin:$HOME/.npm-global/bin:$HOME/bin:/usr/local/bin:$PATH\";";

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshHost {
    pub host: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
}

/// Validates a concrete SSH host alias: `1..=128` ASCII characters, starting with `[A-Za-z0-9]` and continuing with
/// `[A-Za-z0-9._-]`. Never accepts leading `-`, whitespace, wildcards (`*`, `?`, `!`), `@`, `/`, or control characters.
pub fn valid_ssh_host(s: &str) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-'))
}

/// Builds a non-interactive `ssh` command targeting `host` with `-T -o BatchMode=yes` and optional `-F <ssh_config>`.
pub fn ssh_base_command(shared: &Shared, host: &str, connect_timeout_secs: u64) -> Result<Command, &'static str> {
    if !valid_ssh_host(host) {
        return Err("bad_request");
    }
    let mut cmd = Command::new(&shared.cfg.ssh_bin);
    if let Some(cfg_path) = &shared.cfg.ssh_config {
        cmd.arg("-F").arg(cfg_path);
    }
    cmd.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        &format!("ConnectTimeout={connect_timeout_secs}"),
        "--",
        host,
    ]);
    Ok(cmd)
}

fn read_regular_config(path: &Path) -> Option<String> {
    let lmeta = std::fs::symlink_metadata(path).ok()?;
    if lmeta.file_type().is_symlink() || !lmeta.file_type().is_file() {
        return None;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_NONBLOCK)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_SSH_CONFIG_BYTES {
        return None;
    }
    let mut raw = Vec::new();
    if file.take(MAX_SSH_CONFIG_BYTES + 1).read_to_end(&mut raw).is_err() || raw.len() as u64 > MAX_SSH_CONFIG_BYTES {
        return None;
    }
    String::from_utf8(raw).ok()
}

fn tokenize_ssh_line(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if !in_quotes && c == '#' && cur.is_empty() {
            break;
        }
        if c == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if !in_quotes && (c.is_ascii_whitespace() || (tokens.is_empty() && c == '=')) {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            if c != '=' {
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_whitespace() {
                        chars.next();
                    } else if tokens.len() == 1 && next == '=' {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn glob_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return !name.starts_with('.');
    }
    if !pattern.contains('*') {
        return pattern == name;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if !parts[0].is_empty() && !name.starts_with(parts[0]) {
        return false;
    }
    if let Some(&last) = parts.last() {
        if !last.is_empty() && !name.ends_with(last) {
            return false;
        }
    }
    let mut rest = name;
    for (idx, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if idx == 0 {
            rest = &rest[part.len()..];
        } else if let Some(pos) = rest.find(part) {
            rest = &rest[pos + part.len()..];
        } else {
            return false;
        }
    }
    !(pattern.starts_with('*') && name.starts_with('.'))
}

fn expand_include_pattern(pattern: &str, ssh_dir: &Path, home_dir: Option<&Path>) -> Vec<PathBuf> {
    let resolved = if let Some(stripped) = pattern.strip_prefix("~/") {
        match home_dir {
            Some(h) => h.join(stripped),
            None => return Vec::new(),
        }
    } else if Path::new(pattern).is_absolute() {
        PathBuf::from(pattern)
    } else {
        ssh_dir.join(pattern)
    };
    let Some(file_pat) = resolved.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    if !file_pat.contains('*') {
        return vec![resolved];
    }
    let Some(parent) = resolved.parent() else {
        return Vec::new();
    };
    let Ok(iter) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut matched: Vec<PathBuf> = iter
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            glob_match(file_pat, &name).then(|| e.path())
        })
        .collect();
    matched.sort();
    matched
}

struct ParserState {
    hosts: Vec<SshHost>,
    by_name: std::collections::HashMap<String, usize>,
    active: Vec<usize>,
    in_wildcard_default: bool,
    default_user: Option<String>,
    default_port: Option<u16>,
    visited: HashSet<PathBuf>,
    files_read: usize,
    ssh_dir: PathBuf,
    home_dir: Option<PathBuf>,
}

impl ParserState {
    fn parse_file(&mut self, path: &Path, depth: usize) {
        if depth > MAX_INCLUDE_DEPTH || self.files_read >= MAX_INCLUDE_FILES {
            return;
        }
        let Some(canon) = std::fs::canonicalize(path).ok() else {
            return;
        };
        if !self.visited.insert(canon) {
            return;
        }
        let Some(text) = read_regular_config(path) else {
            return;
        };
        self.files_read += 1;

        for raw_line in text.lines() {
            let tokens = tokenize_ssh_line(raw_line.trim());
            if tokens.len() < 2 {
                continue;
            }
            let key = tokens[0].to_ascii_lowercase();
            match key.as_str() {
                "include" => {
                    for pat in &tokens[1..] {
                        for inc_path in expand_include_pattern(pat, &self.ssh_dir, self.home_dir.as_deref()) {
                            self.parse_file(&inc_path, depth + 1);
                        }
                    }
                }
                "match" => {
                    self.active.clear();
                    self.in_wildcard_default = false;
                }
                "host" => {
                    self.active.clear();
                    self.in_wildcard_default = tokens.len() == 2 && tokens[1] == "*";
                    for pat in &tokens[1..] {
                        if pat.contains('*') || pat.contains('?') || pat.starts_with('!') || !valid_ssh_host(pat) {
                            continue;
                        }
                        let idx = if let Some(&existing) = self.by_name.get(pat.as_str()) {
                            existing
                        } else {
                            if self.hosts.len() >= MAX_SSH_HOSTS {
                                continue;
                            }
                            let next_idx = self.hosts.len();
                            self.hosts.push(SshHost { host: pat.clone(), hostname: None, user: None, port: None });
                            self.by_name.insert(pat.clone(), next_idx);
                            next_idx
                        };
                        if !self.active.contains(&idx) {
                            self.active.push(idx);
                        }
                    }
                }
                "hostname" => {
                    let val = &tokens[1];
                    if val.is_empty() || val.len() > 255 || val.contains('%') || val.chars().any(crate::folders::refused_char) {
                        continue;
                    }
                    for &idx in &self.active {
                        if self.hosts[idx].hostname.is_none() {
                            self.hosts[idx].hostname = Some(val.clone());
                        }
                    }
                }
                "user" => {
                    let val = &tokens[1];
                    if val.is_empty() || val.len() > 128 || val.chars().any(crate::folders::refused_char) {
                        continue;
                    }
                    if self.in_wildcard_default {
                        if self.default_user.is_none() {
                            self.default_user = Some(val.clone());
                        }
                    } else {
                        for &idx in &self.active {
                            if self.hosts[idx].user.is_none() {
                                self.hosts[idx].user = Some(val.clone());
                            }
                        }
                    }
                }
                "port" => {
                    let Ok(port) = tokens[1].parse::<u16>() else { continue };
                    if port == 0 {
                        continue;
                    }
                    if self.in_wildcard_default {
                        if self.default_port.is_none() {
                            self.default_port = Some(port);
                        }
                    } else {
                        for &idx in &self.active {
                            if self.hosts[idx].port.is_none() {
                                self.hosts[idx].port = Some(port);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn discover_hosts(shared: &Shared) -> Vec<SshHost> {
    let home_dir = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
    let config_path = match &shared.cfg.ssh_config {
        Some(p) => p.clone(),
        None => match &home_dir {
            Some(h) => h.join(".ssh").join("config"),
            None => return Vec::new(),
        },
    };
    let ssh_dir = config_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let mut state = ParserState {
        hosts: Vec::new(),
        by_name: std::collections::HashMap::new(),
        active: Vec::new(),
        in_wildcard_default: false,
        default_user: None,
        default_port: None,
        visited: HashSet::new(),
        files_read: 0,
        ssh_dir,
        home_dir,
    };
    state.parse_file(&config_path, 0);
    for h in &mut state.hosts {
        if h.user.is_none() {
            h.user = state.default_user.clone();
        }
        if h.port.is_none() {
            h.port = state.default_port;
        }
    }
    state.hosts
}

pub fn ssh_hosts(shared: &Shared) -> Reply {
    let hosts: Vec<Value> = discover_hosts(shared)
        .into_iter()
        .map(|h| {
            json!({
                "host": h.host,
                "hostname": h.hostname,
                "user": h.user,
                "port": h.port,
            })
        })
        .collect();
    Ok(obj(json!({"hosts": hosts})))
}

fn clean_error_line(raw: &str) -> String {
    let first = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("ssh connection failed");
    first
        .chars()
        .filter(|c| !crate::folders::refused_char(*c))
        .take(200)
        .collect()
}

pub fn ssh_check(shared: &Shared, host: &str) -> Reply {
    ssh_check_within(shared, host, Duration::from_secs(SSH_PROBE_TIMEOUT_SECS))
}

pub(crate) fn ssh_check_within(shared: &Shared, host: &str, timeout: Duration) -> Reply {
    let connect_secs = timeout.as_secs().clamp(1, 5);
    let mut cmd = ssh_base_command(shared, host, connect_secs)?;
    let probe = format!(
        "sh -lc '{REMOTE_PATH_EXPORT} printf \"LICO_HOME=%s\\n\" \"$HOME\"; command -v claude >/dev/null 2>&1 && echo LICO_PROV=claude; command -v codex >/dev/null 2>&1 && echo LICO_PROV=codex; true'"
    );
    cmd.arg(probe)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(obj(json!({
                "host": host,
                "reachable": false,
                "providers": [],
                "home": Value::Null,
                "error": clean_error_line(&format!("cannot execute ssh: {e}")),
            })));
        }
    };

    let out_pipe = child.stdout.take();
    let err_pipe = child.stderr.take();
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe {
            let _ = p.take(16 * 1024).read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe {
            let _ = p.take(16 * 1024).read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
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

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();

    let Some(exit) = status else {
        return Ok(obj(json!({
            "host": host,
            "reachable": false,
            "providers": [],
            "home": Value::Null,
            "error": "timed out",
        })));
    };

    if !exit.success() {
        let msg = if !stderr.trim().is_empty() {
            clean_error_line(&stderr)
        } else {
            format!("ssh exited with status {}", exit.code().unwrap_or(-1))
        };
        return Ok(obj(json!({
            "host": host,
            "reachable": false,
            "providers": [],
            "home": Value::Null,
            "error": msg,
        })));
    }

    let mut home: Option<String> = None;
    let mut providers: Vec<&'static str> = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim_end_matches('\r');
        if let Some(h) = trimmed.strip_prefix("LICO_HOME=") {
            if !h.is_empty() && h.starts_with('/') && crate::folders::valid_path_shape(h) {
                home = Some(h.to_string());
            }
        } else if trimmed == "LICO_PROV=claude" && !providers.contains(&"claude") {
            providers.push("claude");
        } else if trimmed == "LICO_PROV=codex" && !providers.contains(&"codex") {
            providers.push("codex");
        }
    }

    Ok(obj(json!({
        "host": host,
        "reachable": true,
        "providers": providers,
        "home": home,
        "error": Value::Null,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn dummy_shared_with_ssh(root: &Path, ssh_bin: PathBuf, ssh_config: Option<PathBuf>) -> Shared {
        for sub in ["log", "accounts", "cred", "work", "handoff", "registry"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        let cfg = crate::core::Config {
            login_bin: None,
            login_args: Vec::new(),
            login_timeout: 5,
            log_dir: root.join("log"),
            agent: PathBuf::from("/bin/true"),
            retain: 64,
            window: 64,
            queue_bytes: 65536,
            payload: 64,
            max_sessions: 8,
            codex: PathBuf::from("/bin/true"),
            codex_args: Vec::new(),
            claude: PathBuf::from("/bin/true"),
            claude_args: Vec::new(),
            cred_bin: PathBuf::from("/bin/true"),
            cred_args: Vec::new(),
            cred_dir: root.join("cred"),
            job_bin: PathBuf::from("/bin/true"),
            workspace_root: root.join("work"),
            handoff_dir: root.join("handoff"),
            accounts_dir: root.join("accounts"),
            registry_dir: root.join("registry"),
            ssh_bin,
            ssh_config,
            text: true,
            realistic_text: false,
        };
        let accounts = crate::accounts::Accounts::load(&cfg.accounts_dir).unwrap();
        Shared {
            cfg,
            sessions: std::sync::RwLock::new(Vec::new()),
            views: std::sync::Mutex::new(std::collections::HashMap::new()),
            started: Instant::now(),
            pending_creates: std::sync::atomic::AtomicUsize::new(0),
            accounts: std::sync::Mutex::new(accounts),
            logins: std::sync::Mutex::new(std::collections::HashMap::new()),
            clip: Default::default(),
        }
    }

    #[test]
    fn valid_ssh_host_rejects_flags_wildcards_spaces_and_accepts_aliases() {
        for bad in ["", "-oProxyCommand=id", "-p", ".leading", "_leading", "*", "gpu-*", "box?", "!neg", "user@box", "box/path", "two words"] {
            assert!(!valid_ssh_host(bad), "expected invalid: {bad}");
        }
        let too_long = "a".repeat(129);
        assert!(!valid_ssh_host(&too_long));
        for good in ["gpu-box", "build_srv_02", "node1.lab.internal", "a", &"b".repeat(128)] {
            assert!(valid_ssh_host(good), "expected valid: {good}");
        }
    }

    #[test]
    fn ssh_config_parser_handles_includes_wildcards_match_blocks_and_deduplicates() {
        let dir = std::env::temp_dir().join(format!("lico-ssh-cfg-{}", crate::core::random_hex(6)));
        let ssh_dir = dir.join("dot_ssh");
        let conf_d = ssh_dir.join("conf.d");
        std::fs::create_dir_all(&conf_d).unwrap();

        std::fs::write(
            conf_d.join("10_cluster.conf"),
            "# Included cluster hosts\nHost gpu-box train-01\n  HostName 10.20.0.5\n  User researcher\n  Port=2222\n",
        )
        .unwrap();
        let outside = dir.join("outside.conf");
        std::fs::write(&outside, "Host symlinked-host\n  HostName 10.99.99.99\n").unwrap();
        std::os::unix::fs::symlink(&outside, conf_d.join("20_link.conf")).unwrap();

        let main_cfg = ssh_dir.join("config");
        std::fs::write(
            &main_cfg,
            "Include conf.d/*.conf\n\n\
             Host *.internal !bad-node -flaglike\n\
               User ignored\n\n\
             Host build-srv gpu-box\n\
               HostName build.example.invalid\n\n\
             Match host build-srv exec \"true\"\n\
               User match_only_user\n\n\
             Host *\n\
               User fallback_user\n\
               Port 2200\n",
        )
        .unwrap();

        let shared = dummy_shared_with_ssh(&dir.join("state"), PathBuf::from("/bin/false"), Some(main_cfg));
        let found = discover_hosts(&shared);
        assert_eq!(
            found,
            vec![
                SshHost {
                    host: "gpu-box".into(),
                    hostname: Some("10.20.0.5".into()),
                    user: Some("researcher".into()),
                    port: Some(2222),
                },
                SshHost {
                    host: "train-01".into(),
                    hostname: Some("10.20.0.5".into()),
                    user: Some("researcher".into()),
                    port: Some(2222),
                },
                SshHost {
                    host: "build-srv".into(),
                    hostname: Some("build.example.invalid".into()),
                    user: Some("fallback_user".into()),
                    port: Some(2200),
                },
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ssh_check_reports_providers_handles_failures_and_reaps_timeouts() {
        let dir = std::env::temp_dir().join(format!("lico-ssh-chk-{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let mock_ssh = dir.join("mock_ssh.sh");
        std::fs::write(
            &mock_ssh,
            "#!/bin/sh\n\
             host=\"\"\n\
             while [ $# -gt 0 ]; do\n\
               if [ \"$1\" = \"--\" ]; then host=\"$2\"; shift 2; break; fi\n\
               shift\n\
             done\n\
             case \"$host\" in\n\
               gpu-box)\n\
                 printf 'LICO_HOME=/remote/alice\\nLICO_PROV=claude\\nLICO_PROV=codex\\n'\n\
                 exit 0 ;;\n\
               hang-box)\n\
                 sleep 10\n\
                 exit 0 ;;\n\
               *)\n\
                 echo 'ssh: connect to host offline-box port 22: Connection refused' >&2\n\
                 exit 255 ;;\n\
             esac\n",
        )
        .unwrap();
        std::fs::set_permissions(&mock_ssh, std::fs::Permissions::from_mode(0o700)).unwrap();

        let shared = dummy_shared_with_ssh(&dir.join("state"), mock_ssh, None);
        assert_eq!(ssh_check(&shared, "-bad"), Err("bad_request"));

        let ok_res = ssh_check_within(&shared, "gpu-box", Duration::from_secs(2)).unwrap();
        assert_eq!(ok_res["reachable"], json!(true));
        assert_eq!(ok_res["providers"], json!(["claude", "codex"]));
        assert_eq!(ok_res["home"], json!("/remote/alice"));
        assert_eq!(ok_res["error"], Value::Null);

        let down_res = ssh_check_within(&shared, "offline-box", Duration::from_secs(2)).unwrap();
        assert_eq!(down_res["reachable"], json!(false));
        assert_eq!(down_res["providers"], json!([]));
        assert!(down_res["error"].as_str().unwrap().contains("Connection refused"));

        let t0 = Instant::now();
        let hung_res = ssh_check_within(&shared, "hang-box", Duration::from_millis(180)).unwrap();
        assert!(t0.elapsed() < Duration::from_secs(2));
        assert_eq!(hung_res["reachable"], json!(false));
        assert_eq!(hung_res["error"], json!("timed out"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
