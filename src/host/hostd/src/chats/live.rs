//! Is another program writing this chat right now? (`M11-P02`) A chat that a Claude Code or Codex process in another terminal is writing must not be resumed from Lico: a
//! running external process is never adopted and nothing is injected into it (SES-07). The provider programs hold no transcript file open, so the signals are two, and
//! either one marks the chat live (the cautious side):
//! - `recent_write`: the transcript was modified within the last 180 seconds (`LICO_LIVE_WRITE_SECS` changes it, for the tests);
//! - a same-user `claude` or `codex` process that is not one of this host's own agents (the parent chain does not reach this host) either names the session id on its
//!   command line (`process_has_session`), or runs in the chat's folder and started no later than the chat's last write (`process_in_folder`: a closed chat older than
//!   the process is free). An unreadable `cwd` or command line is unknown: it never makes a chat live and never frees one, and the write signal still applies.
//!
//! A chat that is open in a tab of Lico is Lico's own and is never "elsewhere"; a remote chat is not looked at. The process list is read from `/proc` (comm first, the rest
//! only for the two program names) and kept for two seconds, and the check is made only for the items of a returned page and for the chat a creation would continue.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::{norm_path, ChatItem};

pub(super) const RECENT_WRITE: &str = "recent_write";
pub(super) const PROCESS_HAS_SESSION: &str = "process_has_session";
pub(super) const PROCESS_IN_FOLDER: &str = "process_in_folder";

extern "C" {
    fn geteuid() -> u32;
    fn sysconf(name: i32) -> i64;
}

fn write_window_secs() -> i64 {
    static SECS: OnceLock<i64> = OnceLock::new();
    *SECS.get_or_init(|| std::env::var("LICO_LIVE_WRITE_SECS").ok().and_then(|v| v.parse().ok()).filter(|n| *n >= 0).unwrap_or(180))
}

/// One provider process as `/proc` shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Proc {
    pub provider: String,
    pub cwd: Option<String>,
    pub args: Vec<String>,
    pub start: Option<i64>, // unix seconds
}

fn read_small(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path).ok()?.take(8192).read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn boot_time() -> Option<i64> {
    let text = std::fs::read_to_string("/proc/stat").ok()?;
    text.lines().find_map(|l| l.strip_prefix("btime ")).and_then(|v| v.trim().parse().ok())
}

/// `(parent, start time in clock ticks since boot)` from `/proc/<pid>/stat` (the name in parentheses may hold spaces: the fields count from the last `)`).
fn stat_of(pid: u32) -> Option<(u32, u64)> {
    let text = String::from_utf8_lossy(&read_small(Path::new(&format!("/proc/{pid}/stat")))?).into_owned();
    let rest = &text[text.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    Some((fields.get(1)?.parse().ok()?, fields.get(19)?.parse().ok()?))
}

/// Whether the parent chain of `pid` reaches `host`: such a process is one of this host's own agents.
fn is_ours(pid: u32, host: u32) -> bool {
    let mut at = pid;
    for _ in 0..64 {
        if at == host {
            return true;
        }
        match stat_of(at) {
            Some((parent, _)) if parent > 1 && parent != at => at = parent,
            _ => return false,
        }
    }
    false
}

/// The same-user `claude` and `codex` processes that are not this host's own.
fn read_procs() -> Vec<Proc> {
    let (host, me) = (std::process::id(), unsafe { geteuid() });
    let (tick, boot) = (unsafe { sysconf(2) }.max(1), boot_time());
    let mut found = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else { return found };
    for entry in dir.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let Some(comm) = read_small(&entry.path().join("comm")) else { continue };
        let provider = String::from_utf8_lossy(&comm).trim().to_string();
        if provider != "claude" && provider != "codex" {
            continue;
        }
        if std::os::unix::fs::MetadataExt::uid(&match entry.metadata() { Ok(m) => m, Err(_) => continue }) != me || is_ours(pid, host) {
            continue;
        }
        let cwd = std::fs::read_link(entry.path().join("cwd")).ok().map(|p| p.to_string_lossy().into_owned());
        let args = read_small(&entry.path().join("cmdline")).map(|b| b.split(|c| *c == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect()).unwrap_or_default();
        let start = stat_of(pid).zip(boot).map(|((_, ticks), boot)| boot + (ticks as i64) / tick);
        found.push(Proc { provider, cwd, args, start });
    }
    found
}

fn procs() -> Arc<Vec<Proc>> {
    static CACHE: Mutex<Option<(Instant, Arc<Vec<Proc>>)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    if let Some((at, list)) = cache.as_ref() {
        if at.elapsed().as_secs() < 2 {
            return list.clone();
        }
    }
    let list = Arc::new(read_procs());
    *cache = Some((Instant::now(), list.clone()));
    list
}

fn names(proc: &Proc, session: &str) -> bool {
    proc.args.iter().any(|a| a == session || a.strip_suffix(session).is_some_and(|head| head.ends_with('=')))
}

/// Why `item` is thought to be written by another program, if it is (see the top of this file).
pub(super) fn judge(item: &ChatItem, now: i64, procs: &[Proc]) -> Option<&'static str> {
    if item.open_session.is_some() || item.ssh_host.is_some() || !item.recorded {
        return None;
    }
    let written = item.updated_at as i64;
    if now - written <= write_window_secs() {
        return Some(RECENT_WRITE);
    }
    let mine = || procs.iter().filter(|p| p.provider == item.provider);
    if mine().any(|p| names(p, &item.provider_session)) {
        return Some(PROCESS_HAS_SESSION);
    }
    let folder = item.cwd.as_deref().map(norm_path)?;
    mine().any(|p| p.cwd.as_deref().map(norm_path) == Some(folder) && p.start.map_or(true, |s| written + 2 >= s)).then_some(PROCESS_IN_FOLDER)
}

