//! The remembered pages (M22-P05c): the address, title and time of the pages a browser tab has shown, newest first, at most `MAX_PAGES`, in the owner-only file `browser_pages.json`
//! of the host's state directory. The browser adapter writes it (a page that has loaded in a tab that is not local), reopens from it and clears it; the chats read it to list the
//! pages beside the conversations. A missing, damaged, group-readable or link file reads as an empty list and says nothing; the next page rewrites it whole, through a temporary
//! file and a rename, with mode 0600. A page is kept without the part of its address after `#`, and a page whose address carries a secret by name (`?token=`, `?code=`, `;jsessionid=`,
//! see `carries_secret`) is not kept at all (M23-P08): the file is read with the same rule, so what an older run wrote is hidden too.

use lico_browser_view::parse_address;
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::Mutex;

pub const FILE: &str = "browser_pages.json";
/// How many pages are kept: the newest.
pub const MAX_PAGES: usize = 100;
/// The longest file that is read, in bytes (a hundred pages of the longest address and title are well under it).
pub const MAX_FILE: u64 = 64 * 1024;
/// The longest title kept, in characters.
pub const MAX_TITLE: usize = 200;

const O_NOFOLLOW: i32 = 0o400_000;
const O_CLOEXEC: i32 = 0o2_000_000;

/// The read-modify-write of the file is one at a time in this program (the two engines, the normal one and the one for local tabs, deliver their pages from two threads).
static WRITING: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub address: String,
    /// The page's title; empty when it has none (the cards show the address then).
    pub title: String,
    /// When it was last shown, in seconds since the epoch.
    pub time: u64,
}

/// The words that make a query name a secret's: a name is split into words and one of them must be on this list (`keyword`, `author`, `encoding` and `zipcode` are not).
const SECRET_WORDS: [&str; 27] = [
    "token", "secret", "password", "passwd", "pwd", "pass", "passcode", "auth", "authorization", "session", "sessionid", "sessid", "sid", "sig", "signature", "key", "apikey", "code", "otp", "jwt",
    "ticket", "credential", "credentials", "bearer", "magic", "invite", "csrf",
];
/// A name written as one word (`JSESSIONID`, `accesstoken`) is a secret's when it ends with one of these.
const SECRET_ENDINGS: [&str; 7] = ["token", "secret", "password", "passwd", "apikey", "sessionid", "sessid"];

/// The address without its fragment (the part from the first `#`): a fragment never reaches the server, and an implicit sign-in flow puts its token there.
pub fn strip_fragment(address: &str) -> &str {
    address.find('#').map_or(address, |i| &address[..i])
}

/// `text` with its `%XX` escapes undone (a name written `to%6Ben` is `token`; a `%` with no two hex digits after it stays as it is); bytes that are not UTF-8 are read lossily.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
        match bytes[i] {
            b'%' if i + 2 < bytes.len() && hex(bytes[i + 1]).is_some() && hex(bytes[i + 2]).is_some() => {
                out.push(hex(bytes[i + 1]).unwrap_or(0) * 16 + hex(bytes[i + 2]).unwrap_or(0));
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The words of a name, in lower case: split at anything that is not a letter or a digit and between a small letter and a capital (`accessToken`, `x-amz-signature`, `api_key`).
fn words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in percent_decoded(name).chars() {
        let split = !c.is_alphanumeric() || (c.is_uppercase() && current.chars().last().is_some_and(char::is_lowercase));
        if split && !current.is_empty() {
            words.push(std::mem::take(&mut current).to_lowercase());
        }
        if c.is_alphanumeric() {
            current.push(c);
        }
    }
    if !current.is_empty() {
        words.push(current.to_lowercase());
    }
    words
}

fn is_secret_name(name: &str) -> bool {
    let words = words(name);
    words.iter().any(|w| SECRET_WORDS.contains(&w.as_str())) || {
        let one = words.concat();
        SECRET_ENDINGS.iter().any(|e| one.ends_with(e))
    }
}

/// Whether an address (with no fragment) carries a secret by name: in a query name, or in a path parameter (`/app;jsessionid=...`).
fn carries_secret(address: &str) -> bool {
    let (before, query) = address.split_once('?').unwrap_or((address, ""));
    let name = |piece: &str| piece.split('=').next().unwrap_or("").to_string();
    let in_path = before.split('/').skip(3).flat_map(|segment| segment.split(';').skip(1)).any(|param| is_secret_name(&name(param)));
    in_path || query.split(['&', ';']).any(|piece| is_secret_name(&name(piece)))
}

/// A page as it is kept, or why not. An explicit `http://` or `https://` address that `parse_address` takes (so no user name, no local form), else `bad_address`; the address without
/// its fragment (D1); a query or path parameter named like a secret is `secret_address` (D2); the title has the control characters and extra white space gone, cut at `MAX_TITLE`
/// characters.
fn clean(address: &str, title: &str, time: u64) -> Result<Page, &'static str> {
    let lower = address.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("bad_address");
    }
    let address = parse_address(address).map_err(|_| "bad_address")?;
    let address = strip_fragment(&address).to_string();
    if carries_secret(&address) {
        return Err("secret_address");
    }
    let spaced: String = title.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let title: String = spaced.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_TITLE).collect();
    Ok(Page { address, title, time })
}

