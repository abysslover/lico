//! Archives for the Directories menu (`M24-P10c`): what is an archive by its name, a free name beside it, the vetting of what an archive holds before anything is written, and the two runs (`compress_within`, `extract_within`) that call the system's own `zip`, `unzip` and `tar`.
//! Standard library only, no window. An archive is somebody else's file: its members are listed and vetted first (no name that starts at the root or goes up, no link, device or pipe, no more than 100,000 items or 8 GiB), the tools run with a list of arguments (no shell), a cleared environment and a time limit, and what a run made is removed when it fails.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The most items an archive may hold, and the most bytes its items may add up to, for the window to open it.
pub(crate) const MAX_MEMBERS: usize = 100_000;
pub(crate) const MAX_TOTAL: u64 = 8 << 30;
/// How long one archive may take before it is stopped and what it made is removed.
pub(crate) const RUN_LIMIT: Duration = Duration::from_secs(30 * 60);
/// How much of a tool's error text is read, and how much of a listing (the rest of a longer one is a refusal).
const ERR_LIMIT: usize = 4096;
const LISTING_LIMIT: usize = 64 << 20;
/// The only places the tools are looked for and run from.
const PATH: &str = "/usr/local/bin:/usr/bin:/bin";
const LINE_BREAK: &str = "a name has a line break (the list of the archive cannot be trusted)";
const UNREADABLE: &str = "the list of the archive could not be read";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Format {
    Zip,
    Tar,
}

/// One item of an archive as its listing says: the kind letter (`-` file, `d` folder, `l` and `h` links, `?` no kind given, others devices), the size and the name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Member {
    pub kind: char,
    pub size: u64,
    pub name: String,
}

/// What a run answered: whether it ended well, what it wrote (up to what was asked) and the first 4 KiB of its error text.
#[derive(Debug)]
pub(crate) struct Ran {
    pub ok: bool,
    pub out: Vec<u8>,
    pub err: String,
}

/// The format of an archive by the ending of its name (in any case) and the length of that ending; None when the name is not one (a name that is only an ending is a hidden file, not an archive).
pub(crate) fn kind_of(name: &str) -> Option<(Format, usize)> {
    const ENDINGS: [(&str, Format); 8] = [(".tar.gz", Format::Tar), (".tgz", Format::Tar), (".tar.bz2", Format::Tar), (".tbz2", Format::Tar), (".tar.xz", Format::Tar), (".txz", Format::Tar), (".zip", Format::Zip), (".tar", Format::Tar)];
    let lower = name.to_ascii_lowercase();
    ENDINGS.iter().find(|(end, _)| lower.ends_with(end) && lower.len() > end.len()).map(|(end, format)| (*format, end.len()))
}

/// The name of an archive without its ending.
pub(crate) fn stem_of(name: &str) -> Option<&str> {
    kind_of(name).map(|(_, ending)| &name[..name.len() - ending])
}

/// The first name beside `parent`'s items that nothing holds: `wanted` + `ending`, then `wanted 2` + `ending`, and so on (a link that points nowhere holds its name). None after a thousand.
pub(crate) fn free_name(parent: &Path, wanted: &str, ending: &str) -> Option<String> {
    (1..=1000).map(|n| if n == 1 { format!("{wanted}{ending}") } else { format!("{wanted} {n}{ending}") }).find(|name| std::fs::symlink_metadata(parent.join(name)).is_err())
}

/// A name for the person to read: cut short, with a control character shown as `?`.
fn shown(name: &str) -> String {
    name.chars().take(60).map(|c| if c.is_control() { '?' } else { c }).collect()
}

