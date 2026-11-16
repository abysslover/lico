//! The contract tests of `lico-sandbox` (SPEC.md). First the pure part (`plan`), then the denials: the probe is this test binary itself, started as the confined child with
//! the test `zz_probe` and an action in its environment. Every denial has its control: the same probe, allowed, succeeds.

use lico_sandbox::*;
use std::collections::BTreeSet;
use std::io::Read;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr as UnixAddr, UnixListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// ---- the pure part --------------------------------------------------------------------------------------------------------------------------------

const EXECUTE: u64 = 1 << 0;
const WRITE_FILE: u64 = 1 << 1;
const READ_FILE: u64 = 1 << 2;
const READ_DIR: u64 = 1 << 3;
const REFER: u64 = 1 << 13;
const TRUNCATE: u64 = 1 << 14;
const IOCTL_DEV: u64 = 1 << 15;
const BIND_TCP: u64 = 1 << 0;
const CONNECT_TCP: u64 = 1 << 1;

fn dirs(_: &Path) -> Option<PathKind> {
    Some(PathKind::Dir)
}

fn files_and_dirs(p: &Path) -> Option<PathKind> {
    if p.to_string_lossy().ends_with(".txt") || p.starts_with("/dev/") {
        Some(PathKind::File)
    } else {
        Some(PathKind::Dir)
    }
}

fn conf() -> Confinement {
    Confinement { min_abi: 1, ..Confinement::default() }
}

#[test]
fn the_constants_are_named_with_the_values_the_spec_gives() {
    assert_eq!((MIN_ABI, MAX_PATHS, MAX_PORTS, MAX_ENV, MAX_ENV_BYTES), (4, 64, 16, 32, 16 * 1024));
    assert_eq!(Confinement::default().min_abi, MIN_ABI);
}

#[test]
fn the_rights_a_kernel_handles_grow_with_its_abi() {
    assert_eq!(handled_fs(1), 0x1fff);
    assert_eq!(handled_fs(2), 0x1fff | REFER);
    assert_eq!(handled_fs(3), 0x1fff | REFER | TRUNCATE);
    assert_eq!(handled_fs(4), 0x1fff | REFER | TRUNCATE);
    assert_eq!(handled_fs(5), 0x1fff | REFER | TRUNCATE | IOCTL_DEV);
    assert_eq!(handled_fs(9), handled_fs(5), "a newer kernel: only the rights this crate knows");
    assert_eq!((handled_net(3), handled_net(4)), (0, BIND_TCP | CONNECT_TCP));
}

#[test]
fn a_directory_and_a_file_get_the_rights_that_fit_them() {
    let c = Confinement {
        read_exec: vec!["/usr".into()],
        read: vec!["/data/notes.txt".into(), "/etc".into()],
        read_write: vec!["/scratch".into(), "/dev/null".into()],
        ..conf()
    };
    let p = plan(&c, 4, &files_and_dirs).unwrap();
    let rule = |path: &str| p.paths.iter().find(|r| r.path == Path::new(path)).unwrap().access;
    assert_eq!(rule("/usr"), EXECUTE | READ_FILE | READ_DIR);
    assert_eq!(rule("/etc"), READ_FILE | READ_DIR);
    assert_eq!(rule("/data/notes.txt"), READ_FILE, "a file cannot have a directory right: the kernel would refuse the rule");
    let rw = rule("/scratch");
    assert_eq!(rw, handled_fs(4) & !EXECUTE, "a scratch directory: everything but execute");
    assert_eq!(rule("/dev/null"), WRITE_FILE | READ_FILE | TRUNCATE);
    assert!(p.paths.iter().all(|r| r.access & !p.handled_fs == 0), "no rule asks for a right that is not handled");
    assert_eq!(p.paths.len(), 5);
}

#[test]
fn the_rights_of_a_scratch_directory_follow_the_abi() {
    let c = Confinement { read_write: vec!["/scratch".into()], ..conf() };
    let at = |abi| plan(&c, abi, &dirs).unwrap().paths[0].access;
    assert_eq!(at(1) & (REFER | TRUNCATE | IOCTL_DEV), 0);
    assert_eq!(at(2) & (REFER | TRUNCATE), REFER);
    assert_eq!(at(3) & (REFER | TRUNCATE), REFER | TRUNCATE);
    assert_eq!(at(5) & IOCTL_DEV, IOCTL_DEV);
    assert_eq!(at(4) & EXECUTE, 0);
}

