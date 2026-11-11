//! The host side of the helper process against a FAKE job program (a small `sh` script in a private folder), and the real thing: the plan's
//! property (after jobs the caller has no child, no more threads, no more open files and the same memory) is read from `/proc` in a process of
//! its own, because the other tests of this binary start and end processes beside it.
//!
//! The in-process tests below share the test process, so they look at the one pid they started (`/proc/<pid>`), never at the whole table.

use super::*;
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

/// The fake `lico`: `$1` is `xjob`, `$2` says what to do, `$3` is a file the test names (pids go there).
const SCRIPT: &str = r#"#!/bin/sh
[ "$1" = xjob ] || { echo "not xjob" >&2; exit 9; }
case "$2" in
ok) echo '{"ok":1,"v":7}' ;;
ok_noeol) printf '{"ok":1,"v":8}' ;;
fail2) echo '{"ok":0,"error":"no_display","detail":"cannot open the display"}'; exit 2 ;;
fail1) echo '{"ok":0,"error":"usage","detail":"bad words"}'; exit 1 ;;
fail1_nodetail) echo '{"ok":0,"error":"usage"}'; exit 1 ;;
fail3) echo '{"ok":0,"error":"odd","detail":"d"}'; exit 3 ;;
exit0_notok) echo '{"ok":0,"error":"x","detail":"d"}' ;;
exit2_ok1) echo '{"ok":1}'; exit 2 ;;
two) echo '{"ok":1}'; echo '{"ok":1}' ;;
none) exit 0 ;;
none_exit2) echo 'it broke' >&2; exit 2 ;;
text) echo hello ;;
broken_json) echo '{"ok":1' ;;
bigok) printf '{"ok":1,"pad":"'; head -c 300000 /dev/zero | tr '\0' a; printf '"}\n' ;;
big) head -c 200000 /dev/zero | tr '\0' a ;;
endless) exec yes aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ;;
hang) echo $$ > "$3"; sleep 60 & echo $! > "$3.gc"; wait ;;
hang_quiet) echo $$ > "$3"; exec sleep 60 ;;
sig) echo 'about to die' >&2; kill -9 $$ ;;
stderr_flood) head -c 300000 /dev/zero | tr '\0' e >&2; echo '{"ok":1}' ;;
stderr_fail) head -c 9000 /dev/zero | tr '\0' e >&2; echo 'x' ;;
stdin) echo "{\"ok\":1,\"stdin\":\"$(readlink /proc/$$/fd/0)\"}" ;;
group) echo "{\"ok\":1,\"pid\":$$,\"pgrp\":$(cut -d' ' -f5 /proc/$$/stat)}" ;;
args) shift 2; printf '{"ok":1,"n":%s,"first":"%s"}\n' "$#" "$1" ;;
leave) sleep 60 >/dev/null 2>&1 </dev/null & echo $! > "$3"; echo '{"ok":1}' ;;
*) echo '{"ok":0,"error":"usage","detail":"unknown"}'; exit 1 ;;
esac
"#;

struct Fake {
    dir: PathBuf,
    bin: PathBuf,
}