/// Why the window will not open an archive that holds `members`, in words for the person; None when it holds nothing it should not. Nothing has been written when this answers.
pub(crate) fn member_problem(members: &[Member]) -> Option<String> {
    if members.len() > MAX_MEMBERS {
        return Some("it holds more than 100,000 items".into());
    }
    for member in members {
        let name = member.name.as_str();
        match member.kind {
            '-' | 'd' | '?' => {}
            'l' | 'h' => return Some(format!("it holds a link ({})", shown(name))),
            _ => return Some(format!("it holds a device or a pipe ({})", shown(name))),
        }
        if name.is_empty() {
            return Some("an item has no name".into());
        }
        if name.starts_with('/') {
            return Some(format!("a name starts at the root of the disk ({})", shown(name)));
        }
        if name.split('/').any(|part| part == "..") {
            return Some(format!("a name goes up out of its folder ({})", shown(name)));
        }
        if name.chars().any(char::is_control) {
            return Some(format!("a name has a line break or a control character ({})", shown(name)));
        }
    }
    let total = members.iter().fold(0u64, |sum, member| sum.saturating_add(member.size));
    (total > MAX_TOTAL).then(|| "it would write more than 8 GiB".to_string())
}

/// The next word of a listing line (the spaces before it skipped) and what follows its one closing space.
fn word(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start_matches(' ');
    let end = line.find(' ')?;
    Some((&line[..end], &line[end + 1..]))
}

/// One item line of `unzip -Z`: the first word is the kind and the rights, then the version, the system, the size, two flags, the method, the date and the time; the name is all that follows. None when the line has not that shape.
fn zip_member(line: &str) -> Option<Member> {
    let (rights, rest) = word(line)?;
    let kind = rights.chars().next().filter(|c| "-?dlbcpsDh".contains(*c))?;
    let (version, rest) = word(rest)?;
    let (major, minor) = version.split_once('.')?;
    if major.is_empty() || minor.is_empty() || !major.chars().chain(minor.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (_system, rest) = word(rest)?;
    let (size, rest) = word(rest)?;
    let size = size.parse::<u64>().ok()?;
    let (_flags, rest) = word(rest)?;
    let (_method, rest) = word(rest)?;
    let (_date, rest) = word(rest)?;
    let (time, name) = word(rest)?;
    time.contains(':').then(|| Member { kind, size, name: name.to_string() })
}

/// A line like `6 files, 17 bytes uncompressed, ...` or `1 file, ...`: the last line of `unzip -Z`.
fn zip_footer(line: &str) -> bool {
    word(line).is_some_and(|(count, rest)| !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()) && rest.starts_with("file"))
}

/// The items of an archive from the output of `unzip -Z`. A name with a line break makes lines that are no item (or one too many): the whole listing is then refused, because what it shows cannot be trusted. The first line, the second (with the number of items) and the last must be where they are.
pub(crate) fn zip_listing(text: &str) -> Result<Vec<Member>, String> {
    let lines: Vec<&str> = text.split_terminator('\n').collect();
    if lines.len() == 2 && lines[0].starts_with("Archive:") && lines[1].starts_with("Empty zip file") {
        return Ok(Vec::new());
    }
    if lines.len() < 3 || !lines[0].starts_with("Archive:") || !zip_footer(lines[lines.len() - 1]) {
        return Err(UNREADABLE.into());
    }
    let held = lines[1].strip_prefix("Zip file size: ").and_then(|l| l.rsplit_once("number of entries: ")).and_then(|(_, n)| n.trim().parse::<usize>().ok()).ok_or(UNREADABLE)?;
    let body = &lines[2..lines.len() - 1];
    if body.len() != held {
        return Err(LINE_BREAK.into());
    }
    body.iter().map(|line| zip_member(line).ok_or_else(|| LINE_BREAK.to_string())).collect()
}