fn parse(text: &str) -> Vec<Page> {
    let Ok(Value::Object(top)) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    if top.get("v").and_then(Value::as_u64) != Some(1) {
        return Vec::new();
    }
    let mut pages: Vec<Page> = Vec::new();
    for entry in top.get("pages").and_then(Value::as_array).into_iter().flatten() {
        let (Some(address), Some(title), Some(time)) = (entry.get("address").and_then(Value::as_str), entry.get("title").and_then(Value::as_str), entry.get("time").and_then(Value::as_u64)) else { continue };
        if let Ok(page) = clean(address, title, time) {
            if !pages.iter().any(|p| p.address == page.address) {
                pages.push(page);
            }
        }
    }
    pages.truncate(MAX_PAGES);
    pages
}

fn render(pages: &[Page]) -> String {
    let list: Vec<Value> = pages.iter().map(|p| json!({"address": p.address, "title": p.title, "time": p.time})).collect();
    json!({"v": 1, "pages": list}).to_string()
}

/// The remembered pages, newest first (as they are in the file: the file is kept in that order).
pub fn load(dir: &Path) -> Vec<Page> {
    let Ok(file) = OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_CLOEXEC).open(dir.join(FILE)) else { return Vec::new() };
    let Ok(m) = file.metadata() else { return Vec::new() };
    let mine = std::fs::metadata("/proc/self").is_ok_and(|me| me.uid() == m.uid());
    if !mine || m.permissions().mode() & 0o077 != 0 || m.len() > MAX_FILE {
        return Vec::new();
    }
    let mut text = String::new();
    if file.take(MAX_FILE).read_to_string(&mut text).is_err() {
        return Vec::new();
    }
    parse(&text)
}

fn save(dir: &Path, pages: &[Page]) -> std::io::Result<()> {
    let temporary = dir.join(format!(".browser_pages.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temporary);
    let mut out = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)?;
    let written = out.write_all(render(pages).as_bytes()).and_then(|()| out.sync_all());
    match written.and_then(|()| std::fs::rename(&temporary, dir.join(FILE))) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(e)
        }
    }
}

/// Remembers a page shown at `time`: it goes to the front (the same address renews its entry), the oldest beyond `MAX_PAGES` go. An address that is not an `http(s)` address is
/// refused (`bad_address`), one that carries a secret by name too (`secret_address`); a file that cannot be written is `not_saved`.
pub fn remember(dir: &Path, address: &str, title: &str, time: u64) -> Result<(), &'static str> {
    let page = clean(address, title, time)?;
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut pages = load(dir);
    pages.retain(|p| p.address != page.address);
    pages.insert(0, page);
    pages.truncate(MAX_PAGES);
    save(dir, &pages).map_err(|_| "not_saved")
}

/// Forgets every page (the file is removed, a damaged one too) and says how many there were.
pub fn forget(dir: &Path) -> usize {
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let count = load(dir).len();
    let _ = std::fs::remove_file(dir.join(FILE));
    count
}