#[test]
fn ports_become_rules_one_per_port_and_a_port_in_both_lists_has_both_rights() {
    let c = Confinement { connect_ports: vec![9222, 8080], bind_ports: vec![9222], ..conf() };
    let p = plan(&c, 4, &dirs).unwrap();
    assert_eq!(p.handled_net, BIND_TCP | CONNECT_TCP, "every TCP bind and connect is denied but these");
    assert_eq!(p.ports, vec![PortRule { port: 9222, access: BIND_TCP | CONNECT_TCP }, PortRule { port: 8080, access: CONNECT_TCP }]);
    let none = plan(&conf(), 4, &dirs).unwrap();
    assert!(none.ports.is_empty() && none.handled_net == BIND_TCP | CONNECT_TCP, "no port listed: all TCP is denied");
}

#[test]
fn it_fails_closed_for_everything_it_cannot_give_or_does_not_understand() {
    let bad = |c: Confinement, abi: u32| plan(&c, abi, &|p: &Path| if p.to_string_lossy().contains("missing") { None } else { Some(PathKind::Dir) }).unwrap_err();
    assert_eq!(bad(Confinement { min_abi: 4, ..conf() }, 3), Error::Unsupported { have: 3, need: 4 });
    assert_eq!(bad(Confinement { min_abi: 4, ..conf() }, 0), Error::Unsupported { have: 0, need: 4 });
    assert_eq!(bad(Confinement { connect_ports: vec![80], ..conf() }, 3), Error::Unsupported { have: 3, need: 4 }, "a port rule needs ABI 4 even when min_abi was lowered");
    assert!(matches!(bad(Confinement { read: vec!["relative/dir".into()], ..conf() }, 4), Error::Path(_)));
    assert!(matches!(bad(Confinement { read: vec!["/a/../etc".into()], ..conf() }, 4), Error::Path(_)));
    assert!(matches!(bad(Confinement { read: vec!["/".into()], ..conf() }, 4), Error::Path(_)), "the root confines nothing");
    assert!(matches!(bad(Confinement { read: vec!["/missing/dir".into()], ..conf() }, 4), Error::Path(_)));
    assert!(matches!(bad(Confinement { read: vec!["/a\0b".into()], ..conf() }, 4), Error::Path(_)));
    assert!(matches!(bad(Confinement { read: vec!["/etc".into()], read_write: vec!["/etc".into()], ..conf() }, 4), Error::Path(_)), "a path listed twice");
    assert!(matches!(bad(Confinement { read: (0..=MAX_PATHS).map(|i| format!("/p{i}").into()).collect(), ..conf() }, 4), Error::TooMany(_)));
    assert!(matches!(bad(Confinement { connect_ports: vec![0], ..conf() }, 4), Error::Port(_)));
    assert!(matches!(bad(Confinement { connect_ports: vec![80, 80], ..conf() }, 4), Error::Port(_)));
    assert!(matches!(bad(Confinement { bind_ports: (1..=(MAX_PORTS as u16 + 1)).collect(), ..conf() }, 4), Error::TooMany(_)));
    assert!(matches!(bad(Confinement { cwd: "relative".into(), ..conf() }, 4), Error::Path(_)));
    assert!(matches!(bad(Confinement { cwd: "/missing/cwd".into(), ..conf() }, 4), Error::Path(_)));
}

#[test]
fn the_environment_is_checked_name_by_name_and_by_size() {
    let env = |pairs: &[(&str, &str)]| Confinement { env: pairs.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(), ..conf() };
    assert!(plan(&env(&[("OBSCURA_CDP_TOKEN", "abc"), ("_X1", "")]), 4, &dirs).is_ok());
    for bad in [("", "v"), ("1BAD", "v"), ("A-B", "v"), ("A B", "v"), ("A=B", "v"), ("A", "v\0w")] {
        assert!(matches!(plan(&env(&[bad]), 4, &dirs), Err(Error::Env(_))), "{bad:?}");
    }
    assert!(matches!(plan(&env(&[("A", "1"), ("A", "2")]), 4, &dirs), Err(Error::Env(_))), "a name twice");
    let many: Vec<(String, String)> = (0..=MAX_ENV).map(|i| (format!("V{i}"), "x".into())).collect();
    assert!(matches!(plan(&Confinement { env: many, ..conf() }, 4, &dirs), Err(Error::TooMany(_))));
    let big = Confinement { env: vec![("BIG".into(), "x".repeat(MAX_ENV_BYTES))], ..conf() };
    assert!(matches!(plan(&big, 4, &dirs), Err(Error::TooMany(_))), "over the byte cap");
}

