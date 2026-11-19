//! What runs in the foreground of a terminal (M23-P04a): the name of a program from its command line, and the `agent` word of `terminal.list` and `terminal.watch`.

use super::*;

// ---------------------------------------------------------------------------------------------------------------------------------- what runs in the foreground (M23-P04a)

/// A command line as `/proc/<pid>/cmdline` has it: each argument followed by a NUL.
fn cmd(args: &[&str]) -> Vec<u8> {
    args.iter().flat_map(|a| a.bytes().chain(std::iter::once(0))).collect()
}

#[test]
fn a_program_is_named_by_the_last_part_of_its_own_name_or_of_the_script_an_interpreter_runs() {
    use crate::terminal::agent::of_cmdline as of;
    // started by name or by path, with arguments (the three are native programs on the owner's machine)
    assert_eq!(of(&cmd(&["claude"])), Some("claude"));
    assert_eq!(of(&cmd(&["/opt/u/.local/bin/claude", "--model", "x"])), Some("claude"));
    assert_eq!(of(&cmd(&["/opt/codex/bin/codex", "resume"])), Some("codex"));
    assert_eq!(of(&cmd(&["agy", "--conversation", "8855101e-c35b-4966-91d7-21139f40b3c3"])), Some("agy"));
    // a script that an interpreter runs: the first argument that is not a flag
    assert_eq!(of(&cmd(&["node", "/usr/bin/codex", "--help"])), Some("codex"));
    assert_eq!(of(&cmd(&["/usr/bin/node", "--no-warnings", "/usr/lib/node_modules/x/claude"])), Some("claude"));
    assert_eq!(of(&cmd(&["/usr/bin/env", "FOO=1", "agy"])), Some("agy"));
    assert_eq!(of(&cmd(&["python3.12", "-u", "/x/agy"])), Some("agy"));
    assert_eq!(of(&cmd(&["/bin/sh", "/tmp/rig/codex"])), Some("codex"));
    // the ending of a script is not part of its name; a title the program wrote over its command line may have no ending NUL
    assert_eq!(of(&cmd(&["node", "/x/codex.js"])), Some("codex"));
    assert_eq!(of(b"claude"), Some("claude"));
}

#[test]
fn a_name_that_is_not_exact_or_a_program_that_only_mentions_an_agent_is_no_agent() {
    use crate::terminal::agent::of_cmdline as of;
    assert_eq!(of(&cmd(&["vim", "claude"])), None, "an editor that opens a file of that name");
    assert_eq!(of(&cmd(&["cat", "/x/agy"])), None);
    assert_eq!(of(&cmd(&["node", "claude.txt"])), None);
    assert_eq!(of(&cmd(&["claudette"])), None);
    assert_eq!(of(&cmd(&["/opt/claude/bin/vim"])), None, "a folder is not the program");
    assert_eq!(of(&cmd(&["bash"])), None);
    assert_eq!(of(&cmd(&["-bash"])), None, "a login shell");
    assert_eq!(of(&cmd(&["bash", "-c", "agy"])), None, "the text after -c is code, not a script");
    assert_eq!(of(&cmd(&["node", "-e", "claude"])), None);
    assert_eq!(of(&cmd(&["env", "FOO=claude"])), None, "a value is not a program");
    assert_eq!(of(&cmd(&["node"])), None, "an interpreter with no script");
    assert_eq!(of(&cmd(&["node", "other.js", "agy"])), None, "only the script counts, not what it is given");
    assert_eq!(of(b""), None);
    assert_eq!(of(b"\0\0"), None, "a program that blanked its command line");
    // only the beginning of a command line is read: flags that run on past the bound hide nothing and show nothing
    let mut long = vec!["env"];
    long.extend(std::iter::repeat_n("-x", 3000));
    long.push("agy");
    assert_eq!(of(&cmd(&long)), None);
}

