//! `lico xjob <job> [args...]`: the job side of the helper process (plan M02-P04, S03; XF-07).
//!
//! Every job that needs the X server (a capture now, the clipboard in S04) runs in a process of its own: the one program called with the
//! hidden first argument `xjob`, which does the job, writes ONE JSON line on standard output and ends. The long-lived host holds no X
//! connection; `helper::run_job` is the host side that starts such a process and reads the line. The name is not in any usage text or help.
//! A job needs no host socket and reads nothing from standard input.
//!
//! The jobs are one table, `JOBS`: a new job is one more line. A job takes the words after its name and never returns (it prints its line and
//! exits: 0 on success, 1 for a usage error, 2 for any other failure, as `lico capture` does).

use serde_json::json;

/// A job: the words after its name; prints the one answer line and exits.
pub type Job = fn(&[String]) -> !;

/// The table of jobs (S04b adds `clipboard_write` here).
pub const JOBS: &[(&str, Job)] = &[
    ("capture", crate::capture::command),
    ("clipboard_read", crate::clipboard::job::read_command),
    ("computer", crate::computer::job_command),
];

/// The job of that name.
pub fn lookup(name: &str) -> Option<Job> {
    JOBS.iter().find(|(n, _)| *n == name).map(|(_, job)| *job)
}

/// The words after `xjob`.
pub fn run(args: &[String]) -> ! {
    match args.first().map(String::as_str) {
        None => fail("no job given"),
        Some(name) => match lookup(name) {
            Some(job) => job(&args[1..]),
            None => fail(&format!("unknown job {name}")),
        },
    }
}

fn fail(detail: &str) -> ! {
    println!("{}", json!({"ok": 0, "error": "usage", "detail": detail}));
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_knows_capture_and_the_clipboard_read_and_nothing_else() {
        assert!(lookup("capture").is_some());
        assert!(lookup("clipboard_read").is_some());
        assert!(lookup("computer").is_some());
        for name in ["", "Capture", "capture ", "Clipboard_read", "clipboard_read ", "clipboard", "clipboard_write", "Computer", "xjob", "--screen"] {
            assert!(lookup(name).is_none(), "{name:?}");
        }
        let mut names: Vec<&str> = JOBS.iter().map(|(n, _)| *n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), JOBS.len(), "a job name is listed once");
    }

    #[test]
    fn the_hidden_name_is_in_no_usage_text() {
        let cli = include_str!("cli.rs");
        let usage = cli.split("const USAGE: &str = ").nth(1).and_then(|t| t.split("\";\n").next()).expect("the usage text");
        assert!(usage.contains("capture"), "the slice found the right text");
        assert!(!usage.contains("xjob"));
    }
}