/// What `tar --quoting-style=escape` wrote for a name, as the name is: `\n` and the like, `\\`, and `\ooo` (the bytes of a name in a language the listing could not show) are undone.
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'\\' || at + 1 >= bytes.len() {
            out.push(bytes[at]);
            at += 1;
            continue;
        }
        let next = bytes[at + 1];
        let plain = match next {
            b'a' => Some(7),
            b'b' => Some(8),
            b'f' => Some(12),
            b'n' => Some(10),
            b'r' => Some(13),
            b't' => Some(9),
            b'v' => Some(11),
            b'\\' => Some(b'\\'),
            _ => None,
        };
        if let Some(byte) = plain {
            out.push(byte);
            at += 2;
        } else if at + 4 <= bytes.len() && bytes[at + 1..at + 4].iter().all(|b| (b'0'..=b'7').contains(b)) {
            out.push(bytes[at + 1..at + 4].iter().fold(0u32, |v, b| v * 8 + u32::from(b - b'0')) as u8);
            at += 4;
        } else {
            out.push(b'\\');
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One line of `tar --list --verbose --numeric-owner --quoting-style=escape`: the rights (the first letter is the kind), `uid/gid` as numbers (so that no name of an owner can stand where the date is), the size (two numbers for a device), the date, the time and the name.
fn tar_member(line: &str) -> Result<Member, String> {
    let bad = || UNREADABLE.to_string();
    let (rights, rest) = word(line).ok_or_else(bad)?;
    let (ids, rest) = word(rest).ok_or_else(bad)?;
    let (size, rest) = word(rest).ok_or_else(bad)?;
    let (date, rest) = word(rest).ok_or_else(bad)?;
    let (time, name) = word(rest).ok_or_else(bad)?;
    let kind = rights.chars().next().filter(|_| rights.len() >= 10).ok_or_else(bad)?;
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let date_ok = date.len() == 10 && date.char_indices().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() });
    let numbers: Vec<&str> = size.split(',').collect();
    let size_ok = (numbers.len() == 1 || numbers.len() == 2) && numbers.iter().all(|n| digits(n));
    let ids_ok = ids.split_once('/').is_some_and(|(uid, gid)| digits(uid) && digits(gid));
    if !(date_ok && size_ok && ids_ok && time.contains(':') && time.chars().all(|c| c.is_ascii_digit() || c == ':')) {
        return Err(bad());
    }
    Ok(Member { kind, size: if numbers.len() == 1 { numbers[0].parse().unwrap_or(u64::MAX) } else { 0 }, name: unescape(name) })
}

/// The items of an archive from the output of `tar --list --verbose --numeric-owner --quoting-style=escape`: one line each.
pub(crate) fn tar_listing(text: &str) -> Result<Vec<Member>, String> {
    text.split_terminator('\n').map(tar_member).collect()
}

/// The full path of a tool in the short list of places the window runs tools from; None when it is not there or the name is not a plain name.
pub(crate) fn find_tool(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if name.is_empty() || name.contains('/') || name.contains('\0') {
        return None;
    }
    PATH.split(':').map(|dir| Path::new(dir).join(name)).find(|path| std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// Why an archive of this format cannot be opened on this machine (the tool is not installed); None when it can.
pub(crate) fn tool_missing(format: Format) -> Option<&'static str> {
    match format {
        Format::Zip => find_tool("unzip").is_none().then_some("unzip is not installed"),
        Format::Tar => find_tool("tar").is_none().then_some("tar is not installed"),
    }
}

pub(crate) fn zip_missing() -> bool {
    find_tool("zip").is_none()
}

/// The language the tools run in: the person's own when it is a UTF-8 one, else `C.UTF-8`, so that a name in Korean is kept as it is.
pub(crate) fn lang_from(var: Option<&str>) -> String {
    match var {
        Some(lang) if lang.to_ascii_lowercase().contains("utf-8") || lang.to_ascii_lowercase().contains("utf8") => lang.to_string(),
        _ => "C.UTF-8".into(),
    }
}

/// What a pipe says, up to `keep` bytes. A pipe that is cut short at `keep` is dropped when `stop` is set (the tool is then ended by the closed pipe); otherwise the rest is read and thrown away so that the tool never waits on a full pipe.
fn read_cut(mut pipe: impl Read, keep: usize, stop: bool) -> (Vec<u8>, bool) {
    let (mut kept, mut cut, mut chunk) = (Vec::new(), false, [0u8; 8192]);
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) | Err(_) => return (kept, cut),
            Ok(n) => {
                let room = keep.saturating_sub(kept.len()).min(n);
                kept.extend_from_slice(&chunk[..room]);
                if room < n {
                    cut = true;
                    if stop {
                        return (kept, cut);
                    }
                }
            }
        }
    }
}