#[test]
fn the_agent_of_a_process_is_read_from_its_own_command_line_and_a_process_that_is_not_there_is_none() {
    let rig = Rig::new();
    let agy = rig.dir.join("agy");
    std::fs::copy(std::env::var("PATH").unwrap().split(':').map(|d| PathBuf::from(d).join("sleep")).find(|p| p.is_file()).unwrap(), &agy).unwrap();
    std::fs::set_permissions(&agy, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = std::process::Command::new(&agy).arg("20").spawn().unwrap();
    let pid = child.id() as i32;
    // the command line of a process that has only just been started may still read empty (the arguments are placed after the spawn returns)
    eventually("the command line is there", Duration::from_secs(5), || crate::terminal::agent::of_group(pid) == Some("agy"));
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(crate::terminal::agent::of_group(pid), None, "a process that has ended");
    for odd in [0, -1, i32::MAX] {
        assert_eq!(crate::terminal::agent::of_group(odd), None, "{odd}");
    }
    assert_eq!(crate::terminal::agent::of_group(std::process::id() as i32), None, "the test program itself");
}

/// What the three words of the terminal's answers are while `tab` runs: the list's row and a watch that is answered at once.
fn agent_of(rig: &Rig, tab: &str) -> (Value, Value) {
    let list = rig.call(json!({"op": "terminal.list"})).unwrap();
    let row = list["terminals"].as_array().unwrap().iter().find(|r| r["tab"] == json!(tab)).unwrap().clone();
    let watch = rig.call(json!({"op": "terminal.watch", "tab": tab, "seq": 0})).unwrap();
    assert!(row.get("agent").is_some() && watch.contains_key("agent"), "both answers always say `agent`, a null when there is none: {row} {watch:?}");
    (row["agent"].clone(), watch["agent"].clone())
}

#[test]
fn the_list_and_the_watch_name_the_agent_in_the_foreground_and_say_null_when_it_ends_or_the_program_is_another() {
    let rig = Rig::with_watchdog(40);
    // the three names run one after another in the foreground of a shell with job control (a group of their own, as an interactive shell runs a command): copies of sleep named
    // claude and agy, and a script named codex (an interpreter's argument); a Ctrl+C ends each (the script's own trap keeps the shell, which a dash would otherwise end with the program), and a plain sleep comes last
    let tab = rig.open(
        "set -m; trap : INT; for n in claude agy; do cp \"$(command -v sleep)\" \"$HOME/$n\"; done; printf '#!/bin/sh\\nsleep 30\\n' > \"$HOME/codex\"; chmod +x \"$HOME/codex\"; printf 'ready\\n'; \
         \"$HOME/claude\" 30; printf 'one\\n'; \"$HOME/codex\"; printf 'two\\n'; \"$HOME/agy\" 30; printf 'three\\n'; sleep 30",
    );
    rig.shows(&tab, "ready");
    for (name, over) in [("claude", "one"), ("codex", "two"), ("agy", "three")] {
        eventually(&format!("both answers say {name}"), Duration::from_secs(5), || agent_of(&rig, &tab) == (json!(name), json!(name)));
        rig.type_in(&tab, "\x03");
        rig.shows(&tab, over);
    }
    // a program that is another one is no agent, and neither is the shell between two programs
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(agent_of(&rig, &tab), (Value::Null, Value::Null));
}

#[test]
fn a_shell_that_ended_has_no_agent() {
    let rig = Rig::new();
    let tab = rig.open("exit 0");
    rig.until(&tab, "the end", |w| w["running"] == json!(false));
    assert_eq!(agent_of(&rig, &tab), (Value::Null, Value::Null));
}

#[test]
fn a_shell_that_ended_has_no_agent_even_while_an_agent_it_started_still_holds_the_pty() {
    let rig = Rig::with_watchdog(40);
    // a copy of sleep named agy, started in the background of a shell that then ends: the program stays (it ignores the hang-up the system sends its group, and so stays) and holds the pty
    let tab = rig.open("trap '' HUP; cp \"$(command -v sleep)\" \"$HOME/agy\"; \"$HOME/agy\" 30 & printf 'started\\n'; exit 0");
    rig.until(&tab, "the shell's end after the program started", |w| w["running"] == json!(false) && screen(w).contains("started"));
    let agy = rig.dir.join("agy").to_string_lossy().into_owned();
    let alive_program = || {
        std::fs::read_dir("/proc").unwrap().filter_map(|e| std::fs::read(e.ok()?.path().join("cmdline")).ok()).any(|c| String::from_utf8_lossy(&c).starts_with(&agy))
    };
    eventually("the program is still running", Duration::from_secs(5), alive_program);
    assert_eq!(agent_of(&rig, &tab), (Value::Null, Value::Null), "the pty has no foreground group once the shell has ended");
}
