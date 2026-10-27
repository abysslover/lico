//! The window's stop of an old host (`stop`) against old hosts of our own on a socket: one that closes the connection of `host.stop` and goes on listening for a
//! moment before it is gone, one that answers its status and nothing else, one whose place another host has taken. (The window with a host of another build is in
//! `tests/test_host_replacement.py`.)

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::JoinHandle;

use super::*;

fn socket_path(tag: &str) -> PathBuf {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!("lico-update-unit-{}-{tag}-{}.sock", std::process::id(), COUNT.fetch_add(1, Ordering::SeqCst)))
}

/// An old host: `host.stop` is answered with `stop_answer` (read and dropped without an answer when there is none), the status is answered with `status`
/// (held without an answer when there is none), and when `alive` says no the listener is closed and the file removed, the way the kernel does it when the
/// process is gone.
fn old_host(path: &PathBuf, status: Option<Value>, stop_answer: Option<Value>, mut alive: impl FnMut() -> bool + Send + 'static) -> JoinHandle<()> {
    let listener = UnixListener::bind(path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let path = path.clone();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        while alive() {
            let Ok((stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
            let mut line = String::new();
            let _ = BufReader::new(&stream).read_line(&mut line);
            let (answer, hold) = if line.contains("\"status\"") { (&status, true) } else { (&stop_answer, false) };
            match answer {
                Some(answer) => {
                    let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
                }
                None if hold => held.push(stream),
                None => {}
            }
        }
        drop((held, listener));
        let _ = std::fs::remove_file(path);
    })
}

fn said(stopped: &Stopped) -> String {
    match stopped {
        Stopped::Here => "Here".into(),
        Stopped::Elsewhere => "Elsewhere".into(),
        Stopped::Ended => "Ended".into(),
        Stopped::Failed(why) => format!("Failed({why})"),
    }
}

#[test]
fn a_host_that_closes_the_stop_and_listens_for_a_moment_longer_is_waited_for_and_counts_as_ended() {
    let path = socket_path("linger");
    let end = Instant::now() + Duration::from_millis(600);
    let host = old_host(&path, None, None, move || Instant::now() < end);
    let began = Instant::now();
    let stopped = stop(path.to_str().unwrap(), &json!({"pid": 4242, "build": "aa"}));
    assert!(matches!(stopped, Stopped::Ended), "{}", said(&stopped));
    assert!(began.elapsed() < Duration::from_secs(3), "the wait is for the moment the host needs, not for the whole of it: {:?}", began.elapsed());
    host.join().unwrap();
}

#[test]
fn a_host_that_does_not_answer_the_stop_but_still_answers_its_status_is_ended_by_its_pid() {
    let path = socket_path("silent");
    let mut child = std::process::Command::new("sleep").arg("10").spawn().unwrap();
    let pid = u64::from(child.id());
    let status = json!({"v": 1, "ok": 1, "pid": pid, "build": "aa"});
    let host = old_host(&path, Some(status.clone()), None, move || child.try_wait().unwrap().is_none()); // gone when the process is gone
    let began = Instant::now();
    let stopped = stop(path.to_str().unwrap(), &status);
    assert!(matches!(stopped, Stopped::Here), "{}", said(&stopped));
    assert!(began.elapsed() < Duration::from_secs(6), "the wait for a host that goes on answering has an end: {:?}", began.elapsed());
    host.join().unwrap();
}

#[test]
fn a_host_that_answers_the_stop_with_an_error_is_alive_and_is_ended_by_its_pid_without_waiting() {
    let path = socket_path("unknown");
    let mut child = std::process::Command::new("sleep").arg("10").spawn().unwrap();
    let pid = u64::from(child.id());
    let status = json!({"v": 1, "ok": 1, "pid": pid, "build": "aa"});
    let host = old_host(&path, Some(status.clone()), Some(json!({"v": 1, "ok": 0, "error": "unknown_op"})), move || child.try_wait().unwrap().is_none());
    let began = Instant::now();
    let stopped = stop(path.to_str().unwrap(), &status);
    assert!(matches!(stopped, Stopped::Here), "{}", said(&stopped));
    assert!(began.elapsed() < Duration::from_secs(2), "a host that answered is not waited for: {:?}", began.elapsed());
    host.join().unwrap();
}

#[test]
fn a_host_of_another_pid_where_the_old_one_was_is_not_the_one_to_end() {
    let path = socket_path("replaced");
    let end = Instant::now() + Duration::from_secs(2);
    let host = old_host(&path, Some(json!({"v": 1, "ok": 1, "pid": 4243, "build": "bb"})), None, move || Instant::now() < end);
    let began = Instant::now();
    let stopped = stop(path.to_str().unwrap(), &json!({"pid": 4242, "build": "aa"}));
    assert!(matches!(stopped, Stopped::Ended), "{}", said(&stopped));
    assert!(began.elapsed() < Duration::from_secs(1), "it is seen at once: {:?}", began.elapsed());
    host.join().unwrap();
}
