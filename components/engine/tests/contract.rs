//! The contract tests of `lico-engine` (SPEC.md).

use lico_engine::*;

const REAL_LISTING: &str = include_str!("fixtures/moli_v1_1_25_listing.txt");
const ROOT_DIR: &str = "drwxr-xr-x root/root 0 2026-11-15 18:29 moli-v1.1.25-x86_64-unknown-linux-gnu/\n";
const MEMBER_LINE: &str = "-rwxr-xr-x root/root 112357968 2026-11-15 18:29 moli-v1.1.25-x86_64-unknown-linux-gnu/moli\n";

#[test]
fn the_pins_are_the_release_that_was_measured_and_the_url_is_https() {
    assert_eq!((UPSTREAM, RELEASE, LICENCE), ("lexmount/moli", "v1.1.25", "MIT OR Apache-2.0"));
    assert!(ARCHIVE_URL.starts_with("https://github.com/lexmount/moli/releases/download/v1.1.25/") && ARCHIVE_URL.ends_with("/moli-x86_64-unknown-linux-gnu.tar.gz"), "{ARCHIVE_URL}");
    assert!(!ARCHIVE_URL.contains("http://") && !ARCHIVE_URL.contains(' ') && !ARCHIVE_URL.contains('@'));
    for h in [ARCHIVE_SHA256, ENGINE_SHA256] {
        assert!(h.len() == 64 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')), "{h}");
    }
    assert!(SOURCE_COMMIT.len() == 40 && SOURCE_COMMIT.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')), "{SOURCE_COMMIT}");
    assert_ne!(ARCHIVE_SHA256, ENGINE_SHA256, "the archive and the file in it are two different things");
    assert_eq!(ENGINE_FILE, "moli");
    assert_eq!(ARCHIVE_ROOT, "moli-v1.1.25-x86_64-unknown-linux-gnu");
    assert_eq!(MEMBER, "moli-v1.1.25-x86_64-unknown-linux-gnu/moli");
    assert_eq!((ARCHIVE_BYTES, ENGINE_BYTES), (45_655_260, 112_357_968));
    assert!(MAX_ARCHIVE_BYTES >= ARCHIVE_BYTES && MAX_ARCHIVE_BYTES <= 2 * ARCHIVE_BYTES, "a cap near the real size, not a blank cheque");
    assert!(MAX_FILE_BYTES >= ENGINE_BYTES && MAX_FILE_BYTES <= 2 * ENGINE_BYTES);
    assert_eq!(hash12(ARCHIVE_SHA256), "04c732046260");
    assert_eq!(size_mb(), 46);
    assert_eq!(dir_name(), "moli-v1.1.25");
}

#[test]
fn the_engine_is_one_record_with_lico_own_version_contract_and_ledger() {
    // A patch of Lico's on the upstream, or another upstream release, is a new version of this record: these four move together.
    assert_eq!((ENGINE_VERSION, CONTRACT, LEDGER.len()), ("0.1.0", 1, 0));
    let label = label();
    assert!(label.contains("Moli") && label.contains(RELEASE) && label.contains("lico-engine 0.1.0") && label.contains("contract 1"), "{label}");
    assert!(!label.contains("patch"), "with an empty ledger the label names no patch: {label}");
}

#[test]
fn a_hash_is_the_pinned_one_only_when_it_is_equal_in_every_character() {
    assert!(verdict(ENGINE_SHA256, ENGINE_SHA256));
    assert!(!verdict(ENGINE_SHA256, ARCHIVE_SHA256));
    let mut flipped = ENGINE_SHA256.to_string();
    flipped.replace_range(63.., if ENGINE_SHA256.ends_with('0') { "1" } else { "0" });
    assert!(!verdict(&flipped, ENGINE_SHA256), "the last character counts");
    for i in [0usize, 31, 63] {
        let mut bytes = ENGINE_SHA256.as_bytes().to_vec();
        bytes[i] ^= 1;
        assert!(!verdict(std::str::from_utf8(&bytes).unwrap(), ENGINE_SHA256), "a difference of one single bit at {i} is a difference");
    }
    assert!(!verdict(&ENGINE_SHA256.to_uppercase(), ENGINE_SHA256), "only the lowercase form of the digest is accepted");
    assert!(!verdict("", ENGINE_SHA256) && !verdict(&ENGINE_SHA256[..63], ENGINE_SHA256) && !verdict(&format!("{ENGINE_SHA256}0"), ENGINE_SHA256));
    assert!(!verdict("", ""), "an empty digest never matches, even for an empty pin");
}

#[test]
fn the_real_listing_of_the_release_is_accepted_and_only_the_engine_is_to_be_extracted() {
    assert_eq!(check_listing(REAL_LISTING).unwrap(), [MEMBER]);
    let without_dirs: String = REAL_LISTING.lines().filter(|l| !l.starts_with('d')).map(|l| format!("{l}\n")).collect();
    assert_eq!(check_listing(&without_dirs).unwrap(), [MEMBER], "an archive that lists no folder entries is fine");
    let dot_slash = REAL_LISTING.replace("moli-v1.1.25", "./moli-v1.1.25");
    assert_eq!(check_listing(&dot_slash).unwrap(), [MEMBER], "a ./ in front of every name is fine");
}

#[test]
fn everything_else_in_a_listing_is_refused_with_its_reason() {
    let base = format!("{ROOT_DIR}{MEMBER_LINE}");
    let entry = |line: &str| check_listing(&format!("{base}{line}\n"));
    let r = "moli-v1.1.25-x86_64-unknown-linux-gnu";
    for (line, what) in [
        (format!("-rw-r--r-- a/b 5 2026-11-15 18:14 {r}/../evil"), "a path with .."),
        (format!("-rw-r--r-- a/b 5 2026-11-15 18:14 {r}/sub/../../evil"), "a path with .. inside"),
        ("-rw-r--r-- a/b 5 2026-11-15 18:14 ../evil".to_string(), "a path with .. outside the root"),
        ("-rw-r--r-- a/b 5 2026-11-15 18:14 /etc/passwd".to_string(), "an absolute path"),
        ("-rw-r--r-- a/b 5 2026-11-15 18:14 stranger.txt".to_string(), "a file outside the root folder"),
        ("-rw-r--r-- a/b 5 2026-11-15 18:14 moli".to_string(), "the engine's name outside the root folder"),
        (format!("-rw-r--r-- a/b 5 2026-11-15 18:14 {r}x/moli"), "a root folder that only starts like the real one"),
        (format!("lrwxrwxrwx a/b 0 2026-11-15 18:14 {r}/link -> /etc/passwd"), "a symbolic link"),
        (format!("hrw-r--r-- a/b 0 2026-11-15 18:14 {r}/other link to {r}/moli"), "a hard link"),
        (format!("crw-rw-rw- a/b 1,3 2026-11-15 18:14 {r}/null"), "a character device"),
        (format!("brw-rw---- a/b 8,0 2026-11-15 18:14 {r}/disk"), "a block device"),
        (format!("prw-r--r-- a/b 0 2026-11-15 18:14 {r}/pipe"), "a fifo"),
        ("drwxr-xr-x a/b 0 2026-11-15 18:14 elsewhere/".to_string(), "a folder that is not below the root"),
        (format!("-rw-r--r-- a/b 5 2026-11-15 18:14 {r}/moli"), "the engine twice"),
        (format!("-rw-r--r-- a/b 999999999999 2026-11-15 18:14 {r}/big"), "a file over the size cap"),
        (format!("-rw-r--r-- a/b many 2026-11-15 18:14 {r}/size"), "a size that is not a number"),
        (format!("-rw-r--r-- a/b 5 2026-11-15 18:14 {r}/a name with spaces"), "a name with spaces"),
    ] {
        let verdict = entry(&line);
        assert!(matches!(verdict, Err(Failure::Entry(_))), "{what}: {verdict:?}");
    }
    assert_eq!(entry(&format!("-rw-r--r-- a/b 5 2026-11-15 18:14 {r}/Moli")).unwrap(), [MEMBER], "another regular file under the root is judged but never extracted");
    assert!(matches!(check_listing(""), Err(Failure::Entry(_))), "an empty archive");
    assert!(matches!(check_listing(ROOT_DIR), Err(Failure::Entry(_))), "no engine in it");
    let other_case = MEMBER_LINE.replace("/moli", "/Moli");
    assert!(matches!(check_listing(&other_case), Err(Failure::Entry(_))), "the engine under another case is not the engine");
    let twice = format!("{MEMBER_LINE}{MEMBER_LINE}");
    assert!(matches!(check_listing(&twice), Err(Failure::Entry(_))), "the engine twice, alone");
    let as_link = "lrwxrwxrwx a/b 0 2026-11-15 18:14 moli-v1.1.25-x86_64-unknown-linux-gnu/moli -> /bin/sh\n";
    assert!(matches!(check_listing(as_link), Err(Failure::Entry(_))), "the engine as a link");
    let too_big = MEMBER_LINE.replace("112357968", &(MAX_FILE_BYTES + 1).to_string());
    assert!(matches!(check_listing(&too_big), Err(Failure::Entry(_))), "the engine over the size cap");
    let many: String = (0..=MAX_ENTRIES).map(|i| format!("-rw-r--r-- a/b 5 2026-11-15 18:14 {r}/f{i}\n")).collect();
    assert!(matches!(check_listing(&format!("{MEMBER_LINE}{many}")), Err(Failure::Entry(_))), "too many entries");
    assert!(matches!(check_listing(&format!("{REAL_LISTING}\u{0}")), Err(Failure::Entry(_))), "a NUL");
    assert!(matches!(check_listing(&"x".repeat(MAX_LISTING_BYTES + 1)), Err(Failure::Entry(_))), "a listing over the cap");
}

#[test]
fn curls_exit_codes_become_the_failures_they_mean() {
    assert_eq!(failure_of_curl_exit(6), Failure::Network);
    assert_eq!(failure_of_curl_exit(7), Failure::Network);
    assert_eq!(failure_of_curl_exit(35), Failure::Tls);
    assert_eq!(failure_of_curl_exit(60), Failure::Tls);
    assert_eq!(failure_of_curl_exit(28), Failure::Timeout);
    assert_eq!(failure_of_curl_exit(63), Failure::TooBig);
    assert_eq!(failure_of_curl_exit(23), Failure::NoSpace);
    assert_eq!(failure_of_curl_exit(18), Failure::Truncated);
    assert_eq!(failure_of_curl_exit(56), Failure::Truncated);
    assert_eq!(failure_of_curl_exit(22), Failure::Other("the release page answered with an error".into()));
    assert!(matches!(failure_of_curl_exit(99), Failure::Other(_)));
}

#[test]
fn every_failure_says_why_and_only_a_hash_mismatch_or_a_bad_entry_is_a_security_notice() {
    let all = [Failure::CurlMissing, Failure::TarMissing, Failure::Network, Failure::Tls, Failure::Timeout, Failure::TooBig, Failure::NoSpace, Failure::Truncated, Failure::HashMismatch, Failure::Entry("x".into()), Failure::Cannot("no sandbox"), Failure::Other("y".into())];
    for f in &all {
        assert!(f.text().len() > 10 && !f.text().contains('\n'), "{f:?}: {}", f.text());
        assert_eq!(f.security(), matches!(f, Failure::HashMismatch | Failure::Entry(_)), "{f:?}");
    }
    assert!(Failure::HashMismatch.text().contains("not the pinned") && Failure::HashMismatch.text().contains("removed"));
    assert!(Failure::Entry("x".into()).text().contains("not the engine"));
    let steps = manual_steps();
    assert!(steps.contains(ARCHIVE_URL) && steps.contains(ARCHIVE_SHA256) && steps.contains(ENGINE_SHA256) && steps.contains("0700"), "{steps}");
    assert!(steps.contains("~/.local/share/lico/browser/moli-v1.1.25") && steps.contains(MEMBER), "{steps}");
    assert!(!steps.to_lowercase().contains("obscura"), "{steps}");
}

#[test]
fn the_caps_are_exact_the_last_entry_and_the_last_byte_that_fit_are_accepted() {
    let r = "moli-v1.1.25-x86_64-unknown-linux-gnu";
    let fill = |n: usize| -> String { (0..n).map(|i| format!("-rw-r--r-- a/b 5 2026-11-15 18:14 {r}/f{i}\n")).collect() };
    assert_eq!(check_listing(&format!("{MEMBER_LINE}{}", fill(MAX_ENTRIES - 1))).unwrap(), [MEMBER], "exactly the most entries");
    assert!(matches!(check_listing(&format!("{MEMBER_LINE}{}", fill(MAX_ENTRIES))), Err(Failure::Entry(_))), "one more");
    let at_cap = MEMBER_LINE.replace("112357968", &MAX_FILE_BYTES.to_string());
    assert_eq!(check_listing(&at_cap).unwrap(), [MEMBER], "a file of exactly the cap");
    let bare_root = "drwxr-xr-x root/root 0 2026-11-15 18:29 moli-v1.1.25-x86_64-unknown-linux-gnu\n";
    assert_eq!(check_listing(&format!("{bare_root}{MEMBER_LINE}")).unwrap(), [MEMBER], "the root folder's entry without a slash");
    let head = format!("{MEMBER_LINE}-rw-r--r-- a/b 5 2026-11-15 18:14 {r}/");
    let pad = MAX_LISTING_BYTES - head.len() - 1;
    let exactly = format!("{head}{}\n", "p".repeat(pad));
    assert_eq!(exactly.len(), MAX_LISTING_BYTES);
    assert_eq!(check_listing(&exactly).unwrap(), [MEMBER], "a listing of exactly the cap is read");
    assert!(matches!(check_listing(&format!("{head}{}\n", "p".repeat(pad + 1))), Err(Failure::Entry(_))), "one byte over the cap");
}

#[test]
fn a_nul_inside_a_name_is_refused_though_the_line_looks_whole() {
    let line = "-rw-r--r-- a/b 5 2026-11-15 18:14 moli-v1.1.25-x86_64-unknown-linux-gnu/a\u{0}b\n";
    assert!(matches!(check_listing(&format!("{MEMBER_LINE}{line}")), Err(Failure::Entry(_))));
}
