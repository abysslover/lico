//! What runs in the foreground of a terminal (M23-P04a): the one question the host asks about a program a person started in a terminal, answered with one of three words, `claude`,
//! `codex` or `agy`, or with nothing. Read-only: the foreground process group of the pty (`pty::Master::foreground_group`), then the command line of the group's leader from `/proc`
//! (the same user's, bounded). Nothing is sent to the program and nothing of its screen or its conversation is read.

use std::io::Read;

/// The programs that are named. Nothing else is ever answered.
pub const AGENTS: [&str; 3] = ["claude", "codex", "agy"];

/// How much of a command line is read: a name that is not within it is not seen (a program's own name and its first arguments are at the front).
const MOST: usize = 4096;

/// The programs that run a script given as an argument (`node /usr/bin/codex`); for these the script's name is the program's.
fn runs_a_script(name: &str) -> bool {
    matches!(name, "node" | "nodejs" | "bun" | "deno" | "sh" | "bash" | "dash" | "env") || name.starts_with("python")
}

/// The flags after which the next argument is code, not a script (`bash -c agy`, `node -e claude`).
fn takes_code(flag: &str) -> bool {
    matches!(flag, "-c" | "-e" | "-p" | "--eval" | "--print")
}

/// The last component of a path, without the ending of a script.
fn leaf(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or("");
    [".js", ".mjs", ".cjs"].iter().find_map(|e| name.strip_suffix(e)).unwrap_or(name)
}

fn named(name: &str) -> Option<&'static str> {
    AGENTS.iter().copied().find(|a| *a == name)
}

/// The agent that `cmdline` (a command line as `/proc/<pid>/cmdline` has it: the arguments, each followed by a NUL) starts, if it is one of `AGENTS`: the last component of the
/// program's own name, or, when that is an interpreter, of the first argument that is not a flag (after `env`, not a `NAME=value` either).
pub fn of_cmdline(cmdline: &[u8]) -> Option<&'static str> {
    let text = String::from_utf8_lossy(&cmdline[..cmdline.len().min(MOST)]);
    let mut args = text.split('\0');
    let program = leaf(args.next().unwrap_or(""));
    if let Some(agent) = named(program) {
        return Some(agent);
    }
    if !runs_a_script(program) {
        return None;
    }
    for arg in args {
        if takes_code(arg) {
            return None;
        }
        if arg.starts_with('-') || (program == "env" && arg.contains('=')) {
            continue;
        }
        return named(leaf(arg));
    }
    None
}

/// The agent in the foreground process group `group` (the pid of its leader), if any.
pub fn of_group(group: i32) -> Option<&'static str> {
    if group <= 1 {
        return None;
    }
    let mut buf = Vec::new();
    std::fs::File::open(format!("/proc/{group}/cmdline")).ok()?.take(MOST as u64).read_to_end(&mut buf).ok()?;
    of_cmdline(&buf)
}