#[test]
fn a_plan_is_the_same_every_time_and_no_debug_shows_an_environment_value() {
    let c = Confinement { read_exec: vec!["/usr".into(), "/lib".into()], connect_ports: vec![3, 1, 2], env: vec![("OBSCURA_CDP_TOKEN".into(), "s3cret-token-value".into())], ..conf() };
    assert_eq!(plan(&c, 4, &dirs), plan(&c, 4, &dirs));
    assert_eq!(plan(&c, 4, &dirs).unwrap().ports.iter().map(|p| p.port).collect::<Vec<_>>(), [3, 1, 2], "the order of the lists");
    let shown = format!("{c:?} {:?} {:?}", plan(&c, 4, &dirs), Error::Env("x"));
    assert!(!shown.contains("s3cret-token-value") && shown.contains("OBSCURA_CDP_TOKEN"), "{shown}");
}

// ---- the denials, with a probe ----------------------------------------------------------------------------------------------------------------------

fn tmp() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sandbox")
}

fn fresh(name: &str) -> PathBuf {
    let d = tmp().join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn exe() -> PathBuf {
    std::env::current_exe().unwrap()
}

fn landlock() {
    let abi = abi_version().unwrap_or(0);
    assert!(abi >= MIN_ABI, "these tests need a kernel with Landlock ABI {MIN_ABI} or more (this one has {abi}): they do not pass by skipping");
}

/// What the probe is allowed to read and run: the test binary's directory and the system libraries.
fn base(scratch: &Path) -> Confinement {
    Confinement {
        read_exec: vec![exe().parent().unwrap().to_path_buf(), "/usr".into(), "/lib".into(), "/lib64".into()],
        read: vec!["/etc/ld.so.cache".into()],
        read_write: vec![scratch.to_path_buf()],
        cwd: scratch.to_path_buf(),
        ..Confinement::default()
    }
}

#[derive(Debug, PartialEq)]
enum Probe {
    Ok(String),
    Err(String),
}

fn command(conf: &Confinement, action: &str, arg: &str) -> Command {
    let mut c = conf.clone();
    c.env.push(("LICO_PROBE_ACTION".into(), action.into()));
    c.env.push(("LICO_PROBE_ARG".into(), arg.into()));
    let mut cmd = Command::new(exe());
    cmd.args(["zz_probe", "--exact", "--nocapture", "--test-threads=1"]).stdout(Stdio::piped()).stderr(Stdio::piped());
    confine(&mut cmd, &c).unwrap();
    cmd
}

fn parse(stdout: &str, stderr: &str) -> Probe {
    match stdout.lines().find_map(|l| l.find("PROBE:").map(|at| &l[at + 6..])) {
        Some(r) => match r.strip_prefix("err:") {
            Some(e) => Probe::Err(e.to_string()),
            None => Probe::Ok(r.strip_prefix("ok:").unwrap_or(r).to_string()),
        },
        None => panic!("the probe printed no result: stdout {stdout:?} stderr {stderr:?}"),
    }
}

fn run(conf: &Confinement, action: &str, arg: &str) -> Probe {
    let out = command(conf, action, arg).output().expect("the confined probe starts");
    parse(&String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr))
}

fn denied(p: &Probe) -> bool {
    matches!(p, Probe::Err(e) if e.starts_with("PermissionDenied") || e.contains("os error 13") || e.contains("code 13"))
}