impl Fake {
    fn new() -> Fake {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let base = std::env::var_os("TMPDIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("lico-helper-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        fs::DirBuilder::new().mode(0o700).create(&dir).expect("private folder");
        let bin = dir.join("lico");
        let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o700).open(&bin).unwrap();
        std::io::Write::write_all(&mut f, SCRIPT.as_bytes()).unwrap();
        drop(f);
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
        Fake { dir, bin }
    }
    fn file(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }
    /// Runs the fake job. A freshly written program can be refused with "Text file busy" when another thread of this test process forks at the
    /// same moment and its child still holds the write descriptor; only that is retried (a missing or refused program is the tests' business).
    fn run(&self, job: &str, args: &[&str], limits: &JobLimits) -> Result<Value, JobError> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        for _ in 0..20 {
            match run_job(&self.bin, job, &args, limits) {
                Err(e) if e.reason == "spawn_failed" && e.detail.contains("busy") => std::thread::sleep(Duration::from_millis(25)),
                other => return other,
            }
        }
        run_job(&self.bin, job, &args, limits)
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn lim(max: usize, ms: u64) -> JobLimits {
    JobLimits { max_output: max, deadline: Duration::from_millis(ms) }
}

fn ok_limits() -> JobLimits {
    lim(1 << 20, 10_000)
}

fn alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// A process is gone when /proc has no entry for it (a zombie still has one). A process that is not our child is reaped by its new parent, so give it a moment.
fn gone_within(pid: u32, ms: u64) -> bool {
    let end = Instant::now() + Duration::from_millis(ms); // a wait for the new parent (init or a subreaper) to reap a killed grandchild
    while Instant::now() < end {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    !alive(pid)
}

fn pid_in(path: &str) -> u32 {
    fs::read_to_string(path).unwrap().trim().parse().unwrap()
}

#[test]
fn an_answer_line_is_returned_as_a_value() {
    let f = Fake::new();
    assert_eq!(f.run("ok", &[], &ok_limits()).unwrap(), json!({"ok": 1, "v": 7}));
    assert_eq!(f.run("ok_noeol", &[], &ok_limits()).unwrap(), json!({"ok": 1, "v": 8}), "a last line without its line end is one line");
}

#[test]
fn a_job_receives_its_arguments_after_its_name() {
    let f = Fake::new();
    // `args` shifts `xjob` and its own name away and counts the rest: the arguments follow, in order, each as one word
    let v = f.run("args", &["--region", "1,2,3,4"], &ok_limits());
    assert!(matches!(&v, Ok(x) if x["n"] == 2 && x["first"] == "--region"), "{v:?}");
}

#[test]
fn the_job_s_own_failure_is_passed_through() {
    let f = Fake::new();
    assert_eq!(f.run("fail2", &[], &ok_limits()), Err(JobError::new("no_display", "cannot open the display")));
    assert_eq!(f.run("fail1", &[], &ok_limits()), Err(JobError::new("usage", "bad words")));
    assert_eq!(f.run("fail1_nodetail", &[], &ok_limits()), Err(JobError::new("usage", "")));
    let e = f.run("fail2", &[], &ok_limits()).unwrap_err();
    assert_eq!(e.to_json(), json!({"ok": 0, "error": "no_display", "detail": "cannot open the display"}));
}

#[test]
fn an_answer_that_is_not_exactly_one_line_is_a_bad_answer() {
    let f = Fake::new();
    for job in ["two", "none", "text", "broken_json", "exit0_notok", "exit2_ok1", "fail3", "none_exit2"] {
        let e = f.run(job, &[], &ok_limits()).unwrap_err();
        assert_eq!(e.reason, "bad_answer", "{job}: {e:?}");
    }
    let e = f.run("none_exit2", &[], &ok_limits()).unwrap_err();
    assert!(e.detail.contains("no answer line") && e.detail.contains("exit status 2") && e.detail.contains("stderr: it broke"), "{e:?}");
    assert!(f.run("two", &[], &ok_limits()).unwrap_err().detail.contains("more than one line"));
    assert!(f.run("text", &[], &ok_limits()).unwrap_err().detail.contains("not JSON"));
}

#[test]
fn an_answer_over_the_limit_is_cut_off_and_the_job_ended() {
    let f = Fake::new();
    let e = f.run("big", &[], &lim(64 * 1024, 10_000)).unwrap_err();
    assert_eq!(e.reason, "output_too_large", "{e:?}");
    assert!(e.detail.contains("65536"), "{e:?}");
    // a job that never stops writing is not waited for: the function returns at once (its process is reaped by then)
    let began = Instant::now();
    let e = f.run("endless", &[], &lim(1000, 10_000)).unwrap_err();
    assert_eq!(e.reason, "output_too_large", "{e:?}");
    assert!(began.elapsed() < Duration::from_secs(2), "{:?}", began.elapsed());
    // the limit counts the bytes of the answer with its line end: `{"ok":1,"v":7}\n` is 15 bytes
    let v = f.run("ok", &[], &lim(15, 5000));
    assert!(v.is_ok(), "{v:?}");
    let v = f.run("ok", &[], &lim(14, 5000));
    assert_eq!(v.unwrap_err().reason, "output_too_large");
    assert_eq!(f.run("none", &[], &lim(0, 5000)).unwrap_err().reason, "bad_answer", "no output at all is not an answer, and not too large");
}

#[test]
fn a_large_answer_is_read_while_the_job_runs() {
    // 300 KB is over the 64 KiB a pipe holds: a caller that waited for the job before reading would deadlock it until the deadline
    let f = Fake::new();
    let began = Instant::now();
    let v = f.run("bigok", &[], &lim(1 << 20, 10_000)).unwrap();
    assert_eq!(v["pad"].as_str().map(str::len), Some(300_000));
    assert!(began.elapsed() < Duration::from_secs(5), "{:?}", began.elapsed());
}

#[test]
fn a_flood_on_standard_error_neither_blocks_the_job_nor_fills_the_detail() {
    let f = Fake::new();
    assert!(f.run("stderr_flood", &[], &ok_limits()).is_ok(), "300 KB on stderr must not block the job");
    let e = f.run("stderr_fail", &[], &ok_limits()).unwrap_err();
    assert_eq!(e.reason, "bad_answer");
    let kept = e.detail.split("stderr: ").nth(1).unwrap().len();
    assert_eq!(kept, STDERR_KEPT, "the detail keeps {STDERR_KEPT} bytes of standard error and no more: {kept}");
}

#[test]
fn a_job_that_hangs_is_ended_by_its_deadline_and_nothing_is_left() {
    let f = Fake::new();
    let pidfile = f.file("hang.pid");
    let began = Instant::now();
    let e = f.run("hang", &[&pidfile], &lim(1 << 20, 400)).unwrap_err();
    let took = began.elapsed();
    assert_eq!(e.reason, "timeout", "{e:?}");
    assert!(e.detail.contains("hang") && e.detail.contains("400 ms"), "the detail names the job and the time: {e:?}");
    assert!(took >= Duration::from_millis(400) && took < Duration::from_millis(1400), "deadline + 1 s at most: {took:?}");
    let (pid, gc) = (pid_in(&pidfile), pid_in(&format!("{pidfile}.gc")));
    assert!(!alive(pid), "the job is gone and reaped (no zombie entry) when the function returns");
    assert!(gone_within(gc, 3000), "the grandchild the job started is gone too (the group kill)");
    // a job that execs (no shell around it) is ended in the same way
    let e = f.run("hang_quiet", &[&pidfile], &lim(1 << 20, 200)).unwrap_err();
    assert_eq!(e.reason, "timeout");
    assert!(!alive(pid_in(&pidfile)));
}

#[test]
fn a_deadline_of_zero_ends_the_job_at_once() {
    // also the reason why `>=` and `>` on the deadline are the same: at the very instant the loop would poll with a timeout of 0 and look again
    let f = Fake::new();
    let pidfile = f.file("zero.pid");
    let began = Instant::now();
    let e = f.run("hang_quiet", &[&pidfile], &lim(1 << 20, 0)).unwrap_err();
    assert_eq!(e.reason, "timeout", "{e:?}");
    assert!(began.elapsed() < Duration::from_secs(1));
    if let Ok(text) = fs::read_to_string(&pidfile) {
        assert!(!alive(text.trim().parse().unwrap()));
    }
}

#[test]
fn a_job_that_leaves_something_running_does_not_leave_it_running() {
    // the job answers, closes its pipes and ends, but started `sleep` with its output redirected: the group goes with it
    let f = Fake::new();
    let pidfile = f.file("leave.pid");
    let v = f.run("leave", &[&pidfile], &ok_limits());
    assert!(v.is_ok(), "{v:?}");
    assert!(gone_within(pid_in(&pidfile), 3000));
}

#[test]
fn a_job_ended_by_a_signal_says_which() {
    let f = Fake::new();
    let e = f.run("sig", &[], &ok_limits()).unwrap_err();
    assert_eq!(e.reason, "killed", "{e:?}");
    assert!(e.detail.contains("signal 9") && e.detail.contains("about to die"), "{e:?}");
}

#[test]
fn a_program_that_cannot_start_is_spawn_failed() {
    let f = Fake::new();
    let e = run_job(&f.dir.join("missing"), "ok", &[], &ok_limits()).unwrap_err();
    assert_eq!(e.reason, "spawn_failed", "{e:?}");
    fs::write(f.dir.join("plain"), "#!/bin/sh\n").unwrap(); // mode 0644: not executable
    let e = run_job(&f.dir.join("plain"), "ok", &[], &ok_limits()).unwrap_err();
    assert_eq!(e.reason, "spawn_failed", "{e:?}");
    let e = run_job(&f.dir, "ok", &[], &ok_limits()).unwrap_err(); // a folder
    assert_eq!(e.reason, "spawn_failed", "{e:?}");
}

#[test]
fn a_job_has_no_standard_input_and_a_process_group_of_its_own() {
    let f = Fake::new();
    let v = f.run("stdin", &[], &ok_limits()).unwrap();
    assert_eq!(v["stdin"], "/dev/null");
    let v = f.run("group", &[], &ok_limits()).unwrap();
    assert_eq!(v["pgrp"], v["pid"], "the job leads a group of its own: the group kill reaches it and what it started, never the caller");
    assert_ne!(v["pgrp"].as_u64(), Some(unsafe { getpgrp() } as u64));
}

extern "C" {
    fn getpgrp() -> i32;
}

// ---- the plan's property, in a process of its own -----------------------------------------------------------------------

fn lico_bin() -> PathBuf {
    let path = match std::env::var_os("LICO_BIN") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../private/build/host_hostd/release/lico"),
    };
    assert!(path.is_file(), "the real program is needed (build it: the suite does, or `cargo build --release --bins`); set LICO_BIN to it; not found: {}", path.display());
    path
}

fn proc_children() -> Vec<u32> {
    let mut all = Vec::new();
    for task in fs::read_dir("/proc/self/task").unwrap().flatten() {
        if let Ok(text) = fs::read_to_string(task.path().join("children")) {
            all.extend(text.split_whitespace().filter_map(|p| p.parse::<u32>().ok()));
        }
    }
    all.sort();
    all
}

fn status_field(name: &str) -> u64 {
    let text = fs::read_to_string("/proc/self/status").unwrap();
    let line = text.lines().find(|l| l.starts_with(name)).unwrap();
    line[name.len()..].trim_start_matches(':').split_whitespace().next().unwrap().parse().unwrap()
}

#[derive(Clone, Copy, Debug)]
struct Snap {
    children: usize,
    threads: u64,
    fds: usize,
    rss_kib: u64,
}

fn smaps_anon_kib() -> Option<u64> {
    let text = fs::read_to_string("/proc/self/smaps_rollup").ok()?;
    let line = text.lines().find(|l| l.starts_with("Anonymous:"))?;
    line["Anonymous:".len()..].split_whitespace().next()?.parse().ok()
}

fn snap() -> Snap {
    let rss_kib = smaps_anon_kib().unwrap_or_else(|| status_field("RssAnon") + status_field("RssShmem"));
    Snap { children: proc_children().len(), threads: status_field("Threads"), fds: fs::read_dir("/proc/self/fd").unwrap().count(), rss_kib }
}

/// The driver: runs only when the test below starts this binary again with the environment set (it is `ignore`d otherwise). It is the "host":
/// it holds no X connection, calls the helper, and its own `/proc` is read before and after. The parent started the X server, so the driver's
/// children are only the jobs.
#[test]
#[ignore = "started by the_host_is_unchanged_by_jobs in a process of its own"]
fn property_driver() {
    assert_eq!(std::env::var("LICO_HELPER_DRIVER").as_deref(), Ok("1"), "this test is started by its parent only");
    let bin = lico_bin();
    let fake = Fake::new();
    let shot = fake.file("shot.png");
    let args: Vec<String> = ["--region", "0,0,320,200", "--out", &shot].iter().map(|s| s.to_string()).collect();
    let capture = |args: &[String]| run_job(&bin, "capture", args, &JobLimits::capture());
    let hang = |ms: u64| fake.run("hang_quiet", &[&fake.file("hang.pid")], &lim(1 << 20, ms));

    // stdin of this process is a pipe the parent keeps open: a job that inherited it would show a pipe, not /dev/null
    let v = fake.run("stdin", &[], &ok_limits()).unwrap();
    assert_eq!(v["stdin"], "/dev/null", "{v}");
    assert_ne!(fs::read_link("/proc/self/fd/0").unwrap(), Path::new("/dev/null"), "the driver's own stdin must not be /dev/null for this check to mean anything");

    // warm-up: the first jobs make the allocator, kernel per-CPU VmRSS counters and the dynamic loader settle
    for _ in 0..6 {
        capture(&args).unwrap();
    }
    let _ = hang(50);
    for _ in 0..30 {
        fake.run("ok", &[], &ok_limits()).unwrap();
    }
    let before = snap();
    println!("prop: before   {before:?}");
    assert_eq!(before.children, 0, "the host's children before: {:?}", proc_children());

    let mut trace = Vec::new();
    for i in 0..50 {
        let v = capture(&args).unwrap_or_else(|e| panic!("capture {i}: {e:?}"));
        assert!(v["width"] == 320 && v["height"] == 200 && v["ok"] == 1, "{v}");
        trace.push(snap().rss_kib);
    }
    let after_captures = snap();
    println!("prop: resident size (KiB) after each of the 50 captures {trace:?}");
    println!("prop: after 50 captures {after_captures:?}");
    let began = Instant::now();
    for _ in 0..20 {
        assert_eq!(hang(50).unwrap_err().reason, "timeout");
    }
    let timeouts = began.elapsed();
    let after = snap();
    println!("prop: after 20 timeouts {after:?} ({timeouts:?} for 20, deadline 50 ms each)");
    // a long run of quick jobs: a leak of the helper itself (a buffer per job) would grow with the number of jobs, not by one step
    for _ in 0..300 {
        fake.run("ok", &[], &ok_limits()).unwrap();
    }
    let long = snap();
    println!("prop: after 300 quick jobs {long:?}");

    for (what, s) in [("50 captures", after_captures), ("20 timeouts", after), ("300 quick jobs", long)] {
        assert_eq!(s.children, 0, "children after {what}: {:?}", proc_children());
        assert_eq!(s.threads, before.threads, "threads after {what}");
        assert_eq!(s.fds, before.fds, "open files after {what}");
        assert!(s.rss_kib <= before.rss_kib + RSS_ALLOWANCE_KIB, "resident memory after {what}: {} KiB against {} KiB before (allowed +{RSS_ALLOWANCE_KIB} KiB)", s.rss_kib, before.rss_kib);
    }
    println!("prop: verdict ok (resident size allowance {RSS_ALLOWANCE_KIB} KiB)");
}

/// "Within the noise" for the resident size. Measured, not chosen: the same driver (50 captures and 20 timeouts after a warm-up) run 20 times on this
/// host, log `prop_loop20_b.log` of the slice's evidence folder (20 of 20 green), moved the resident size by 0 KiB in most runs and by +192 KiB (48 pages: one step of
/// the allocator, never more, never growing with the jobs) in some. The allowance is twice the largest step seen. The exact counts (children,
/// threads, open files) have no noise and are compared for equality.
const RSS_ALLOWANCE_KIB: u64 = 3 * 192;

struct Xvfb {
    child: std::process::Child,
    name: String,
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn xvfb(geometry: &str) -> Xvfb {
    let mut child = Command::new("Xvfb")
        .args(["-displayfd", "1", "-screen", "0", geometry, "-nolisten", "tcp", "-noreset"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Xvfb must be installed for these tests (they never skip)");
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    Xvfb { child, name: format!(":{}", line.trim()) }
}

#[test]
fn the_host_is_unchanged_by_jobs() {
    // class L3 (a private X server, 320 x 200; one test, about 8 s): the property of the plan, measured in a process of its own, because the
    // other tests of this binary start processes and threads beside it. The process is this very test binary started again on the driver above.
    let bin = lico_bin();
    let server = xvfb("320x200x24");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["helper::tests::property_driver", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env("LICO_HELPER_DRIVER", "1")
        .env("LICO_BIN", &bin)
        .env("DISPLAY", &server.name)
        .stdin(Stdio::piped()) // a pipe that stays open: see the stdin check of the driver
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    let out = child.wait_with_output().unwrap();
    drop(stdin);
    let (text, err) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    for line in text.lines().filter_map(|l| l.find("prop:").map(|at| &l[at..])) {
        println!("{line}");
    }
    assert!(out.status.success() && text.contains("prop: verdict ok"), "the driver failed:\n{text}\n{err}");
}

#[test]
fn a_running_job_that_is_dropped_is_ended_and_reaped() {
    // the safety net under every path out of `run_job` (an early return, a panic): the guard ends the group and reaps the child when it goes away
    let child = Command::new("sleep").arg("60").stdin(Stdio::null()).process_group(0).spawn().unwrap();
    let pid = child.id();
    let guard = Running { pgid: pid as i32, child, reaped: false };
    assert!(alive(pid));
    drop(guard);
    assert!(!alive(pid), "ended and reaped (no entry in /proc, not even a zombie)");
}
