//! The engine installs itself (M17-P10; requested by the owner in the chat, 2026-11-17, the spec wording approved by the lead, to be confirmed by the owner). Only the pinned
//! archive from the pinned URL is fetched, by the system `curl` as a helper process (Lico has no HTTP or TLS code and no new dependency); what makes it safe is not the channel
//! but the pinned SHA-256, checked on the open file. The archive is listed by the system `tar` and the listing is judged by the pure part (`lico-engine-pack`): exactly the
//! two named regular files. Both files are hashed again, get their modes, and the folder is moved to its place with one atomic rename. Everything happens in a fresh `0700` staging
//! folder beside the destination that is removed on every way out. No environment variable, file or argument changes the URL or the hashes (`Source::pinned` is the only source
//! of a product build); the tests build a `Source` of their own.

use super::engine::{check_chain, Problem, Where};
use super::sha256;
use lico_engine_pack::{self as pack, Failure};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const O_NOFOLLOW: i32 = 0o400_000;
const O_CLOEXEC: i32 = 0o2_000_000;
/// How long the whole transfer may take before curl stops it (a slow line, not a hang).
const MAX_SECONDS: u32 = 900;
const CONNECT_SECONDS: u32 = 30;

/// What is fetched and what it must be. The product has one, `pinned()`.
#[derive(Clone)]
pub struct Source {
    pub url: String,
    /// `https` in the product; a test server speaks plain http and says so here.
    pub proto: &'static str,
    pub archive_sha256: String,
    pub engine_sha256: String,
    pub worker_sha256: String,
    pub max_archive: u64,
    pub max_seconds: u32,
    /// The program to run in place of the one found as `curl` (None: look for it).
    pub fetcher: Option<PathBuf>,
    /// The folders searched for `curl` and `tar` (None: the process's own PATH).
    pub search: Option<OsString>,
}

impl Source {
    pub fn pinned() -> Source {
        Source {
            url: pack::ARCHIVE_URL.to_string(),
            proto: "=https",
            archive_sha256: pack::ARCHIVE_SHA256.to_string(),
            engine_sha256: pack::ENGINE_SHA256.to_string(),
            worker_sha256: pack::WORKER_SHA256.to_string(),
            max_archive: pack::MAX_ARCHIVE_BYTES,
            max_seconds: MAX_SECONDS,
            fetcher: None,
            search: None,
        }
    }
}

/// The host (and port) of an address, as the tab names the source.
pub fn host_of(url: &str) -> String {
    url.split_once("://").map_or(url, |(_, rest)| rest).split('/').next().unwrap_or("").chars().take(80).collect()
}

/// The source a running host uses: the pinned one. Only a build with the `install_seam` feature (the system tests' build, never the release) can name another, from a file.
#[cfg(not(feature = "install_seam"))]
pub fn source_of_process() -> Source {
    Source::pinned()
}

#[cfg(feature = "install_seam")]
pub fn source_of_process() -> Source {
    let read = || -> Option<Source> {
        let path = std::env::var_os("LICO_INSTALL_SEAM_FILE")?;
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        let text = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
        Some(Source {
            url: text("url")?,
            proto: "=http",
            archive_sha256: text("archive_sha256")?,
            engine_sha256: text("engine_sha256")?,
            worker_sha256: text("worker_sha256")?,
            max_archive: v.get("max_archive").and_then(|x| x.as_u64()).unwrap_or(1 << 20),
            max_seconds: 30,
            fetcher: None,
            search: None,
        })
    };
    read().unwrap_or_else(Source::pinned)
}

fn io_failure(e: &std::io::Error) -> Failure {
    if e.raw_os_error() == Some(28) {
        Failure::NoSpace
    } else {
        Failure::Other(format!("{:?}", e.kind()))
    }
}

