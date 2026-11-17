//! `lico-engine-pack` (SPEC.md): the pinned browser engine release as data and the pure decisions of installing it. Std only; it runs no process and touches no file.

pub const RELEASE: &str = "v0.2.6";
/// The only place the engine is fetched from: compiled in, https, reachable by no variable, file or argument.
pub const ARCHIVE_URL: &str = "https://github.com/h4ckf0r0day/obscura/releases/download/v0.2.6/obscura-x86_64-linux.tar.gz";
pub const ARCHIVE_SHA256: &str = "eed986e478c5ec5439f531f2d7fb6b0e36a356f1df7a6c44a8e8fd4f2486bd76";
pub const ENGINE_SHA256: &str = "c1400eec0b1572ea9b583b1544feafb0b25cea4e155b353dbba9c144d94ae6e3";
pub const WORKER_SHA256: &str = "d1025e540736b494d5c3fefe5abc09f3a774c65d9be891d818b0eb374d34dd72";
pub const ENGINE_FILE: &str = "obscura";
pub const WORKER_FILE: &str = "obscura-worker";
/// The size of the real archive (MEASURED in M17-P01).
pub const ARCHIVE_BYTES: u64 = 71_782_128;
/// The most an archive may be: a little over the real size, so a server that sends more is stopped.
pub const MAX_ARCHIVE_BYTES: u64 = 80 << 20;
/// The most one file of the archive may hold (the real engine is 80,583,768 bytes).
pub const MAX_FILE_BYTES: u64 = 120 << 20;
/// The most of a `tar -tv` listing that is read.
pub const MAX_LISTING_BYTES: usize = 16 << 10;
const MAX_ENTRIES: usize = 4;

/// The first twelve characters of a digest, as the tab shows them.
pub fn hash12(digest: &str) -> &str {
    digest.get(..12).unwrap_or(digest)
}

/// The size of the download in megabytes, rounded, as the tab says "about N MB".
pub fn size_mb() -> u32 {
    ((ARCHIVE_BYTES + 500_000) / 1_000_000) as u32
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
    /// The archive holds something other than the two named regular files (the reason).
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
    format!(
        "By hand: download {ARCHIVE_URL} (SHA-256 {ARCHIVE_SHA256}), put the files {ENGINE_FILE} and {WORKER_FILE} in ~/.local/share/lico/browser/{RELEASE} (every directory and file yours, mode 0700, not writable by others); {ENGINE_FILE} must be {ENGINE_SHA256} and {WORKER_FILE} {WORKER_SHA256}."
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

/// Checks the listing of `tar -tv` (GNU format, one entry a line): exactly the two named regular files (a `./` in front is fine, the archive's own `./` folder entry is fine), each
/// once, each within the size cap, no other entry of any kind. Returns the names to extract, in the order of the archive.
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
    let mut names: Vec<String> = Vec::new();
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
        match kind {
            'd' if name == "./" || name == "." => continue,
            'd' => return no("a folder inside the archive"),
            '-' => {}
            'l' | 'h' => return no("a link"),
            _ => return no("an entry that is not a regular file (a device or a fifo)"),
        }
        let bare = name.strip_prefix("./").unwrap_or(name);
        if bare != ENGINE_FILE && bare != WORKER_FILE {
            return no("a file that is not one of the two named");
        }
        if names.iter().any(|n| n == bare) {
            return no("a file twice");
        }
        match fields[2].parse::<u64>() {
            Ok(n) if n <= MAX_FILE_BYTES => {}
            Ok(_) => return no("a file over the size cap"),
            Err(_) => return no("a size that is not a number"),
        }
        names.push(bare.to_string());
    }
    if names.len() != 2 {
        return no("the engine or its worker is missing");
    }
    Ok(names)
}
