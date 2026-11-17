//! `lico-engine` (SPEC.md): the browser engine as one record that Lico versions itself (the upstream it is built on, Lico's own version and contract number, the ledger of Lico's patches) and the pure decisions of installing it. Std only; it runs no process and touches no file.

/// The upstream project the engine is, and its release (the pin).
pub const UPSTREAM: &str = "lexmount/moli";
pub const UPSTREAM_NAME: &str = "Moli";
pub const RELEASE: &str = "v1.1.25";
/// The commit of the tag `RELEASE` in the upstream (MEASURED: the tag was cloned once, M20).
pub const SOURCE_COMMIT: &str = "080c3fc2cbbb12207512925afb3feb0db38dfcab";
pub const LICENCE: &str = "MIT OR Apache-2.0";

/// Lico's own version of this record. It moves with the upstream release and with the ledger, never alone: a test pins the three together.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The number of the ENGINE CONTRACT this record satisfies: how the host starts the engine, finds its socket and takes its frames (SPEC.md). The host speaks one number and refuses a record that names another.
pub const CONTRACT: u32 = 1;
/// Lico's patches on the upstream, one line each. Empty: the pinned file is the upstream's own, byte for byte.
pub const LEDGER: &[&str] = &[];

/// The only place the engine is fetched from: compiled in, https, reachable by no variable, file or argument.
pub const ARCHIVE_URL: &str = "https://github.com/lexmount/moli/releases/download/v1.1.25/moli-x86_64-unknown-linux-gnu.tar.gz";
pub const ARCHIVE_SHA256: &str = "04c73204626054c9ec0064f7fc00abb0dcfa7f8b9c3ec9df5de8e2afc301087f";
pub const ENGINE_SHA256: &str = "e452e1166afbf69bd4acf78f7c012cd986f6cb467c80f4565db9bb8d10e42a25";
/// The folder the archive holds everything in, and the one member Lico extracts from it.
pub const ARCHIVE_ROOT: &str = "moli-v1.1.25-x86_64-unknown-linux-gnu";
pub const MEMBER: &str = "moli-v1.1.25-x86_64-unknown-linux-gnu/moli";
/// The name of the engine file once installed.
pub const ENGINE_FILE: &str = "moli";
/// The sizes of the real archive and of the engine in it (MEASURED in M20).
pub const ARCHIVE_BYTES: u64 = 45_655_260;
pub const ENGINE_BYTES: u64 = 112_357_968;
/// The most an archive may be: a little over the real size, so a server that sends more is stopped.
pub const MAX_ARCHIVE_BYTES: u64 = 64 << 20;
/// The most one file of the archive may hold.
pub const MAX_FILE_BYTES: u64 = 128 << 20;
/// The most of a `tar -tv` listing that is read.
pub const MAX_LISTING_BYTES: usize = 16 << 10;
pub const MAX_ENTRIES: usize = 32;

/// The first twelve characters of a digest, as the tab shows them.
pub fn hash12(digest: &str) -> &str {
    digest.get(..12).unwrap_or(digest)
}

/// The size of the download in megabytes, rounded, as the tab says "about N MB".
pub fn size_mb() -> u32 {
    ((ARCHIVE_BYTES + 500_000) / 1_000_000) as u32
}

/// The folder of the install under `lico/browser/` in the data folder: one per upstream release.
pub fn dir_name() -> String {
    format!("moli-{RELEASE}")
}

/// What the tab and the list say the engine is: the upstream, its release, Lico's version and contract of the record, and how many patches of Lico's are on it.
pub fn label() -> String {
    let patches = if LEDGER.is_empty() { String::new() } else { format!(", {} Lico patches", LEDGER.len()) };
    format!("{UPSTREAM_NAME} {RELEASE} (lico-engine {ENGINE_VERSION}, contract {CONTRACT}{patches})")
}