#[test]
fn a_file_outside_the_allowed_paths_is_denied_and_an_allowed_one_is_read() {
    landlock();
    let scratch = fresh("read_scratch");
    let outside = fresh("read_outside");
    std::fs::write(outside.join("secret.txt"), "not for the child").unwrap();
    std::fs::write(scratch.join("mine.txt"), "for the child").unwrap();
    let c = base(&scratch);
    assert_eq!(run(&c, "read", scratch.join("mine.txt").to_str().unwrap()), Probe::Ok("for the child".into()), "the control: an allowed file is read");
    let p = run(&c, "read", outside.join("secret.txt").to_str().unwrap());
    assert!(denied(&p), "{p:?}");
    let allowed_now = Confinement { read: vec![outside.clone(), "/etc/ld.so.cache".into()], ..c.clone() };
    assert_eq!(run(&allowed_now, "read", outside.join("secret.txt").to_str().unwrap()), Probe::Ok("not for the child".into()), "the control: allowed, the same read succeeds");
    let p = run(&c, "read", "/etc/hostname");
    assert!(denied(&p), "a system file that is not listed: {p:?}");
}

#[test]
fn a_write_outside_the_scratch_directory_is_denied_and_one_inside_is_made() {
    landlock();
    let scratch = fresh("write_scratch");
    let outside = fresh("write_outside");
    let c = base(&scratch);
    assert_eq!(run(&c, "write", scratch.join("new.txt").to_str().unwrap()), Probe::Ok("written".into()));
    assert_eq!(std::fs::read_to_string(scratch.join("new.txt")).unwrap(), "written by the probe");
    let p = run(&c, "write", outside.join("new.txt").to_str().unwrap());
    assert!(denied(&p) && !outside.join("new.txt").exists(), "{p:?}");
    let p = run(&c, "mkdir", outside.join("d").to_str().unwrap());
    assert!(denied(&p) && !outside.join("d").exists(), "{p:?}");
    let p = run(&c, "write", exe().parent().unwrap().join("planted.txt").to_str().unwrap());
    assert!(denied(&p), "a directory that may be read and executed may not be written: {p:?}");
    let p = run(&c, "exec", scratch.join("new.txt").to_str().unwrap());
    assert!(denied(&p), "nothing in the scratch directory may be executed: {p:?}");
}

fn listener() -> (std::net::TcpListener, u16) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    (l, port)
}

#[test]
fn a_connection_to_a_port_that_is_not_allowed_is_denied_and_to_the_allowed_one_works() {
    landlock();
    let scratch = fresh("connect");
    let ((_a, allowed), (_b, other)) = (listener(), listener());
    let c = Confinement { connect_ports: vec![allowed], ..base(&scratch) };
    assert_eq!(run(&c, "connect", &allowed.to_string()), Probe::Ok("connected".into()), "the control");
    let p = run(&c, "connect", &other.to_string());
    assert!(denied(&p), "{p:?}");
    let none = base(&scratch);
    let p = run(&none, "connect", &allowed.to_string());
    assert!(denied(&p), "no port listed: no connection: {p:?}");
}

#[test]
fn a_bind_to_a_port_that_is_not_allowed_is_denied_and_to_the_allowed_one_works() {
    landlock();
    let scratch = fresh("bind");
    let (free, p1) = listener();
    drop(free);
    let c = Confinement { bind_ports: vec![p1], ..base(&scratch) };
    assert_eq!(run(&c, "bind", &p1.to_string()), Probe::Ok("bound".into()), "the control");
    let (free2, p2) = listener();
    drop(free2);
    let p = run(&c, "bind", &p2.to_string());
    assert!(denied(&p), "{p:?}");
    let p = run(&c, "bind", "0");
    assert!(denied(&p), "port 0 (any) is not the allowed port either: {p:?}");
}

#[test]
fn the_child_sees_exactly_the_environment_it_was_given() {
    landlock();
    let scratch = fresh("env");
    let c = Confinement { env: vec![("OBSCURA_CDP_TOKEN".into(), "t".into()), ("LANG".into(), "C".into())], ..base(&scratch) };
    let Probe::Ok(names) = run(&c, "env", "") else { panic!() };
    let got: BTreeSet<&str> = names.split(',').filter(|s| !s.is_empty()).collect();
    let want: BTreeSet<&str> = ["OBSCURA_CDP_TOKEN", "LANG", "LICO_PROBE_ACTION", "LICO_PROBE_ARG"].into();
    assert_eq!(got, want, "the test's own environment (PATH, HOME, CARGO_*...) is not passed");
    assert!(std::env::var_os("PATH").is_some(), "the test's own environment is not empty");
}

