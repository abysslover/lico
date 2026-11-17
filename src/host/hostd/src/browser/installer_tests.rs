//! The installer against a fake release server on loopback (plain http: the test says so in its `Source`) and small fake archives made by the system `tar`. No real service.

use super::*;
use std::io::Write;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub(in crate::browser) enum Reply {
    Body(Vec<u8>),
    /// Says it is `claimed` bytes long, sends the body and closes.
    Short(Vec<u8>, usize),
    /// Answers only after the test has sent on the channel (the install is seen "installing" until then).
    Held(Vec<u8>, std::sync::mpsc::Receiver<()>),
}

pub(in crate::browser) struct Server {
    pub url: String,
    pub hits: Arc<AtomicUsize>,
}

pub(in crate::browser) fn serve(reply: Reply) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/moli-x86_64-unknown-linux-gnu.tar.gz", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { break };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 2048];
            let _ = std::io::Read::read(&mut s, &mut buf);
            let (body, claimed) = match &reply {
                Reply::Body(b) => (b.clone(), b.len()),
                Reply::Short(b, c) => (b.clone(), *c),
                Reply::Held(b, gate) => {
                    let _ = gate.recv();
                    (b.clone(), b.len())
                }
            };
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {claimed}\r\nConnection: close\r\n\r\n");
            let _ = s.write_all(&body);
        }
    });
    Server { url, hits }
}

pub(in crate::browser) fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lico_installer_{name}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub(in crate::browser) fn where_in(home: &Path) -> Where {
    Where { home: home.to_path_buf(), xdg_data: None, state_dir: home.join("state") }
}

fn digest(bytes: &[u8]) -> String {
    sha256::hex_of(bytes).unwrap()
}

pub(in crate::browser) struct Fake {
    pub bytes: Vec<u8>,
    pub archive_sha: String,
    pub engine_sha: String,
}

/// The names of files inside the archive's own folder, as a listing would show them.
pub(in crate::browser) fn inroot(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| format!("{}/{n}", lico_engine::ARCHIVE_ROOT)).collect()
}

/// An archive like the release's, made by tar: the archive's own folder with a `moli` file, a readme and a licence in a folder; `extra` runs in the folder that is archived (the one that
/// holds the archive's own folder) before it is, to add a link, a pipe...; `tar_args` are the tar options and `names` what is archived (paths below the folder that is archived).
pub(in crate::browser) fn archive(name: &str, tar_args: &[&str], names: &[String], extra: &dyn Fn(&Path)) -> Fake {
    let dir = scratch(name);
    let src = dir.join("src");
    let root = src.join(lico_engine::ARCHIVE_ROOT);
    std::fs::create_dir_all(root.join("licenses")).unwrap();
    std::fs::write(root.join("moli"), b"#!/bin/sh\necho engine\n").unwrap();
    std::fs::write(root.join("README.md"), b"a readme\n").unwrap();
    std::fs::write(root.join("licenses/NOTICE.txt"), b"a notice\n").unwrap();
    extra(&src);
    let out = dir.join("a.tar.gz");
    let status = Command::new("tar").args(["-czf"]).arg(&out).args(tar_args).arg("-C").arg(&src).args(names).status().unwrap();
    assert!(status.success());
    let bytes = std::fs::read(&out).unwrap();
    let fake = Fake { archive_sha: digest(&bytes), engine_sha: digest(&std::fs::read(root.join("moli")).unwrap()), bytes };
    let _ = std::fs::remove_dir_all(&dir);
    fake
}

pub(in crate::browser) fn good(name: &str) -> Fake {
    archive(name, &[], &inroot(&["moli", "README.md", "licenses/NOTICE.txt"]), &|_| {})
}

pub(in crate::browser) fn source_for(fake: &Fake, url: &str) -> Source {
    Source {
        url: url.to_string(),
        proto: "=http",
        archive_sha256: fake.archive_sha.clone(),
        engine_sha256: fake.engine_sha.clone(),
        max_archive: 1 << 20,
        max_seconds: 20,
        fetcher: None,
        search: None,
    }
}