/// Whether `actual` is the pinned digest: lowercase hexadecimal, 64 characters, equal in every one; an empty digest never matches.
pub fn verdict(actual: &str, pinned: &str) -> bool {
    !actual.is_empty() && actual.len() == pinned.len() && actual.bytes().zip(pinned.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    CurlMissing,
    TarMissing,
    Network,
    Tls,
    Timeout,
    TooBig,
    NoSpace,
    Truncated,
    HashMismatch,
    /// The archive holds something other than the engine in its folder (the reason).
    Entry(String),
    /// Lico could not do what it needs to install safely (for example the fence).
    Cannot(&'static str),
    Other(String),
}

impl Failure {
    pub fn text(&self) -> String {
        match self {
            Failure::CurlMissing => "the program curl was not found on this machine, so the engine could not be fetched".into(),
            Failure::TarMissing => "the program tar was not found on this machine, so the engine could not be unpacked".into(),
            Failure::Network => "the release page could not be reached (no network, or the name did not resolve)".into(),
            Failure::Tls => "a secure connection to the release page could not be made".into(),
            Failure::Timeout => "the download took too long and was stopped".into(),
            Failure::TooBig => "the file on the release page is larger than the engine's pinned size allows, and was refused".into(),
            Failure::NoSpace => "there is no room to write the engine here".into(),
            Failure::Truncated => "the download ended before the whole file came".into(),
            Failure::HashMismatch => "the downloaded file is not the pinned release (its SHA-256 differs): it was removed and nothing was installed".into(),
            Failure::Entry(why) => format!("the archive holds something that is not the engine ({why}): it was removed and nothing was installed"),
            Failure::Cannot(why) => format!("the engine could not be installed safely here ({why})"),
            Failure::Other(why) => format!("the engine could not be installed ({why})"),
        }
    }

    /// A failure that says what came over the network is not what was pinned: shown as a security notice.
    pub fn security(&self) -> bool {
        matches!(self, Failure::HashMismatch | Failure::Entry(_))
    }
}

/// The steps for the person to install the engine by hand (the fallback of every failure).
pub fn manual_steps() -> String {
    let dir = dir_name();
    format!(
        "By hand: download {ARCHIVE_URL} (SHA-256 {ARCHIVE_SHA256}), take the file {MEMBER} out of it and put it as {ENGINE_FILE} in ~/.local/share/lico/browser/{dir} (every directory and file yours, mode 0700, not writable by others); {ENGINE_FILE} must be {ENGINE_SHA256}."
    )
}

/// What curl's exit code means for the install.
pub fn failure_of_curl_exit(code: i32) -> Failure {
    match code {
        6 | 7 => Failure::Network,
        28 => Failure::Timeout,
        35 | 51 | 58 | 59 | 60 | 77 | 82 | 83 | 90 | 91 => Failure::Tls,
        63 => Failure::TooBig,
        23 => Failure::NoSpace,
        18 | 56 | 52 => Failure::Truncated,
        22 => Failure::Other("the release page answered with an error".into()),
        other => Failure::Other(format!("curl ended with code {other}")),
    }
}

/// Checks the listing of `tar -tv` (GNU format, one entry a line) of the archive: every entry sits in the archive's root folder (a `./` in front is fine; the root folder's own entry is fine, and so is an archive that lists no folders),
/// the only entries are folders and regular files, each file is within the size cap, and the engine's member is there exactly once. No link of any kind, no device, no `..`, no absolute path, no name with a space, a bounded
/// number of entries. Returns the names to extract: the engine's member only (the rest of the archive is judged, never written).
pub fn check_listing(listing: &str) -> Result<Vec<String>, Failure> {
    let no = |why: &str| Err(Failure::Entry(why.to_string()));
    if listing.len() > MAX_LISTING_BYTES {
        return no("a listing that is too long");
    }
    if listing.contains('\0') {
        return no("a NUL in the listing");
    }
    let lines: Vec<&str> = listing.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() || lines.len() > MAX_ENTRIES {
        return no("an empty archive or too many entries");
    }
    let below_root = |bare: &str| bare.strip_prefix(ARCHIVE_ROOT).is_some_and(|rest| rest.starts_with('/'));
    let mut member = 0usize;
    for line in lines {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 6 {
            return no("an entry whose line is not the usual six fields (a name with a space, or a link)");
        }
        let kind = line.chars().next().unwrap_or('?');
        let name = fields[5];
        if name.starts_with('/') || name.split('/').any(|p| p == "..") {
            return no("a path that is absolute or has ..");
        }
        let bare = name.strip_prefix("./").unwrap_or(name);
        match kind {
            'd' if name == "./" || name == "." || bare.trim_end_matches('/') == ARCHIVE_ROOT || below_root(bare) => continue,
            'd' => return no("a folder that is not inside the archive's own"),
            '-' => {}
            'l' | 'h' => return no("a link"),
            _ => return no("an entry that is not a regular file (a device or a fifo)"),
        }
        if !below_root(bare) {
            return no("a file outside the archive's own folder");
        }
        match fields[2].parse::<u64>() {
            Ok(n) if n <= MAX_FILE_BYTES => {}
            Ok(_) => return no("a file over the size cap"),
            Err(_) => return no("a size that is not a number"),
        }
        if bare == MEMBER {
            member += 1;
        }
    }
    match member {
        1 => Ok(vec![MEMBER.to_string()]),
        0 => no("the engine is missing"),
        _ => no("the engine twice"),
    }
}