#[test]
fn the_working_directory_is_the_given_one() {
    landlock();
    let scratch = fresh("cwd");
    assert_eq!(run(&base(&scratch), "cwd", ""), Probe::Ok(scratch.to_string_lossy().into_owned()));
}

#[test]
fn no_descriptor_of_the_parent_is_open_in_the_child() {
    landlock();
    let scratch = fresh("fds");
    // a descriptor that is NOT close-on-exec, made on purpose: nothing of the parent may survive
    let leak = std::fs::File::open("/dev/null").unwrap();
    let fd = std::os::fd::AsRawFd::as_raw_fd(&leak);
    extern "C" {
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }
    unsafe {
        let flags = fcntl(fd, 1);
        fcntl(fd, 2, flags & !1); // F_GETFD, F_SETFD without FD_CLOEXEC
    }
    let Probe::Ok(open) = run(&base(&scratch), "fds", "") else { panic!() };
    assert_eq!(open, "", "descriptors above 2 that are open in the child: {open:?}");
    drop(leak);
}

// ---- running the file that was opened (0.2.0) ------------------------------------------------------------------------------------------------------

fn fd_probe(conf: &Confinement, action: &str, file: &std::fs::File) -> Probe {
    let mut c = conf.clone();
    c.env.push(("LICO_PROBE_ACTION".into(), action.into()));
    let mut cmd = confine_fd(file, &c).unwrap();
    cmd.args(["zz_probe", "--exact", "--nocapture", "--test-threads=1"]).stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = cmd.output().expect("the confined probe starts from the open file");
    parse(&String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr))
}

#[test]
fn the_open_file_runs_under_the_fence_and_leaves_no_descriptor_in_the_child() {
    landlock();
    let scratch = fresh("fd_runs");
    let file = std::fs::File::open(exe()).unwrap();
    // the path walk through /proc/self/fd/N happens after the fence is applied: a real run proves the engine's directory right covers the opened file
    assert_eq!(fd_probe(&base(&scratch), "cwd", &file), Probe::Ok(scratch.to_string_lossy().into_owned()));
    assert_eq!(fd_probe(&base(&scratch), "fds", &file), Probe::Ok(String::new()), "the descriptor of the program is not open after the exec");
    // the fence still holds for a program started this way
    let outside = fresh("fd_outside");
    std::fs::write(outside.join("secret.txt"), "no").unwrap();
    let mut c = base(&scratch);
    c.env.push(("LICO_PROBE_ARG".into(), outside.join("secret.txt").to_string_lossy().into_owned()));
    assert!(denied(&fd_probe(&c, "read", &file)));
}

#[test]
fn the_file_that_was_hashed_runs_even_when_the_path_is_replaced_after_the_hash() {
    landlock();
    let dir = fresh("fd_swap");
    let path = dir.join("engine");
    std::fs::copy("/usr/bin/true", &path).unwrap();
    let opened = std::fs::File::open(&path).unwrap(); // the file that was checked and hashed
    std::fs::copy("/usr/bin/false", dir.join("other")).unwrap();
    std::fs::rename(dir.join("other"), &path).unwrap(); // another program takes the path after the hash
    let c = Confinement { read_exec: vec![dir.clone(), "/usr".into(), "/lib".into(), "/lib64".into()], read: vec!["/etc/ld.so.cache".into()], cwd: dir.clone(), ..Confinement::default() };
    let status = confine_fd(&opened, &c).unwrap().status().unwrap();
    assert!(status.success(), "the opened program (true) ran, not the one now at the path");
    let mut by_path = Command::new(&path);
    confine(&mut by_path, &c).unwrap();
    assert!(!by_path.status().unwrap().success(), "the control: by path the replacement (false) runs");
}