/// An executable named `name` in one of the absolute folders of `search`.
fn find_tool(name: &str, search: &Option<OsString>) -> Option<PathBuf> {
    let path = search.clone().or_else(|| std::env::var_os("PATH"))?;
    std::env::split_paths(&path).filter(|d| d.is_absolute()).map(|d| d.join(name)).find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// The staging folder: removed when it goes out of scope, whatever happened.
struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Creates every missing folder of `dir` with mode 0700.
fn make_dirs(dir: &Path) -> std::io::Result<()> {
    let mut missing: Vec<&Path> = dir.ancestors().take_while(|p| !p.exists()).collect();
    missing.reverse();
    for p in missing {
        std::fs::DirBuilder::new().mode(0o700).create(p)?;
    }
    Ok(())
}

fn open_plain(path: &Path) -> Result<File, Failure> {
    let file = OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_CLOEXEC).open(path).map_err(|e| io_failure(&e))?;
    let m = file.metadata().map_err(|e| io_failure(&e))?;
    if !m.is_file() {
        return Err(Failure::Entry("not a plain file".into()));
    }
    Ok(file)
}

fn digest_of(file: &mut File) -> Result<String, Failure> {
    file.seek(SeekFrom::Start(0)).map_err(|e| io_failure(&e))?;
    sha256::hex_of(&mut *file).map_err(|e| io_failure(&e))
}

/// Runs a helper to its end; the helper's standard input is `input` (rewound), its output is read up to `cap` bytes (more is an error), its error text up to 2 KiB.
fn run_tool(mut cmd: Command, input: Option<&mut File>, cap: usize) -> Result<(std::process::ExitStatus, Vec<u8>, String), Failure> {
    cmd.env_clear().env("LC_ALL", "C").env("TZ", "UTC").stdout(Stdio::piped()).stderr(Stdio::piped());
    match input {
        Some(f) => {
            f.seek(SeekFrom::Start(0)).map_err(|e| io_failure(&e))?;
            cmd.stdin(Stdio::from(f.try_clone().map_err(|e| io_failure(&e))?));
        }
        None => {
            cmd.stdin(Stdio::null());
        }
    }
    let mut child: Child = cmd.spawn().map_err(|e| io_failure(&e))?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    // The error text is read on a thread so that neither pipe can fill up and stop the helper.
    let reader = std::thread::spawn(move || {
        if let Some(e) = stderr.as_mut() {
            let _ = e.take(2048).read_to_end(&mut err);
            let _ = std::io::copy(e, &mut std::io::sink());
        }
        err
    });
    let mut too_long = false;
    if let Some(o) = stdout.as_mut() {
        let _ = o.take(cap as u64 + 1).read_to_end(&mut out);
        if out.len() > cap {
            too_long = true;
            let _ = child.kill();
        }
        let _ = std::io::copy(o, &mut std::io::sink());
    }
    let status = child.wait().map_err(|e| io_failure(&e))?;
    let err = reader.join().unwrap_or_default();
    if too_long {
        return Err(Failure::Entry("a listing that is too long".into()));
    }
    Ok((status, out, String::from_utf8_lossy(&err).into_owned()))
}