/// Nothing of the install is left: no staging folder and no destination.
fn nothing_left(at: &Where) {
    let dest = at.pinned_dir();
    assert!(!dest.exists(), "no destination");
    let left: Vec<_> = std::fs::read_dir(dest.parent().unwrap()).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
    assert!(left.is_empty(), "no staging folder or archive left: {left:?}");
}

#[test]
fn the_product_source_is_the_pinned_release_over_https_and_nothing_else_reaches_it() {
    let s = Source::pinned();
    assert_eq!(s.url, lico_engine::ARCHIVE_URL);
    assert!(s.url.starts_with("https://") && s.proto == "=https");
    assert_eq!((s.archive_sha256.as_str(), s.engine_sha256.as_str()), (lico_engine::ARCHIVE_SHA256, lico_engine::ENGINE_SHA256));
    assert!(s.fetcher.is_none() && s.search.is_none(), "the product looks for curl and tar on the PATH and takes no program from anywhere else");
}

#[test]
fn a_good_archive_installs_only_the_engine_with_its_mode_and_leaves_nothing_else() {
    let home = scratch("good");
    let at = where_in(&home);
    let fake = good("good_a");
    let server = serve(Reply::Body(fake.bytes.clone()));
    install(&at, &source_for(&fake, &server.url)).unwrap();
    let dest = at.pinned_dir();
    assert_eq!(std::fs::metadata(dest.join("moli")).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o700);
    let inside: Vec<_> = std::fs::read_dir(&dest).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(inside, [std::ffi::OsString::from("moli")], "the readme and the licence of the archive are judged, not written");
    let parent: Vec<_> = std::fs::read_dir(dest.parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(parent, [std::ffi::OsString::from("moli-v1.1.25")], "only the destination: no staging folder, no archive");
    assert_eq!(server.hits.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_file_that_is_not_the_pinned_archive_is_a_security_failure_and_nothing_stays() {
    let home = scratch("mismatch");
    let at = where_in(&home);
    let fake = good("mismatch_a");
    let server = serve(Reply::Body(fake.bytes.clone()));
    let mut src = source_for(&fake, &server.url);
    src.archive_sha256 = "0".repeat(64);
    let err = install(&at, &src).unwrap_err();
    assert_eq!(err, Failure::HashMismatch);
    assert!(err.security());
    nothing_left(&at);
    // the right archive but a pin that is not the one of the file in it: the second check
    let mut src = source_for(&fake, &server.url);
    src.engine_sha256 = "1".repeat(64);
    assert_eq!(install(&at, &src).unwrap_err(), Failure::HashMismatch);
    nothing_left(&at);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_short_download_a_too_big_one_and_a_missing_curl_are_each_said_plainly() {
    let home = scratch("fail");
    let at = where_in(&home);
    let fake = good("fail_a");
    let short = serve(Reply::Short(fake.bytes[..fake.bytes.len() / 2].to_vec(), fake.bytes.len()));
    assert_eq!(install(&at, &source_for(&fake, &short.url)).unwrap_err(), Failure::Truncated);
    nothing_left(&at);
    let big = serve(Reply::Body(fake.bytes.clone()));
    let mut src = source_for(&fake, &big.url);
    src.max_archive = fake.bytes.len() as u64 - 1;
    assert_eq!(install(&at, &src).unwrap_err(), Failure::TooBig);
    nothing_left(&at);
    let mut src = source_for(&fake, &big.url);
    src.search = Some("/nonexistent-folder".into());
    assert_eq!(install(&at, &src).unwrap_err(), Failure::CurlMissing);
    nothing_left(&at);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn nothing_is_fetched_from_a_server_that_cannot_be_reached() {
    let home = scratch("net");
    let at = where_in(&home);
    let fake = good("net_a");
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/x.tar.gz", closed.local_addr().unwrap());
    drop(closed);
    assert_eq!(install(&at, &source_for(&fake, &url)).unwrap_err(), Failure::Network);
    nothing_left(&at);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_full_disk_is_said_so_and_nothing_stays() {
    let home = scratch("full");
    let at = where_in(&home);
    let fake = good("full_a");
    let script = home.join("fake_curl.sh");
    std::fs::write(&script, "#!/bin/sh\nexit 23\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut src = source_for(&fake, "http://127.0.0.1:9/x");
    src.fetcher = Some(script);
    assert_eq!(install(&at, &src).unwrap_err(), Failure::NoSpace);
    nothing_left(&at);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_archive_with_a_path_out_a_link_a_pipe_a_stranger_or_no_engine_is_refused_before_anything_is_unpacked() {
    let home = scratch("entries");
    let at = where_in(&home);
    let with = |extra: &str| -> Vec<String> {
        let mut v = inroot(&["moli"]);
        v.push(extra.to_string());
        v
    };
    let cases: Vec<(&str, Fake)> = vec![
        ("dotdot", archive("e1", &["--transform", "s,^,../,"], &inroot(&["moli"]), &|_| {})),
        ("absolute", archive("e2", &["-P", "--transform", "s,^,/tmp/lico_never_written_,"], &inroot(&["moli"]), &|_| {})),
        ("link", archive("e3", &[], &with("escape"), &|d| std::os::unix::fs::symlink("/etc/passwd", d.join("escape")).unwrap())),
        ("pipe", archive("e4", &[], &with("pipe"), &|d| assert!(Command::new("mkfifo").arg(d.join("pipe")).status().unwrap().success()))),
        ("stranger outside the archive's folder", archive("e5", &[], &with("extra"), &|d| std::fs::write(d.join("extra"), b"x").unwrap())),
        ("the engine twice", archive("e6", &[], &inroot(&["moli", "moli"]), &|_| {})),
        ("no engine", archive("e7", &[], &inroot(&["README.md"]), &|_| {})),
    ];
    for (what, fake) in cases {
        let server = serve(Reply::Body(fake.bytes.clone()));
        let err = install(&at, &source_for(&fake, &server.url)).unwrap_err();
        assert!(matches!(err, Failure::Entry(_)) && err.security(), "{what}: {err:?}");
        nothing_left(&at);
    }
    assert!(!Path::new(&format!("/tmp/lico_never_written_{}", lico_engine::ARCHIVE_ROOT)).exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_size_over_the_cap_is_refused_even_when_curl_could_not_know_it_and_a_second_installer_may_win_the_race() {
    let home = scratch("late");
    let at = where_in(&home);
    let fake = good("late_a");
    // a program in place of curl that just writes the archive (no size announced, no cap enforced by it)
    let blob = home.join("blob.tar.gz");
    std::fs::write(&blob, &fake.bytes).unwrap();
    let script = home.join("fake_curl.sh");
    std::fs::write(&script, format!("#!/bin/sh\nwhile [ \"$1\" != \"--output\" ]; do shift; done\ncp {} \"$2\"\n", blob.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut src = source_for(&fake, "http://127.0.0.1:9/x");
    src.fetcher = Some(script.clone());
    src.max_archive = fake.bytes.len() as u64 - 1;
    assert_eq!(install(&at, &src).unwrap_err(), Failure::TooBig);
    nothing_left(&at);
    // the destination was made by someone else a moment before: with the engine in it that is a success, without it it is not
    src.max_archive = 1 << 20;
    let dest = at.pinned_dir();
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("stray"), b"x").unwrap();
    assert!(install(&at, &src).is_err(), "a folder that is there without the engine in it is not an install");
    std::fs::write(dest.join("moli"), b"x").unwrap();
    assert!(install(&at, &src).is_ok(), "the other installer finished first");
    let _ = std::fs::remove_dir_all(&home);
}
