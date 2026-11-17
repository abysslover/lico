//! The contract tests of `lico-engine-pack` (SPEC.md).

use lico_engine_pack::*;

#[test]
fn the_pins_are_the_release_that_was_measured_and_the_url_is_https() {
    assert_eq!(RELEASE, "v0.2.6");
    assert!(ARCHIVE_URL.starts_with("https://github.com/") && ARCHIVE_URL.ends_with("/v0.2.6/obscura-x86_64-linux.tar.gz"), "{ARCHIVE_URL}");
    assert!(!ARCHIVE_URL.contains("http://") && !ARCHIVE_URL.contains(' ') && !ARCHIVE_URL.contains('@'));
    for h in [ARCHIVE_SHA256, ENGINE_SHA256, WORKER_SHA256] {
        assert!(h.len() == 64 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')), "{h}");
    }
    assert_eq!((ENGINE_FILE, WORKER_FILE), ("obscura", "obscura-worker"));
    assert_eq!(ARCHIVE_BYTES, 71_782_128);
    assert!(MAX_ARCHIVE_BYTES >= ARCHIVE_BYTES && MAX_ARCHIVE_BYTES <= 2 * ARCHIVE_BYTES, "a cap near the real size, not a blank cheque");
    assert_eq!(hash12(ARCHIVE_SHA256), "eed986e478c5");
    assert_eq!(size_mb(), 72);
}

#[test]
fn a_hash_is_the_pinned_one_only_when_it_is_equal_in_every_character() {
    assert!(verdict(ENGINE_SHA256, ENGINE_SHA256));
    assert!(!verdict(ENGINE_SHA256, WORKER_SHA256));
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
    assert!(!verdict("", ""), "an empty digest is never a match, even for an empty pin");
}

const GOOD: &str = "-rwxr-xr-x user/user  80583768 2026-11-01 05:48 obscura\n-rwxr-xr-x user/user  75754576 2026-11-01 05:48 obscura-worker\n";

#[test]
fn a_listing_of_exactly_the_two_regular_files_is_accepted_in_either_order_and_with_a_dot_slash() {
    assert_eq!(check_listing(GOOD).unwrap(), ["obscura", "obscura-worker"]);
    let swapped = "-rwx------ a/b 75754576 2026-11-01 05:48 ./obscura-worker\n-rw-r--r-- a/b 80583768 2026-11-01 05:48 ./obscura\n";
    assert_eq!(check_listing(swapped).unwrap(), ["obscura-worker", "obscura"]);
    let with_dir = format!("drwxr-xr-x a/b 0 2026-11-01 05:48 ./\n{GOOD}");
    assert_eq!(check_listing(&with_dir).unwrap().len(), 2, "the folder entry of the archive itself is fine");
}

#[test]
fn everything_else_in_a_listing_is_refused_with_its_reason() {
    let entry = |line: &str| check_listing(&format!("{GOOD}{line}\n"));
    for (line, what) in [
        ("-rw-r--r-- a/b 5 2026-11-01 05:48 ../evil", "a path with .."),
        ("-rw-r--r-- a/b 5 2026-11-01 05:48 sub/../../evil", "a path with .. inside"),
        ("-rw-r--r-- a/b 5 2026-11-01 05:48 /etc/passwd", "an absolute path"),
        ("lrwxrwxrwx a/b 0 2026-11-01 05:48 obscura-link -> /etc/passwd", "a symbolic link"),
        ("hrw-r--r-- a/b 0 2026-11-01 05:48 other link to obscura", "a hard link"),
        ("crw-rw-rw- a/b 1,3 2026-11-01 05:48 null", "a character device"),
        ("brw-rw---- a/b 8,0 2026-11-01 05:48 disk", "a block device"),
        ("prw-r--r-- a/b 0 2026-11-01 05:48 pipe", "a fifo"),
        ("drwxr-xr-x a/b 0 2026-11-01 05:48 ./sub/", "a folder that is not the root"),
        ("-rw-r--r-- a/b 5 2026-11-01 05:48 extra.txt", "a third file"),
        ("-rw-r--r-- a/b 5 2026-11-01 05:48 obscura", "the engine twice"),
        ("-rw-r--r-- a/b 5 2026-11-01 05:48 Obscura", "a name that differs in case"),
    ] {
        assert!(matches!(entry(line), Err(Failure::Entry(_))), "{what}: {:?}", entry(line));
    }
    for (listing, what) in [
        ("-rwxr-xr-x a/b 5 2026-11-01 05:48 obscura\n-rwxr-xr-x a/b 5 2026-11-01 05:48 obscura\n", "the engine twice and no worker"),
        ("-rwxr-xr-x a/b 5 2026-11-01 05:48 obscura\n-rwxr-xr-x a/b 5 2026-11-01 05:48 evil\n", "the engine and a stranger"),
        ("-rwxr-xr-x a/b 5 2026-11-01 05:48 obscura\nlrwxrwxrwx a/b 0 2026-11-01 05:48 obscura-worker\n", "the worker as a link"),
        ("-rwxr-xr-x a/b 5 2026-11-01 05:48 obscura\nhrwxr-xr-x a/b 0 2026-11-01 05:48 obscura-worker\n", "the worker as a hard link"),
        ("-rwxr-xr-x a/b 5 2026-11-01 05:48 obscura\n-rwxr-xr-x a/b 5 2026-11-01 05:48 ../obscura-worker\n", "the worker below .."),
    ] {
        assert!(matches!(check_listing(listing), Err(Failure::Entry(_))), "{what}");
    }
    assert!(matches!(check_listing(""), Err(Failure::Entry(_))), "an empty archive");
    assert!(matches!(check_listing("-rwxr-xr-x a/b 80583768 2026-11-01 05:48 obscura\n"), Err(Failure::Entry(_))), "the worker is missing");
    assert!(matches!(check_listing("-rwxr-xr-x a/b 999999999999 2026-11-01 05:48 obscura\n-rwxr-xr-x a/b 5 2026-11-01 05:48 obscura-worker\n"), Err(Failure::Entry(_))), "a file over the size cap");
    assert!(matches!(check_listing("-rwxr-xr-x a/b notanumber 2026-11-01 05:48 obscura\n-rwxr-xr-x a/b 5 2026-11-01 05:48 obscura-worker\n"), Err(Failure::Entry(_))), "a size that is not a number");
    let many: String = (0..9).map(|i| format!("-rw-r--r-- a/b 5 2026-11-01 05:48 f{i}\n")).collect();
    assert!(matches!(check_listing(&many), Err(Failure::Entry(_))), "too many entries");
    assert!(matches!(check_listing(&format!("{GOOD}\u{0}")), Err(Failure::Entry(_))), "a NUL");
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
    assert!(manual_steps().contains(ARCHIVE_URL) && manual_steps().contains(ARCHIVE_SHA256) && manual_steps().contains(ENGINE_SHA256) && manual_steps().contains("0700"));
}