/// Fill `live` of the items (the process list is read once for all of them, and only when a chat is not already marked by its write time).
pub(super) fn mark(items: &mut [ChatItem]) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let mut list: Option<Arc<Vec<Proc>>> = None;
    for item in items.iter_mut() {
        let fresh = item.recorded && item.open_session.is_none() && now - item.updated_at as i64 <= write_window_secs();
        let seen = if fresh { Arc::new(Vec::new()) } else { list.get_or_insert_with(procs).clone() };
        item.live = judge(item, now, &seen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(session: &str, folder: &str, written: i64) -> ChatItem {
        ChatItem {
            provider: "claude".into(),
            provider_session: session.into(),
            account: Some("a1".into()),
            account_label: None,
            ssh_host: None,
            cwd: Some(folder.into()),
            global: false,
            title: session.into(),
            updated_at: written as u64,
            open_session: None,
            open_state: None,
            file: None,
            turns: None,
            recorded: true,
            live: None,
        }
    }

    fn proc(cwd: Option<&str>, args: &[&str], start: Option<i64>) -> Proc {
        Proc { provider: "claude".into(), cwd: cwd.map(str::to_string), args: args.iter().map(|a| a.to_string()).collect(), start }
    }

    #[test]
    fn a_chat_is_live_by_its_write_time_by_a_process_that_names_it_or_by_a_process_in_its_folder_since_before_its_last_write() {
        let now = 1_000_000;
        assert_eq!(judge(&item("c1", "/w", now - 5), now, &[]), Some(RECENT_WRITE));
        assert_eq!(judge(&item("c1", "/w", now - 3600), now, &[]), None);
        let by_id = [proc(Some("/elsewhere"), &["claude", "--resume", "c1"], Some(now - 100))];
        assert_eq!(judge(&item("c1", "/w", now - 3600), now, &by_id), Some(PROCESS_HAS_SESSION));
        assert_eq!(judge(&item("c2", "/w", now - 3600), now, &by_id), None, "it names another chat");
        let joined = [proc(Some("/elsewhere"), &["claude", "--resume=c1"], None)];
        assert_eq!(judge(&item("c1", "/w", now - 3600), now, &joined), Some(PROCESS_HAS_SESSION));
        let in_folder = [proc(Some("/w/"), &["claude"], Some(now - 600))];
        assert_eq!(judge(&item("c1", "/w", now - 300), now, &in_folder), Some(PROCESS_IN_FOLDER), "written after the process started");
        assert_eq!(judge(&item("c1", "/w", now - 3600), now, &in_folder), None, "a closed chat older than the process is free");
        assert_eq!(judge(&item("c1", "/other", now - 300), now, &in_folder), None, "another folder");
    }

    #[test]
    fn what_cannot_be_read_never_frees_a_chat_and_a_chat_of_ours_or_a_remote_one_is_never_judged() {
        let now = 1_000_000;
        let unknown_start = [proc(Some("/w"), &["claude"], None)];
        assert_eq!(judge(&item("c1", "/w", now - 3600), now, &unknown_start), Some(PROCESS_IN_FOLDER), "start unknown: the cautious side");
        let unreadable_cwd = [proc(None, &["claude"], Some(now - 600))];
        assert_eq!(judge(&item("c1", "/w", now - 300), now, &unreadable_cwd), None);
        assert_eq!(judge(&item("c1", "/w", now - 5), now, &unreadable_cwd), Some(RECENT_WRITE), "the write signal still applies");
        let mut ours = item("c1", "/w", now - 5);
        ours.open_session = Some("s1".into());
        assert_eq!(judge(&ours, now, &[]), None);
        let mut remote = item("c1", "/w", now - 5);
        remote.ssh_host = Some("gpu".into());
        assert_eq!(judge(&remote, now, &[]), None);
        let codex = [Proc { provider: "codex".into(), ..proc(Some("/w"), &["codex"], None) }];
        assert_eq!(judge(&item("c1", "/w", now - 3600), now, &codex), None, "a process of the other provider says nothing");
    }

    #[test]
    fn the_stat_line_is_read_from_the_last_parenthesis() {
        let (parent, _) = stat_of(std::process::id()).expect("own stat");
        assert!(parent >= 1);
    }
}