/// Runs a tool in `cwd` with no shell, a cleared environment (the short `PATH` and a UTF-8 language) and no input, and waits for it for `limit`. What it writes is kept up to `keep_out` bytes (0: thrown away; an answer longer than that is an error); its error text is kept up to 4 KiB. A run past the limit is killed.
pub(crate) fn run(mut cmd: Command, cwd: &Path, limit: Duration, keep_out: usize) -> Result<Ran, String> {
    cmd.current_dir(cwd).env_clear().env("PATH", PATH).env("LANG", lang_from(std::env::var("LANG").ok().as_deref())).stdin(Stdio::null());
    cmd.stdout(if keep_out > 0 { Stdio::piped() } else { Stdio::null() }).stderr(Stdio::piped());
    let began = Instant::now();
    let mut child = cmd.spawn().map_err(|e| format!("the tool could not be started ({})", e.kind()))?;
    let out = child.stdout.take().map(|pipe| std::thread::spawn(move || read_cut(pipe, keep_out, true)));
    let err = child.stderr.take().map(|pipe| std::thread::spawn(move || read_cut(pipe, ERR_LIMIT, false)));
    let status = loop {
        if began.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err("it ran past its time limit and was stopped".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("the tool could not be waited for ({})", e.kind())),
        }
    };
    let (out, cut) = out.and_then(|t| t.join().ok()).unwrap_or_default();
    let (err, _) = err.and_then(|t| t.join().ok()).unwrap_or_default();
    if cut {
        return Err("the answer of the tool is too long".into());
    }
    Ok(Ran { ok: status.success(), out, err: String::from_utf8_lossy(&err).into_owned() })
}

/// What a tool that failed said, in one line for the person.
fn said(tool: &str, ran: &Ran) -> String {
    let text = if ran.err.trim().is_empty() { String::from_utf8_lossy(&ran.out[..ran.out.len().min(ERR_LIMIT)]).into_owned() } else { ran.err.clone() };
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
    if line.is_empty() {
        format!("{tool} failed")
    } else {
        format!("{tool} said: {line}")
    }
}

/// Makes `name.zip` beside `name` in `parent` (the first free name, nothing is replaced) with the system's `zip`, which stores a link as a link and never follows it. The work is done in a hidden folder beside it and moved into place at the end, so that nothing partial is ever left in `parent`: a failed or stopped run leaves nothing. The name of the new archive, or why not.
pub(crate) fn compress_within(parent: &Path, name: &str, limit: Duration) -> Result<String, String> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err("that is not a name".into());
    }
    let zip = find_tool("zip").ok_or("zip is not installed")?;
    if std::fs::symlink_metadata(parent.join(name)).is_err() {
        return Err("it is gone".into());
    }
    let made = free_name(parent, name, ".zip").ok_or("there are too many archives of that name")?;
    let part = parent.join(format!(".lico-part-{}", crate::core::random_hex(6)));
    std::fs::create_dir(&part).map_err(|e| if e.kind() == std::io::ErrorKind::PermissionDenied { "no permission to make an archive here".to_string() } else { format!("a place to work in could not be made ({})", e.kind()) })?;
    let mut cmd = Command::new(zip);
    cmd.args(["-r", "-y", "-q"]).arg(part.join(&made)).arg(format!("./{name}"));
    let done = match run(cmd, parent, limit, 0) {
        Ok(ran) if ran.ok => place(&part.join(&made), &parent.join(&made)),
        Ok(ran) => Err(said("zip", &ran)),
        Err(why) => Err(why),
    };
    let _ = std::fs::remove_dir_all(&part);
    done.map(|()| made)
}

