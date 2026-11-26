//! The time of each record of a session (`M24-P11a`).
//!
//! The log of a session (`.seg` files) holds no time, and the lines of the protocol are `[sequence, text]`. The time is kept beside it, as **runs**: a run is
//! `(first sequence, unix seconds)` and a new run begins only when the second differs from the last run's, so a burst of lines in one second is one run (one
//! comparison per record and 16 bytes per second of activity). `Stamps` is the list; `Book` is a list with its file, `<log folder>.times/<session>.times`,
//! beside the log folder and not in it, so a build that does not know the file still starts (it refuses a log folder that holds a file it does not know).
//!
//! A time is a gift, not a record of truth: a file that is missing, torn, unreadable or unwritable means "no time", it is never an error for the session.

mod approx;

pub use approx::{answer, Answer};

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// The most runs held (and the most the file holds): the oldest go first.
pub const MAX_RUNS: usize = 262_144;
/// The most runs one answer of the protocol carries.
pub const MAX_REPLY: usize = 2048;

const ENTRY: usize = 16;
const READ_LIMIT: usize = MAX_RUNS * ENTRY;
/// The file is rewritten without what the log no longer holds when it has more than twice what is kept (and this much more).
const SLACK: usize = 256;

// `open` flags of Linux; the value of O_NOFOLLOW differs between the two architectures this program is built for.
#[cfg(target_arch = "x86_64")]
const O_NOFOLLOW: i32 = 0o400000;
#[cfg(target_arch = "aarch64")]
const O_NOFOLLOW: i32 = 0o100000;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("the value of O_NOFOLLOW is not known for this architecture");

/// The runs of one session in the order of their sequence numbers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamps {
    runs: Vec<(i64, u64)>,
}

impl Stamps {
    pub fn new() -> Stamps {
        Stamps::default()
    }

    /// Notes that the record `seq` was written at `secs`. True when this began a run (the second differs from the last run's); a record that does not go forward is ignored.
    pub fn note(&mut self, seq: i64, secs: u64) -> bool {
        if seq < 0 || self.runs.last().is_some_and(|&(first, at)| seq <= first || at == secs) {
            return false;
        }
        if self.runs.len() >= MAX_RUNS {
            self.runs.remove(0);
        }
        self.runs.push((seq, secs));
        true
    }

    /// The second of the run that covers `seq`: the last run that began at or before it. None before the first run.
    pub fn at(&self, seq: i64) -> Option<u64> {
        match self.runs.partition_point(|run| run.0 <= seq) {
            0 => None,
            end => Some(self.runs[end - 1].1),
        }
    }

    /// The run that covers `from` (the last one that began at or before it, when there is one) and every run that begins before `until`, at most `limit`, oldest first.
    pub fn between(&self, from: i64, until: i64, limit: usize) -> Vec<(i64, u64)> {
        if until <= from {
            return Vec::new();
        }
        let start = self.runs.partition_point(|run| run.0 <= from).saturating_sub(1);
        self.runs[start..].iter().take_while(|run| run.0 < until).take(limit).copied().collect()
    }

    /// Forgets the runs that ended before `seq` (the log dropped those records); the run that covers `seq` stays.
    pub fn drop_before(&mut self, seq: i64) {
        let end = self.runs.partition_point(|run| run.0 <= seq);
        self.runs.drain(..end.saturating_sub(1));
    }

    /// Forgets the runs that begin at or after `seq` (records the log lost in a torn tail: the next record takes the number again). True when any went.
    pub fn drop_from(&mut self, seq: i64) -> bool {
        let keep = self.runs.partition_point(|run| run.0 < seq);
        let dropped = keep < self.runs.len();
        self.runs.truncate(keep);
        dropped
    }

    /// Takes in the runs an answer carried (the window's side): a run already known is replaced, a new one is put in its place; a sequence below zero is no record.
    pub fn merge(&mut self, runs: &[(i64, u64)]) {
        for &(seq, secs) in runs.iter().filter(|run| run.0 >= 0) {
            match self.runs.binary_search_by_key(&seq, |run| run.0) {
                Ok(at) => self.runs[at].1 = secs,
                Err(at) => self.runs.insert(at, (seq, secs)),
            }
        }
        let over = self.runs.len().saturating_sub(MAX_RUNS);
        self.runs.drain(..over);
    }

    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn last(&self) -> Option<(i64, u64)> {
        self.runs.last().copied()
    }

    /// The first run held: where the recorded times begin (the lines before it have none).
    pub fn first(&self) -> Option<(i64, u64)> {
        self.runs.first().copied()
    }
}

/// The folder of the files of the times of the sessions whose log is in `log_dir`: beside it, `<name>.times`.
pub fn dir_for(log_dir: &Path) -> PathBuf {
    match log_dir.file_name() {
        Some(name) => {
            let mut named = name.to_os_string();
            named.push(".times");
            log_dir.with_file_name(named)
        }
        None => log_dir.join(".times"),
    }
}