#[test]
fn an_open_file_that_could_not_run_is_refused_before_any_process() {
    landlock();
    let scratch = fresh("fd_refuse");
    // the scratch directory is also a read_exec path here, so that only the kind of file decides in the first two cases
    let mut c = base(&scratch);
    c.read_exec.push(scratch.clone());
    c.read_write.clear();
    // a directory, a file that is not executable, and a program outside every read_exec path
    assert!(matches!(confine_fd(&std::fs::File::open(&scratch).unwrap(), &c), Err(Error::Path(_))));
    std::fs::write(scratch.join("plain.txt"), "x").unwrap();
    assert!(matches!(confine_fd(&std::fs::File::open(scratch.join("plain.txt")).unwrap(), &c), Err(Error::Path(_))));
    let outside = Confinement { read_exec: vec!["/usr".into(), "/lib".into(), "/lib64".into()], ..c.clone() };
    assert!(matches!(confine_fd(&std::fs::File::open(exe()).unwrap(), &outside), Err(Error::Path(_))), "the file lies outside the read_exec paths");
    assert!(confine_fd(&std::fs::File::open("/usr/bin/true").unwrap(), &outside).is_ok(), "the control: a program under /usr is accepted");
}

#[test]
fn privileges_cannot_be_regained_the_child_leads_its_own_session_and_is_not_the_parents_group() {
    landlock();
    let scratch = fresh("nnp");
    let c = base(&scratch);
    assert_eq!(run(&c, "nnp", ""), Probe::Ok("1".into()));
    let Probe::Ok(s) = run(&c, "session", "") else { panic!() };
    let (pid, sid): (i32, i32) = s.split_once(',').map(|(a, b)| (a.parse().unwrap(), b.parse().unwrap())).unwrap();
    assert_eq!(pid, sid, "the child is the leader of a new session");
    extern "C" {
        fn getsid(pid: i32) -> i32;
    }
    assert_ne!(sid, unsafe { getsid(0) });
}