/// Fetches, checks, unpacks and installs. `Ok` means the pinned engine is in place (`at.pinned_dir()`); on `Err` nothing is left.
pub fn install(at: &Where, src: &Source) -> Result<(), Failure> {
    let curl = match &src.fetcher {
        Some(p) => p.clone(),
        None => find_tool("curl", &src.search).ok_or(Failure::CurlMissing)?,
    };
    let tar = find_tool("tar", &src.search).ok_or(Failure::TarMissing)?;
    let dest = at.pinned_dir();
    let parent = dest.parent().ok_or(Failure::Cannot("no place to install to"))?.to_path_buf();
    make_dirs(&parent).map_err(|e| io_failure(&e))?;
    check_chain(&at.home, &parent).map_err(|p| match p {
        Problem::Unsafe(why) => Failure::Cannot(why),
        _ => Failure::Cannot("the install folder cannot be checked"),
    })?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let staging = parent.join(format!(".install-{}-{nanos}", std::process::id()));
    std::fs::DirBuilder::new().mode(0o700).create(&staging).map_err(|e| io_failure(&e))?;
    let staging = Staging(staging);
    let archive_path = staging.0.join("archive.tar.gz");

    // 1. the download, by curl: no configuration file, no proxy, https only, a size and a time cap, written to the file (nothing is held here)
    let mut fetch = Command::new(&curl);
    fetch
        .env_clear()
        .arg("-q")
        .args(["--fail", "--silent", "--show-error", "--location", "--noproxy", "*", "--proto", src.proto, "--proto-redir", src.proto])
        .args(["--max-filesize", &src.max_archive.to_string(), "--max-time", &src.max_seconds.to_string(), "--connect-timeout", &CONNECT_SECONDS.to_string()])
        .arg("--output")
        .arg(&archive_path)
        .arg("--")
        .arg(&src.url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = fetch.status().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { Failure::CurlMissing } else { io_failure(&e) })?;
    if !status.success() {
        return Err(pack::failure_of_curl_exit(status.code().unwrap_or(-1)));
    }

    // 2. the pinned hash, through the open descriptor; a size over the cap is refused even if curl could not know it in advance
    let mut archive = open_plain(&archive_path)?;
    let size = archive.metadata().map_err(|e| io_failure(&e))?.len();
    if size == 0 || size > src.max_archive {
        return Err(if size == 0 { Failure::Truncated } else { Failure::TooBig });
    }
    if !pack::verdict(&digest_of(&mut archive)?, &src.archive_sha256) {
        return Err(Failure::HashMismatch);
    }

    // 3. the listing, judged by the pure part
    let mut list = Command::new(&tar);
    list.args(["-tvzf", "-"]);
    let (status, out, _) = run_tool(list, Some(&mut archive), pack::MAX_LISTING_BYTES)?;
    if !status.success() {
        return Err(Failure::Cannot("the archive could not be read by tar"));
    }
    pack::check_listing(&String::from_utf8_lossy(&out))?;

    // 4. the unpacking into a folder of the staging folder
    let payload = staging.0.join("payload");
    std::fs::DirBuilder::new().mode(0o700).create(&payload).map_err(|e| io_failure(&e))?;
    let mut extract = Command::new(&tar);
    extract.args(["-xzf", "-", "--no-same-owner", "--no-same-permissions", "-C"]).arg(&payload);
    let (status, _, err) = run_tool(extract, Some(&mut archive), 1024)?;
    if !status.success() {
        return Err(if err.contains("No space left") { Failure::NoSpace } else { Failure::Cannot("tar could not unpack the archive") });
    }

    // 5. the two files, hashed again through their own descriptors, and their modes
    for (name, pinned) in [(pack::ENGINE_FILE, &src.engine_sha256), (pack::WORKER_FILE, &src.worker_sha256)] {
        let mut file = open_plain(&payload.join(name))?;
        if !pack::verdict(&digest_of(&mut file)?, pinned) {
            return Err(Failure::HashMismatch);
        }
        if file.metadata().map_err(|e| io_failure(&e))?.uid() != unsafe { getuid() } {
            return Err(Failure::Cannot("the unpacked file is not this user's"));
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o700)).map_err(|e| io_failure(&e))?;
    }
    std::fs::set_permissions(&payload, std::fs::Permissions::from_mode(0o700)).map_err(|e| io_failure(&e))?;

    // 6. one atomic rename; the staging folder (with the archive) goes with the guard
    match std::fs::rename(&payload, &dest) {
        Ok(()) => Ok(()),
        Err(_) if dest.join(pack::ENGINE_FILE).exists() => Ok(()), // another process installed it first: the next start checks it anyway
        Err(e) => Err(io_failure(&e)),
    }
}

extern "C" {
    fn getuid() -> u32;
}

#[cfg(test)]
#[path = "installer_tests.rs"]
pub(super) mod tests;