/// Removes the files of the sessions that are not in `keep` (the start of the host does this for the log too); a file that is not a `.times` file is not touched.
pub fn sweep(log_dir: &Path, keep: &HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir_for(log_dir)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.strip_suffix(".times").or_else(|| name.strip_suffix(".times.tmp")).is_some_and(|id| !keep.contains(id)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// An id that is safe as a file name: letters, digits, `-` and `_`.
fn safe(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The runs of the file, and whether the file is exactly them (no torn tail, no record that does not go forward, nothing past the limit: of a file that holds more, the newest are read).
/// None when it cannot be read.
fn read(path: &Path) -> Option<(Vec<(i64, u64)>, bool)> {
    let mut file = OpenOptions::new().read(true).custom_flags(O_NOFOLLOW).open(path).ok()?;
    let len = usize::try_from(file.metadata().ok()?.len()).ok()?;
    let skip = (len / ENTRY).saturating_sub(MAX_RUNS) * ENTRY; // whole records only: they start at multiples of 16 from the start of the file
    file.seek(SeekFrom::Start(skip as u64)).ok()?;
    let mut data = Vec::new();
    file.take(READ_LIMIT as u64).read_to_end(&mut data).ok()?;
    let mut clean = skip == 0 && len % ENTRY == 0;
    let mut runs: Vec<(i64, u64)> = Vec::new();
    for chunk in data.chunks_exact(ENTRY) {
        let seq = u64::from_le_bytes(chunk[..8].try_into().ok()?);
        let secs = u64::from_le_bytes(chunk[8..].try_into().ok()?);
        match i64::try_from(seq) {
            Ok(seq) if runs.last().is_none_or(|last| seq > last.0) => runs.push((seq, secs)),
            _ => clean = false,
        }
    }
    Some((runs, clean))
}

/// The runs of a session and the file that keeps them.
pub struct Book {
    id: String,
    stamps: Stamps,
    path: Option<PathBuf>, // None: the id is not safe as a file name, the runs stay in memory
    file: Option<File>,
    on_disk: usize, // records in the file
    broken: bool,   // a write failed: the file is let go (said once), the runs go on in memory
    purged: bool,
}

impl Book {
    /// Reads the file of the session `id` of the log folder `log_dir` when there is one (a file that is not clean is rewritten at once, so that the next record falls in its place).
    pub fn new(log_dir: &Path, id: &str) -> Book {
        let path = safe(id).then(|| dir_for(log_dir).join(format!("{id}.times")));
        let mut book = Book { id: id.to_string(), stamps: Stamps::new(), path, file: None, on_disk: 0, broken: false, purged: false };
        if let Some((runs, clean)) = book.path.as_deref().and_then(read) {
            book.on_disk = runs.len();
            book.stamps.merge(&runs);
            if !clean {
                book.rewrite();
            }
        }
        book
    }

    pub fn stamps(&self) -> &Stamps {
        &self.stamps
    }

    /// Notes the record `seq` written at `secs`; a run that begins is appended to the file.
    pub fn note(&mut self, seq: i64, secs: u64) {
        if self.purged || !self.stamps.note(seq, secs) {
            return;
        }
        let record = [seq.to_le_bytes(), secs.to_le_bytes()].concat();
        self.append(&record);
        self.on_disk += 1; // (a file that cannot be written is let go: the count then no longer matters)
    }

    /// The log dropped the records before `seq`: forgets their runs, and rewrites the file when it holds more than twice what is kept.
    pub fn drop_before(&mut self, seq: i64) {
        self.stamps.drop_before(seq);
        if self.on_disk > 2 * self.stamps.len() + SLACK {
            self.rewrite();
        }
    }

    /// The log holds no record from `seq` on (a torn tail was cut off at a restart): forgets their runs, in the file too.
    pub fn drop_from(&mut self, seq: i64) {
        if self.stamps.drop_from(seq) {
            self.rewrite();
        }
    }

    /// The session was deleted: its file goes, and nothing is written again.
    pub fn purge(&mut self) {
        self.file = None;
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(path.with_extension("times.tmp"));
        }
        self.stamps = Stamps::new();
        self.on_disk = 0;
        self.purged = true;
    }

    fn fail(&mut self, error: &std::io::Error) {
        if !self.broken {
            eprintln!("lico-host: the times of session {} cannot be kept in a file ({:?}); they stay in memory", self.id, error.kind());
        }
        self.broken = true;
        self.file = None;
    }

    fn append(&mut self, record: &[u8]) {
        let Some(path) = self.path.clone().filter(|_| !self.broken) else { return };
        if self.file.is_none() {
            let opened = path
                .parent()
                .map_or(Ok(()), |dir| std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir))
                .and_then(|()| OpenOptions::new().create(true).append(true).mode(0o600).custom_flags(O_NOFOLLOW).open(&path));
            match opened {
                Ok(file) => self.file = Some(file),
                Err(error) => return self.fail(&error),
            }
        }
        if let Some(Err(error)) = self.file.as_mut().map(|file| file.write_all(record)) {
            self.fail(&error);
        }
    }

    /// Writes the runs that are kept to a new file and puts it in place of the old one.
    fn rewrite(&mut self) {
        let Some(path) = self.path.clone().filter(|_| !self.broken) else { return };
        self.file = None;
        let bytes: Vec<u8> = self.stamps.runs.iter().flat_map(|&(seq, secs)| [seq.to_le_bytes(), secs.to_le_bytes()].concat()).collect();
        let temp = path.with_extension("times.tmp");
        let _ = std::fs::remove_file(&temp); // a stale one, or a link put in its place, is taken away, never followed or written through
        let done = path
            .parent()
            .map_or(Ok(()), |dir| std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir))
            .and_then(|()| OpenOptions::new().create_new(true).write(true).mode(0o600).open(&temp))
            .and_then(|mut file| file.write_all(&bytes))
            .and_then(|()| std::fs::rename(&temp, &path));
        match done {
            Ok(()) => self.on_disk = self.stamps.len(),
            Err(error) => {
                let _ = std::fs::remove_file(&temp);
                self.fail(&error);
            }
        }
    }
}

#[cfg(test)]
mod tests;