#[test]
fn the_child_dies_with_its_parent() {
    landlock();
    let scratch = fresh("pdeath");
    // the parent of the confined child is another copy of this test binary (an unconfined one); the test kills it and the child must follow
    let mut parent = Command::new(exe())
        .args(["zz_probe", "--exact", "--nocapture", "--test-threads=1"])
        .env("LICO_PROBE_ACTION", "parent")
        .env("LICO_PROBE_ARG", scratch.to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = String::new();
    let mut buf = [0u8; 256];
    let stdout = parent.stdout.as_mut().unwrap();
    let t = Instant::now();
    let child_pid: i32 = loop {
        let n = stdout.read(&mut buf).unwrap();
        out.push_str(&String::from_utf8_lossy(&buf[..n]));
        if let Some(pid) = out.lines().find_map(|l| l.find("PROBE:pid:").map(|at| &l[at + 10..])).and_then(|p| p.trim().parse().ok()) {
            break pid;
        }
        assert!(n > 0 && t.elapsed() < Duration::from_secs(10), "the intermediate parent said nothing: {out:?}");
    };
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    assert_eq!(unsafe { kill(child_pid, 0) }, 0, "the child is alive while its parent is");
    parent.kill().unwrap();
    parent.wait().unwrap();
    let t = Instant::now();
    while unsafe { kill(child_pid, 0) } == 0 {
        assert!(t.elapsed() < Duration::from_secs(3), "the child is still alive after its parent died");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_kernel_with_less_than_asked_means_no_process() {
    landlock();
    let scratch = fresh("minabi");
    let mut cmd = Command::new(exe());
    let c = Confinement { min_abi: 99, ..base(&scratch) };
    let e = confine(&mut cmd, &c).unwrap_err();
    assert!(matches!(e, Error::Unsupported { need: 99, .. }), "{e:?}");
    let mut cmd = Command::new("/usr/bin/true");
    assert!(confine(&mut cmd, &Confinement { read_exec: vec![scratch.clone()], cwd: scratch.clone(), ..Confinement::default() }).is_err(), "a program that is not under read_exec could not run: refused at once");
    let mut cmd = Command::new("true");
    assert!(confine(&mut cmd, &base(&scratch)).is_err(), "a program that is not an absolute path");
}

#[test]
fn an_address_space_limit_stops_a_probe_that_allocates_more() {
    landlock();
    let scratch = fresh("limit");
    let free = base(&scratch);
    assert_eq!(run(&free, "alloc", "64"), Probe::Ok("allocated".into()), "the control: 64 MiB with no limit");
    let limited = Confinement { limits: Limits { address_space: Some(512 << 20), ..Limits::default() }, ..base(&scratch) };
    let p = run(&limited, "alloc", "1024");
    assert!(matches!(&p, Probe::Err(e) if e.contains("memory") || e.contains("alloc")), "{p:?}");
    assert_eq!(run(&limited, "alloc", "32"), Probe::Ok("allocated".into()), "a smaller one still works under the limit");
}

#[test]
fn the_files_that_are_open_are_limited_when_asked() {
    landlock();
    let scratch = fresh("nofile");
    let limited = Confinement { limits: Limits { open_files: Some(16), ..Limits::default() }, ..base(&scratch) };
    let p = run(&limited, "openmany", &scratch.join("f").to_string_lossy());
    assert!(matches!(&p, Probe::Err(e) if e.contains("Too many") || e.contains("os error 24")), "{p:?}");
}

#[test]
fn strace_itself_works_under_the_confinement_and_sees_what_its_tracee_tries_to_open() {
    landlock();
    if !Path::new("/usr/bin/strace").exists() {
        println!("strace is not installed: this check is skipped (the engine probe then runs without it)");
        return;
    }
    let scratch = fresh("strace");
    let outside = fresh("strace_outside");
    let log = scratch.join("t.txt");
    let mut cmd = Command::new("/usr/bin/strace");
    cmd.args(["-f", "-qq", "-e", "trace=execve,openat", "-o"]).arg(&log).args(["--", "/usr/bin/cat"]).arg(outside.join("secret.txt")).stdout(Stdio::null()).stderr(Stdio::null());
    std::fs::write(outside.join("secret.txt"), "x").unwrap();
    confine(&mut cmd, &base(&scratch)).unwrap();
    let status = cmd.status().unwrap();
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("execve(\"/usr/bin/cat\""), "{text}");
    assert!(text.lines().any(|l| l.contains("secret.txt") && l.contains("EACCES")), "the denied open is in the trace: {text}");
    assert!(!status.success(), "cat could not read the file");
}

// ---- what the fence does NOT close: each is pinned, so that a kernel that begins to cover it makes the test fail and SPEC.md is revisited -----------------

#[test]
fn known_gap_udp_to_loopback_is_not_stopped() {
    landlock();
    let scratch = fresh("udp");
    let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let port = sock.local_addr().unwrap().port();
    assert_eq!(run(&base(&scratch), "udp", &port.to_string()), Probe::Ok("sent".into()), "UDP is not covered by Landlock ABI 4");
    let mut buf = [0u8; 16];
    assert_eq!(sock.recv(&mut buf).unwrap(), 4, "the datagram arrived");
}

#[test]
fn known_gap_an_abstract_unix_socket_is_not_stopped() {
    landlock();
    let scratch = fresh("abstract");
    let name = format!("lico-sandbox-test-{}", std::process::id());
    let addr = UnixAddr::from_abstract_name(name.as_bytes()).unwrap();
    let listener = UnixListener::bind_addr(&addr).unwrap();
    let h = std::thread::spawn(move || listener.accept().map(|_| ()));
    assert_eq!(run(&base(&scratch), "abstract", &name), Probe::Ok("connected".into()), "abstract sockets are not covered");
    assert!(h.join().unwrap().is_ok());
}

#[test]
fn known_gap_a_unix_socket_by_path_in_an_allowed_directory_is_not_stopped() {
    landlock();
    // a socket path must be under 108 bytes: a short directory in memory (tmpfs), removed at the end
    let scratch = PathBuf::from(format!("/dev/shm/lico-sbx-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let h = std::thread::spawn(move || listener.accept().map(|_| ()));
    assert_eq!(run(&base(&scratch), "unix", path.to_str().unwrap()), Probe::Ok("connected".into()));
    assert!(h.join().unwrap().is_ok());
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn known_gap_a_signal_to_another_process_of_the_same_user_is_not_stopped() {
    landlock();
    let scratch = fresh("signal");
    let mut other = Command::new("/usr/bin/sleep").arg("30").spawn().unwrap();
    let pid = other.id();
    let p = run(&base(&scratch), "signal0", &pid.to_string());
    other.kill().unwrap();
    other.wait().unwrap();
    assert_eq!(p, Probe::Ok("signalled".into()), "signals are not covered");
}

// ---- the probe itself -----------------------------------------------------------------------------------------------------------------------------

fn say(r: Result<String, String>) {
    match r {
        Ok(s) => println!("PROBE:ok:{s}"),
        Err(e) => println!("PROBE:err:{e}"),
    }
}

fn io<T>(r: std::io::Result<T>) -> Result<T, String> {
    r.map_err(|e| format!("{:?} (os error {})", e.kind(), e.raw_os_error().unwrap_or(0)))
}

/// Not a test of anything by itself: with no `LICO_PROBE_ACTION` it does nothing; the tests above start this binary as the confined child and read what it prints.
#[test]
fn zz_probe() {
    let (Ok(action), arg) = (std::env::var("LICO_PROBE_ACTION"), std::env::var("LICO_PROBE_ARG").unwrap_or_default()) else { return };
    extern "C" {
        fn getpid() -> i32;
        fn getsid(pid: i32) -> i32;
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
        fn prctl(option: i32, a2: u64, a3: u64, a4: u64, a5: u64) -> i32;
        fn kill(pid: i32, sig: i32) -> i32;
    }
    match action.as_str() {
        "read" => say(io(std::fs::read_to_string(&arg)).map(|s| s)),
        "write" => say(io(std::fs::write(&arg, "written by the probe")).map(|_| "written".into())),
        "mkdir" => say(io(std::fs::create_dir(&arg)).map(|_| "made".into())),
        "exec" => say(io(Command::new(&arg).output()).map(|_| "ran".into())),
        "connect" => say(io(std::net::TcpStream::connect_timeout(&format!("127.0.0.1:{arg}").parse().unwrap(), Duration::from_secs(2))).map(|_| "connected".into())),
        "bind" => say(io(std::net::TcpListener::bind(format!("127.0.0.1:{arg}"))).map(|_| "bound".into())),
        "env" => {
            let mut names: Vec<String> = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned()).collect();
            names.sort();
            say(Ok(names.join(",")));
        }
        "cwd" => say(io(std::env::current_dir()).map(|p| p.to_string_lossy().into_owned())),
        "fds" => {
            let open: Vec<String> = (3..1024).filter(|fd| unsafe { fcntl(*fd, 1) } != -1).map(|fd| fd.to_string()).collect();
            say(Ok(open.join(",")));
        }
        "nnp" => say(Ok(unsafe { prctl(39, 0, 0, 0, 0) }.to_string())),
        "session" => say(Ok(format!("{},{}", unsafe { getpid() }, unsafe { getsid(0) }))),
        "alloc" => {
            let mb: usize = arg.parse().unwrap();
            let mut v: Vec<u8> = Vec::new();
            match v.try_reserve_exact(mb << 20) {
                Ok(()) => {
                    v.resize(mb << 20, 1);
                    say(Ok("allocated".into()));
                }
                Err(_) => say(Err("memory: allocation refused".into())),
            }
        }
        "openmany" => {
            let mut keep = Vec::new();
            for i in 0..64 {
                match std::fs::File::create(format!("{arg}{i}")) {
                    Ok(f) => keep.push(f),
                    Err(e) => return say(Err(format!("{e} (os error {})", e.raw_os_error().unwrap_or(0)))),
                }
            }
            say(Ok("opened".into()));
        }
        "udp" => {
            let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            say(io(s.send_to(b"ping", format!("127.0.0.1:{arg}"))).map(|_| "sent".into()));
        }
        "abstract" => {
            let addr = UnixAddr::from_abstract_name(arg.as_bytes()).unwrap();
            say(io(std::os::unix::net::UnixStream::connect_addr(&addr)).map(|_| "connected".into()));
        }
        "unix" => say(io(std::os::unix::net::UnixStream::connect(&arg)).map(|_| "connected".into())),
        "signal0" => say(if unsafe { kill(arg.parse().unwrap(), 0) } == 0 { Ok("signalled".into()) } else { Err("kill failed".into()) }),
        "parent" => {
            // an unconfined parent of a confined sleeper: it says the sleeper's pid and then waits to be killed
            let scratch = PathBuf::from(&arg);
            let mut cmd = command(&base(&scratch), "sleep", "");
            cmd.stdout(Stdio::null());
            let child = cmd.spawn().unwrap();
            println!("PROBE:pid:{}", child.id());
            std::thread::sleep(Duration::from_secs(60));
        }
        "sleep" => std::thread::sleep(Duration::from_secs(60)),
        other => say(Err(format!("unknown action {other}"))),
    }
}