/// Puts the finished `from` at `to` and never over something: a hard link fails when the name is taken; on a disk that has none (a USB stick) the move is made after a look.
fn place(from: &Path, to: &Path) -> Result<(), String> {
    match std::fs::hard_link(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err("that name is taken".into()),
        Err(_) if std::fs::symlink_metadata(to).is_ok() => Err("that name is taken".into()),
        Err(_) => std::fs::rename(from, to).map_err(|e| format!("the archive could not be put in place ({})", e.kind())),
    }
}

/// Opens `archive` (an absolute path to a file whose name ends like an archive) into a new folder beside it named without the ending (the first free name; nothing is replaced). The members are listed and vetted first: an archive that holds what `member_problem` refuses is refused and nothing is written. A failure or a stop removes the folder that was made. The new folder, or why not.
pub(crate) fn extract_within(archive: &Path, limit: Duration) -> Result<PathBuf, String> {
    let began = Instant::now();
    if !archive.is_absolute() {
        return Err("it needs a full path".into());
    }
    let (Some(parent), Some(name)) = (archive.parent(), archive.file_name().and_then(|n| n.to_str())) else { return Err("the name of the archive cannot be read".into()) };
    let (format, _) = kind_of(name).ok_or("not an archive")?;
    let stem = stem_of(name).ok_or("not an archive")?;
    match std::fs::metadata(archive) {
        Err(_) => return Err("it is gone".into()),
        Ok(meta) if !meta.is_file() => return Err("it is not a file".into()),
        Ok(_) => {}
    }
    if let Some(why) = tool_missing(format) {
        return Err(why.into());
    }
    let (tool, program) = if format == Format::Zip { ("unzip", find_tool("unzip")) } else { ("tar", find_tool("tar")) };
    let program = program.ok_or_else(|| format!("{tool} is not installed"))?;
    let left = || limit.saturating_sub(began.elapsed());
    let listing = match format {
        Format::Zip => {
            let mut cmd = Command::new(&program);
            cmd.arg("-Z").arg(archive);
            let ran = run(cmd, parent, left(), LISTING_LIMIT)?;
            if !ran.ok {
                return Err(said("unzip", &ran));
            }
            zip_listing(&String::from_utf8_lossy(&ran.out))?
        }
        Format::Tar => {
            let mut cmd = Command::new(&program);
            cmd.args(["--list", "--verbose", "--numeric-owner", "--quoting-style=escape", "--file"]).arg(archive);
            let ran = run(cmd, parent, left(), LISTING_LIMIT)?;
            if !ran.ok {
                return Err(said("tar", &ran));
            }
            tar_listing(&String::from_utf8_lossy(&ran.out))?
        }
    };
    if let Some(why) = member_problem(&listing) {
        return Err(why);
    }
    let dest = parent.join(free_name(parent, stem, "").ok_or("there are too many folders of that name")?);
    std::fs::create_dir(&dest).map_err(|e| if e.kind() == std::io::ErrorKind::PermissionDenied { "no permission to extract here".to_string() } else { format!("the folder could not be made ({})", e.kind()) })?;
    let mut cmd = Command::new(&program);
    match format {
        Format::Zip => cmd.args(["-q", "-n", "-d"]).arg(&dest).arg(archive),
        Format::Tar => cmd.args(["--extract", "--no-same-owner", "--file"]).arg(archive).arg("--directory").arg(&dest),
    };
    match run(cmd, &dest, left(), 0) {
        Ok(ran) if ran.ok => Ok(dest),
        other => {
            let _ = std::fs::remove_dir_all(&dest);
            Err(match other {
                Ok(ran) => said(tool, &ran),
                Err(why) => why,
            })
        }
    }
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
