use super::*;
use std::os::unix::fs::symlink;
use std::time::Instant;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lico_archive_{tag}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Runs a real tool (the way a person would) to prepare a test; the test says which tool it needs.
fn made(dir: &Path, program: &str, args: &[&str]) {
    let tool = find_tool(program).unwrap_or_else(|| panic!("{program} is needed by this test"));
    let done = Command::new(tool).current_dir(dir).args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().unwrap();
    assert!(done.success(), "{program} {args:?} failed");
}

fn names(dir: &Path) -> Vec<String> {
    let mut all: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    all.sort();
    all
}

/// Every path under `root` (a link is a link) with `dir` or its content, sorted: what a person would compare.
fn tree(root: &Path) -> Vec<(String, String)> {
    fn walk(base: &Path, at: &Path, into: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            let rel = path.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                into.push((rel, "dir".into()));
                walk(base, &path, into);
            } else if meta.file_type().is_symlink() {
                into.push((rel, format!("link to {}", std::fs::read_link(&path).unwrap().display())));
            } else {
                into.push((rel, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut all = Vec::new();
    walk(root, root, &mut all);
    all.sort();
    all
}

fn m(kind: char, size: u64, name: &str) -> Member {
    Member { kind, size, name: name.into() }
}

/// A folder that a person might hold: nested, an empty folder, a space and Korean in names.
fn a_folder_in(base: &Path, name: &str) -> PathBuf {
    let root = base.join(name);
    put(&root.join("a.txt"), "alpha\n");
    put(&root.join("sub/b.txt"), "beta\n");
    put(&root.join("with space.txt"), "gamma\n");
    put(&root.join("한글/자료.txt"), "delta\n");
    std::fs::create_dir_all(root.join("empty")).unwrap();
    root
}

#[test]
fn the_kind_of_an_archive_is_its_ending_in_any_case_and_a_bare_ending_is_no_archive() {
    for (name, format, ending) in [
        ("a.zip", Format::Zip, 4),
        ("A.ZIP", Format::Zip, 4),
        ("a.tar", Format::Tar, 4),
        ("a.tar.gz", Format::Tar, 7),
        ("a.TGZ", Format::Tar, 4),
        ("a.tar.bz2", Format::Tar, 8),
        ("a.tbz2", Format::Tar, 5),
        ("a.tar.xz", Format::Tar, 7),
        ("a.Txz", Format::Tar, 4),
        ("한글.tar.gz", Format::Tar, 7),
        ("a.b.zip", Format::Zip, 4),
        ("a b.TAR.GZ", Format::Tar, 7),
    ] {
        assert_eq!(kind_of(name), Some((format, ending)), "{name}");
    }
    for name in ["a", "a.gz", "a.tar.gz.bak", "zip", "a.zipx", ".zip", ".tar.gz", ".tgz", "a.7z", "a.rar", "a.tar.Z", "a.tar.", "atgz"] {
        assert_eq!(kind_of(name), None, "{name}");
    }
}

#[test]
fn the_stem_is_the_name_without_its_ending() {
    assert_eq!(stem_of("photos.zip"), Some("photos"));
    assert_eq!(stem_of("a.b.tar.GZ"), Some("a.b"));
    assert_eq!(stem_of("한글 자료.tgz"), Some("한글 자료"));
    assert_eq!(stem_of("x.tar"), Some("x"));
    assert_eq!(stem_of("a.txt"), None);
    assert_eq!(stem_of(".zip"), None);
}

#[test]
fn a_free_name_is_the_first_one_that_nothing_holds() {
    let dir = scratch("free");
    assert_eq!(free_name(&dir, "photos", "").as_deref(), Some("photos"));
    assert_eq!(free_name(&dir, "a.txt", ".zip").as_deref(), Some("a.txt.zip"));
    std::fs::create_dir(dir.join("photos")).unwrap();
    assert_eq!(free_name(&dir, "photos", "").as_deref(), Some("photos 2"), "a folder holds the name");
    put(&dir.join("photos 2"), "x");
    assert_eq!(free_name(&dir, "photos", "").as_deref(), Some("photos 3"), "a file holds the second");
    symlink("/nowhere/at/all", dir.join("photos 3")).unwrap();
    assert_eq!(free_name(&dir, "photos", "").as_deref(), Some("photos 4"), "a link that points nowhere holds it too");
    put(&dir.join("a.txt.zip"), "x");
    assert_eq!(free_name(&dir, "a.txt", ".zip").as_deref(), Some("a.txt 2.zip"), "the number goes before the ending");
    assert_eq!(free_name(&dir, "photos", ".zip").as_deref(), Some("photos.zip"), "another ending is another name");
    let full = scratch("free_full");
    std::fs::create_dir(full.join("x")).unwrap();
    for n in 2..=1000 {
        std::fs::create_dir(full.join(format!("x {n}"))).unwrap();
    }
    assert_eq!(free_name(&full, "x", ""), None, "it gives up after a thousand");
}

#[test]
fn a_listing_is_vetted_item_by_item_and_refused_with_the_reason() {
    let usual = [m('d', 0, "src/"), m('-', 5, "src/a.txt"), m('-', 0, "..a"), m('-', 1, "a..b"), m('-', 1, "src/..hidden/x"), m('-', 1, "./b"), m('?', 1, "from-windows.txt"), m('-', 1, "한글 이름.txt"), m('d', 0, "a/b/")];
    assert_eq!(member_problem(&usual), None);
    assert_eq!(member_problem(&[]), None, "an empty archive holds nothing wrong");
    for (bad, word) in [
        (m('-', 1, "../x"), "goes up"),
        (m('-', 1, "a/../b"), "goes up"),
        (m('d', 0, ".."), "goes up"),
        (m('-', 1, "a/.."), "goes up"),
        (m('d', 0, "a/../"), "goes up"),
        (m('-', 1, "/etc/x"), "root"),
        (m('d', 0, "/"), "root"),
        (m('-', 1, "x\ny"), "control"),
        (m('-', 1, "x\ry"), "control"),
        (m('-', 1, "a\0b"), "control"),
        (m('-', 1, "tab\there"), "control"),
        (m('-', 1, ""), "no name"),
        (m('l', 0, "src/link -> a.txt"), "link"),
        (m('h', 0, "src/hard"), "link"),
        (m('c', 0, "dev/null"), "device"),
        (m('b', 0, "dev/sda"), "device"),
        (m('p', 0, "a.fifo"), "device"),
        (m('s', 0, "a.sock"), "device"),
    ] {
        let why = member_problem(&[m('-', 1, "fine"), bad.clone()]).unwrap_or_else(|| panic!("{bad:?} must be refused"));
        assert!(why.contains(word), "{bad:?}: {why}");
    }
}

#[test]
fn a_listing_with_too_many_items_or_too_many_bytes_is_refused_at_the_edge() {
    let at_the_edge = vec![m('-', 0, "f"); MAX_MEMBERS];
    assert_eq!(member_problem(&at_the_edge), None, "100,000 items are allowed");
    let over = vec![m('-', 0, "f"); MAX_MEMBERS + 1];
    assert!(member_problem(&over).is_some_and(|w| w.contains("100,000")), "one more is not");
    assert_eq!(member_problem(&[m('-', MAX_TOTAL, "a")]), None, "8 GiB exactly is allowed");
    assert!(member_problem(&[m('-', MAX_TOTAL, "a"), m('-', 1, "b")]).is_some_and(|w| w.contains("8 GiB")), "one byte more is not");
    assert!(member_problem(&[m('-', u64::MAX, "a"), m('-', u64::MAX, "b")]).is_some_and(|w| w.contains("8 GiB")), "the sum cannot wrap around");
}

const ZIP_LISTING: &str = "Archive:  /work/out.zip
Zip file size: 927 bytes, number of entries: 6
drwx------  3.0 unx        0 bx stor 26-Nov-25 10:32 src/
drwx------  3.0 unx        0 bx stor 26-Nov-25 10:32 src/sub/
-rw-------  3.0 unx        5 tx stor 26-Nov-25 10:32 src/sub/b.txt
-rw-------  3.0 unx        1 tx stor 26-Nov-25 10:32 src/with space.txt
lrwxrwxrwx  3.0 unx        5 bx stor 26-Nov-25 10:32 src/link
?rw-------  2.0 unx     1234 b- stor 26-Nov-25 10:38 한글 자료.txt
6 files, 1245 bytes uncompressed, 1245 bytes compressed:  0.0%
";

#[test]
fn the_listing_of_unzip_gives_the_kind_the_size_and_the_whole_name_of_each_item() {
    let members = zip_listing(ZIP_LISTING).unwrap();
    assert_eq!(
        members,
        vec![m('d', 0, "src/"), m('d', 0, "src/sub/"), m('-', 5, "src/sub/b.txt"), m('-', 1, "src/with space.txt"), m('l', 5, "src/link"), m('?', 1234, "한글 자료.txt")]
    );
    let fat = "Archive:  x.zip\nZip file size: 300 bytes, number of entries: 1\n-rw-a--     2.0 fat      120 b- defN 20-Jan-02 12:00 docs/readme.txt\n1 file, 120 bytes uncompressed, 90 bytes compressed:  25.0%\n";
    assert_eq!(zip_listing(fat).unwrap(), vec![m('-', 120, "docs/readme.txt")], "a file made on a Windows disk has a shorter first word");
    let one = "Archive:  one.zip\nZip file size: 110 bytes, number of entries: 1\n?rw-------  2.0 unx        2 b- stor 26-Nov-25 10:38 a.txt\n1 file, 2 bytes uncompressed, 2 bytes compressed:  0.0%\n";
    assert_eq!(zip_listing(one).unwrap(), vec![m('?', 2, "a.txt")]);
}

#[test]
fn a_zip_listing_that_a_name_could_have_forged_is_refused_whole() {
    // a name with a line break shows as two lines: the second looks like a member of its own, or like nothing at all
    let split = "Archive:  x.zip\nZip file size: 208 bytes, number of entries: 2\n?rw-------  2.0 unx        1 b- stor 26-Nov-25 10:37 x\n/../../y\n?rw-------  2.0 unx        1 b- stor 26-Nov-25 10:37 ok.txt\n2 files, 2 bytes uncompressed, 2 bytes compressed:  0.0%\n";
    let why = zip_listing(split).expect_err("a line that is no item");
    assert!(why.contains("line break"), "{why}");
    let forged = "Archive:  x.zip\nZip file size: 208 bytes, number of entries: 1\n?rw-------  2.0 unx        1 b- stor 26-Nov-25 10:37 x\n?rw-------  2.0 unx        1 b- stor 26-Nov-25 10:37 fake\n1 file, 1 bytes uncompressed, 1 bytes compressed:  0.0%\n";
    assert!(zip_listing(forged).expect_err("one line too many").contains("line break"), "more lines than the archive says it holds");
    let few = ZIP_LISTING.replace("number of entries: 6", "number of entries: 7");
    assert!(zip_listing(&few).is_err(), "fewer lines than the archive says it holds");
    let footer_in_a_name = "Archive:  x.zip\nZip file size: 208 bytes, number of entries: 1\n?rw-------  2.0 unx        1 b- stor 26-Nov-25 10:37 x\n1 file, 1 bytes uncompressed, 1 bytes compressed:  0.0%\n1 file, 1 bytes uncompressed, 1 bytes compressed:  0.0%\n";
    assert!(zip_listing(footer_in_a_name).is_err(), "a footer in the middle of the items");
    let no_footer = "Archive:  x.zip\nZip file size: 208 bytes, number of entries: 1\n?rw-------  2.0 unx        1 b- stor 26-Nov-25 10:37 x\nthe last line is no footer\n";
    assert!(zip_listing(no_footer).is_err(), "the last line must be the footer, or a line may have been cut off");
    let no_title = "Warning:  x.zip\nZip file size: 208 bytes, number of entries: 1\n?rw-------  2.0 unx        1 b- stor 26-Nov-25 10:37 x\n1 file, 1 bytes uncompressed, 1 bytes compressed:  0.0%\n";
    assert!(zip_listing(no_title).is_err(), "the first line must name the archive");
    for text in ["", "Archive:  x.zip\n", "nothing like it\n", "Archive:  x.zip\nZip file size: 1 bytes, number of entries: 1\n"] {
        assert!(zip_listing(text).is_err(), "{text:?}");
    }
    let empty = "Archive:  e.zip\nEmpty zip file.\n";
    assert_eq!(zip_listing(empty).unwrap(), Vec::<Member>::new(), "an archive with nothing in it");
}

const TAR_LISTING: &str = "drwx------ 1000/1000         0 2026-11-25 10:32 src/
-rw------- 1000/1000         5 2026-11-25 10:32 src/sub/b.txt
-rw------- 1000/1000         1 2026-11-25 10:32 src/with space.txt
lrwxrwxrwx 1000/1000         0 2026-11-25 10:32 src/link -> a.txt
-rw------- 1000/1000 9663676416 2026-11-25 10:38 ./big
prw------- 1000/1000         0 2026-11-25 10:38 ./p
crw-rw-rw- 0/0             1,3 2026-11-25 10:38 dev/null
hrw------- 1000/1000         0 2026-11-25 10:38 ./a link to ./h
-rw------- 1000/1000         2 2026-11-25 10:38 ./new\\nline
-rw------- 1000/1000         2 2026-11-25 10:38 ./\\355\\225\\234\\352\\270\\200.txt
";

#[test]
fn the_listing_of_tar_gives_the_kind_the_size_and_the_unescaped_name_of_each_item() {
    let got = tar_listing(TAR_LISTING).unwrap();
    assert_eq!(got.len(), 10);
    assert_eq!(got[0], m('d', 0, "src/"));
    assert_eq!(got[2], m('-', 1, "src/with space.txt"), "a space in a name stays");
    assert_eq!(got[3].kind, 'l');
    assert_eq!(got[4], m('-', 9_663_676_416, "./big"), "a size bigger than four bytes");
    assert_eq!((got[5].kind, got[6].kind, got[7].kind), ('p', 'c', 'h'));
    assert_eq!(got[6].size, 0, "a device has two numbers where the size is: no size");
    assert_eq!(got[8].name, "./new\nline", "an escaped line break is a line break again, for the check to see");
    assert_eq!(got[9].name, "./한글.txt", "octal escapes are the bytes of the name");
}

#[test]
fn a_tar_listing_that_cannot_be_read_is_refused_and_the_owner_field_cannot_move_the_name() {
    for text in ["garbage\n", "-rw------- 1000/1000 5 not-a-date 10:32 a\n", "-rw------- bob/bob 5 2026-11-25 10:32 a\n", "-rw------- 1000/1000 five 2026-11-25 10:32 a\n", "\n", "-rw------- 1000/1000 5 2026-11-25 1032 a\n"] {
        assert!(tar_listing(text).is_err(), "{text:?}");
    }
    assert_eq!(tar_listing("").unwrap(), Vec::<Member>::new(), "nothing listed is an archive without items");
    let tricky = "-rw------- 1000/1000 5 2026-11-25 10:32 ../x 2026-11-25 10:32 y\n";
    assert_eq!(tar_listing(tricky).unwrap()[0].name, "../x 2026-11-25 10:32 y", "the name is all that follows the time");
    assert_eq!(tar_listing("-rw------- 1000/1000 5 2026-11-25 10:32 a\\\\b\\q\n").unwrap()[0].name, "a\\b\\q", "a backslash, and a backslash that means nothing");
}

#[test]
fn a_language_that_is_not_utf_8_is_replaced_so_that_the_tools_keep_korean_names() {
    assert_eq!(lang_from(Some("ko_KR.UTF-8")), "ko_KR.UTF-8");
    assert_eq!(lang_from(Some("en_US.utf8")), "en_US.utf8");
    assert_eq!(lang_from(Some("C")), "C.UTF-8");
    assert_eq!(lang_from(Some("POSIX")), "C.UTF-8");
    assert_eq!(lang_from(Some("")), "C.UTF-8");
    assert_eq!(lang_from(None), "C.UTF-8");
}

#[test]
fn the_tools_are_found_in_the_short_path_and_this_machine_has_them() {
    for tool in ["zip", "unzip", "tar", "sh"] {
        let found = find_tool(tool).unwrap_or_else(|| panic!("{tool} is installed here"));
        assert!(found.is_absolute() && found.is_file(), "{found:?}");
    }
    assert_eq!(find_tool("no-such-tool-here"), None);
    assert_eq!(find_tool(""), None);
    assert_eq!(find_tool("../bin/sh"), None, "a name with a slash is not looked up");
    assert_eq!((tool_missing(Format::Zip), tool_missing(Format::Tar), zip_missing()), (None, None, false));
}

#[test]
fn a_run_past_its_limit_is_stopped_and_what_it_says_is_cut_and_the_environment_is_cleared() {
    let sleep = find_tool("sleep").unwrap();
    let began = Instant::now();
    let mut cmd = Command::new(sleep);
    cmd.arg("5");
    let why = run(cmd, Path::new("/"), Duration::from_millis(300), 0).expect_err("a run past its limit");
    assert!(why.contains("stopped"), "{why}");
    assert!(began.elapsed() < Duration::from_secs(3), "it was killed, not waited for: {:?}", began.elapsed());

    let sh = find_tool("sh").unwrap();
    let mut loud = Command::new(&sh);
    loud.args(["-c", "head -c 200000 /dev/zero | tr '\\0' 'e' >&2; exit 3"]);
    let ran = run(loud, Path::new("/"), Duration::from_secs(20), 0).unwrap();
    assert!(!ran.ok, "the exit code is the answer");
    assert_eq!(ran.err.len(), ERR_LIMIT, "the error text is read up to 4 KiB and the rest is thrown away without blocking the tool");

    let mut env = Command::new(&sh);
    env.args(["-c", "echo \"[$HOME][$PATH][$LANG][$(pwd)]\""]);
    let ran = run(env, Path::new("/"), Duration::from_secs(20), 1000).unwrap();
    let said = String::from_utf8(ran.out).unwrap();
    assert!(ran.ok && said.starts_with("[]["), "no HOME: {said}");
    assert!(said.contains("[/usr/local/bin:/usr/bin:/bin]") && said.contains("UTF-8") && said.trim_end().ends_with("[/]"), "{said}");

    let mut flood = Command::new(&sh);
    flood.args(["-c", "head -c 100000 /dev/zero | tr '\\0' 'x'"]);
    let why = run(flood, Path::new("/"), Duration::from_secs(20), 1000).expect_err("an answer past its cap");
    assert!(why.contains("too long"), "{why}");

    let mut fine = Command::new(&sh);
    fine.args(["-c", "echo hello; echo trouble >&2; exit 2"]);
    let ran = run(fine, Path::new("/"), Duration::from_secs(20), 1000).unwrap();
    assert_eq!((ran.ok, ran.out, ran.err.trim().to_string()), (false, b"hello\n".to_vec(), "trouble".to_string()));
    let missing = Command::new("/no/such/program");
    assert!(run(missing, Path::new("/"), Duration::from_secs(1), 0).is_err(), "a tool that cannot start is an error with a reason");
}

#[test]
fn a_folder_is_compressed_to_a_zip_beside_it_that_unzip_reads_back_and_a_second_one_never_replaces_the_first() {
    let base = scratch("zip_round");
    let src = a_folder_in(&base, "src");
    let first = compress_within(&base, "src", RUN_LIMIT).unwrap();
    assert_eq!(first, "src.zip");
    assert_eq!(names(&base), ["src", "src.zip"], "nothing else is left in the folder: no half-made file");
    let before = std::fs::read(base.join("src.zip")).unwrap();
    assert_eq!(before[..2], *b"PK", "a zip");
    let second = compress_within(&base, "src", RUN_LIMIT).unwrap();
    assert_eq!(second, "src 2.zip", "the first free name");
    assert_eq!(std::fs::read(base.join("src.zip")).unwrap(), before, "the first archive is the same bytes");
    assert_eq!(names(&base), ["src", "src 2.zip", "src.zip"]);
    let listing = Command::new(find_tool("unzip").unwrap()).arg("-Z1").arg(base.join("src.zip")).output().unwrap();
    let listed = String::from_utf8_lossy(&listing.stdout).into_owned();
    for part in ["src/a.txt", "src/sub/b.txt", "src/with space.txt", "src/한글/자료.txt", "src/empty/"] {
        assert!(listed.lines().any(|l| l == part), "{part} is in the archive: {listed}");
    }
    assert_eq!(tree(&src).len(), 7, "the folder itself is untouched");
}

#[test]
fn a_file_is_compressed_under_its_whole_name_and_a_link_is_stored_as_a_link() {
    let base = scratch("zip_file");
    put(&base.join("notes.txt"), "text\n");
    assert_eq!(compress_within(&base, "notes.txt", RUN_LIMIT).unwrap(), "notes.txt.zip");
    put(&base.join("real/data.txt"), "data\n");
    symlink("real", base.join("pointer")).unwrap();
    assert_eq!(compress_within(&base, "pointer", RUN_LIMIT).unwrap(), "pointer.zip");
    let listing = Command::new(find_tool("unzip").unwrap()).arg("-Z").arg(base.join("pointer.zip")).output().unwrap();
    let text = String::from_utf8_lossy(&listing.stdout).into_owned();
    assert!(text.lines().any(|l| l.starts_with('l') && l.ends_with("pointer")), "the link is in the archive as a link, not as what it points to: {text}");
    assert!(!text.contains("data.txt"), "what it points to is not followed: {text}");
    put(&base.join("outer/in.txt"), "in\n");
    symlink("/etc", base.join("outer/etc_link")).unwrap();
    assert_eq!(compress_within(&base, "outer", RUN_LIMIT).unwrap(), "outer.zip");
    let listing = Command::new(find_tool("unzip").unwrap()).arg("-Z").arg(base.join("outer.zip")).output().unwrap();
    let text = String::from_utf8_lossy(&listing.stdout).into_owned();
    assert!(text.lines().any(|l| l.starts_with('l') && l.ends_with("etc_link")), "a link inside a folder too: {text}");
    assert!(!text.contains("passwd"), "nothing of /etc went in: {text}");
}

#[test]
fn a_compress_that_cannot_be_done_leaves_nothing_in_the_folder() {
    let base = scratch("zip_fail");
    let _ = a_folder_in(&base, "src");
    let why = compress_within(&base, "src", Duration::ZERO).expect_err("a limit of nothing");
    assert!(why.contains("stopped"), "{why}");
    assert_eq!(names(&base), ["src"], "no archive and no partial work");
    // a short limit: should a name pass that must not (`..` is the folder above), the run is stopped instead of reading it all
    for bad in ["", ".", "..", "a/b", "/etc", "x\0y"] {
        assert_eq!(compress_within(&base, bad, Duration::from_secs(2)).err().as_deref(), Some("that is not a name"), "{bad:?}");
    }
    assert_eq!(compress_within(&base, "gone", RUN_LIMIT).err().as_deref(), Some("it is gone"));
    assert_eq!(names(&base), ["src"]);
    assert_eq!(compress_within(&base.join("not-here"), "src", RUN_LIMIT).err().as_deref(), Some("it is gone"), "a folder that is not there");
}

/// Makes `name.tar.gz` of `what` in `base` with the real tar.
fn tar_gz(base: &Path, archive: &str, what: &str) {
    made(base, "tar", &["-czf", archive, what]);
}

#[test]
fn a_zip_is_extracted_into_a_new_folder_named_like_it_and_a_taken_name_gets_a_number() {
    let base = scratch("unzip_round");
    let src = a_folder_in(&base, "src");
    assert_eq!(compress_within(&base, "src", RUN_LIMIT).unwrap(), "src.zip");
    let first = extract_within(&base.join("src.zip"), RUN_LIMIT).unwrap();
    assert_eq!(first, base.join("src 2"), "the folder src is there already: the first free name, nothing replaced");
    assert_eq!(tree(&first.join("src")), tree(&src), "the same files with the same content, the Korean and the space names, the empty folder");
    std::fs::remove_dir_all(&src).unwrap();
    std::fs::remove_dir_all(&first).unwrap();
    let again = extract_within(&base.join("src.zip"), RUN_LIMIT).unwrap();
    assert_eq!(again, base.join("src"), "the name of the archive without its ending");
    assert_eq!(names(&base), ["src", "src.zip"]);
    let third = extract_within(&base.join("src.zip"), RUN_LIMIT).unwrap();
    assert_eq!(third, base.join("src 2"));
}

#[test]
fn a_tar_gz_a_tar_and_a_tar_xz_are_extracted_the_same_way() {
    let base = scratch("untar_round");
    let work = scratch("untar_work");
    let _ = a_folder_in(&work, "photos");
    tar_gz(&work, "photos.tar.gz", "photos");
    made(&work, "tar", &["-cf", "plain.TAR", "photos"]);
    made(&work, "tar", &["-cJf", "packed.txz", "photos"]);
    for (archive, folder) in [("photos.tar.gz", "photos"), ("plain.TAR", "plain"), ("packed.txz", "packed")] {
        std::fs::copy(work.join(archive), base.join(archive)).unwrap();
        let into = extract_within(&base.join(archive), RUN_LIMIT).unwrap_or_else(|e| panic!("{archive}: {e}"));
        assert_eq!(into, base.join(folder), "{archive}");
        assert_eq!(tree(&into.join("photos")), tree(&work.join("photos")), "{archive}");
    }
    assert_eq!(names(&base), ["packed", "packed.txz", "photos", "photos.tar.gz", "plain", "plain.TAR"]);
}

/// The archive is made in `work` from what `args` say; it is copied into a fresh `base` (the folder the person looks at) and extracted there:
/// the answer, and what the folder holds afterwards. `outside` is the folder above `base`, where nothing may appear.
fn extract_from(archive: &str, args: &[&str], prepare: impl Fn(&Path)) -> (Result<PathBuf, String>, Vec<String>, PathBuf) {
    let outer = scratch("untar_bad");
    let base = outer.join("inner");
    let work = outer.join("work");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    prepare(&work);
    made(&work, "tar", args);
    std::fs::copy(work.join(archive), base.join(archive)).unwrap();
    let got = extract_within(&base.join(archive), RUN_LIMIT);
    (got, names(&base), outer)
}

#[test]
fn an_archive_with_a_name_that_goes_up_or_starts_at_the_root_is_refused_and_nothing_is_written() {
    let (got, left, outer) = extract_from("up.tar", &["-cf", "up.tar", "--transform", "s,^,../,", "a.txt"], |w| put(&w.join("a.txt"), "x"));
    let why = got.expect_err("a name that goes up");
    assert!(why.contains("goes up"), "{why}");
    assert_eq!(left, ["up.tar"], "no folder was made");
    assert_eq!(names(&outer), ["inner", "work"], "and nothing outside");

    let hold = scratch("untar_abs_src");
    put(&hold.join("secret.txt"), "x");
    let abs = format!("{}/secret.txt", hold.display());
    let (got, left, _) = extract_from("abs.tar", &["-P", "-cf", "abs.tar", &abs], |_| {});
    assert!(got.expect_err("a root name").contains("root"));
    assert_eq!(left, ["abs.tar"]);
}

#[test]
fn a_tar_with_a_line_break_in_a_name_is_refused_and_nothing_is_written() {
    let (got, left, _) = extract_from("nl.tar", &["-cf", "nl.tar", "new\nline"], |w| put(&w.join("new\nline"), "x"));
    let why = got.expect_err("a line break in a name");
    assert!(why.contains("control"), "{why}");
    assert_eq!(left, ["nl.tar"]);
}

#[test]
fn an_archive_with_a_link_a_hard_link_or_a_pipe_is_refused_and_nothing_is_written() {
    let (got, left, _) = extract_from("link.tar", &["-cf", "link.tar", "d"], |w| {
        put(&w.join("d/a.txt"), "x");
        symlink("/etc", w.join("d/out")).unwrap();
    });
    let why = got.expect_err("a symbolic link");
    assert!(why.contains("link"), "{why}");
    assert_eq!(left, ["link.tar"]);
    let (got, left, _) = extract_from("hard.tar", &["-cf", "hard.tar", "d"], |w| {
        put(&w.join("d/a.txt"), "x");
        std::fs::hard_link(w.join("d/a.txt"), w.join("d/b.txt")).unwrap();
    });
    assert!(got.expect_err("a hard link").contains("link"));
    assert_eq!(left, ["hard.tar"]);
    let (got, left, _) = extract_from("pipe.tar", &["-cf", "pipe.tar", "d"], |w| {
        std::fs::create_dir_all(w.join("d")).unwrap();
        made(&w.join("d"), "mkfifo", &["p"]);
    });
    assert!(got.expect_err("a pipe").contains("device"));
    assert_eq!(left, ["pipe.tar"]);
}

#[test]
fn an_archive_that_would_write_more_than_eight_gib_is_refused_before_anything_is_written() {
    let (got, left, _) = extract_from("big.tar", &["-cSf", "big.tar", "big"], |w| made(w, "truncate", &["-s", "9G", "big"]));
    let why = got.expect_err("nine GiB of sparse file");
    assert!(why.contains("8 GiB"), "{why}");
    assert_eq!(left, ["big.tar"], "the archive is a few kilobytes and its listing said 9 GiB");
}

#[test]
fn a_broken_archive_and_a_failure_in_the_middle_leave_no_folder_and_say_what_the_tool_said() {
    let base = scratch("untar_broken");
    std::fs::write(base.join("junk.tar.gz"), b"this is not a gzip file at all").unwrap();
    let why = extract_within(&base.join("junk.tar.gz"), RUN_LIMIT).expect_err("junk");
    assert!(why.starts_with("tar said:") && why.contains("gzip"), "the tool's own words: {why}");
    assert_eq!(names(&base), ["junk.tar.gz"]);
    std::fs::write(base.join("junk.zip"), b"PK but then nothing").unwrap();
    assert!(extract_within(&base.join("junk.zip"), RUN_LIMIT).err().is_some_and(|w| w.starts_with("unzip said:")));
    assert_eq!(names(&base), ["junk.tar.gz", "junk.zip"]);

    // the listing is fine, the writing fails halfway: a member whose name is too long for the disk
    let long = "x".repeat(300);
    let (got, left, _) = extract_from("long.tar", &["-cf", "long.tar", "--transform", &format!("s,^b$,{long},"), "a.txt", "b"], |w| {
        put(&w.join("a.txt"), "first");
        put(&w.join("b"), "second");
    });
    assert!(got.is_err(), "a name of 300 bytes cannot be written");
    assert_eq!(left, ["long.tar"], "the folder that was made for it is removed again");
}

#[test]
fn what_is_not_an_archive_or_not_a_file_or_not_absolute_is_refused_with_nothing_made() {
    let base = scratch("untar_not");
    put(&base.join("notes.txt"), "text");
    assert_eq!(extract_within(&base.join("notes.txt"), RUN_LIMIT).err().as_deref(), Some("not an archive"));
    assert_eq!(extract_within(&base.join("gone.zip"), RUN_LIMIT).err().as_deref(), Some("it is gone"));
    std::fs::create_dir(base.join("folder.zip")).unwrap();
    assert!(extract_within(&base.join("folder.zip"), RUN_LIMIT).err().is_some_and(|w| w.contains("not a file")), "a folder with a name like an archive");
    symlink("/etc", base.join("to-folder.zip")).unwrap();
    assert!(extract_within(&base.join("to-folder.zip"), RUN_LIMIT).err().is_some_and(|w| w.contains("not a file")), "a link to a folder");
    symlink("/no/such/archive.zip", base.join("dangling.zip")).unwrap();
    assert_eq!(extract_within(&base.join("dangling.zip"), RUN_LIMIT).err().as_deref(), Some("it is gone"), "a link that points nowhere");
    assert_eq!(extract_within(Path::new("relative.zip"), RUN_LIMIT).err().as_deref(), Some("it needs a full path"), "only an absolute path");
    assert_eq!(names(&base), ["dangling.zip", "folder.zip", "notes.txt", "to-folder.zip"]);
    put(&base.join("empty.zip"), "");
    assert!(extract_within(&base.join("empty.zip"), RUN_LIMIT).is_err(), "an empty file is not a zip");
    let _ = a_folder_in(&base, "z");
    assert_eq!(compress_within(&base, "z", RUN_LIMIT).unwrap(), "z.zip");
    let why = extract_within(&base.join("z.zip"), Duration::ZERO).expect_err("a limit of nothing");
    assert!(why.contains("stopped"), "{why}");
    assert!(!names(&base).contains(&"z 2".to_string()), "nothing was made when the listing was stopped");
}

#[test]
fn a_link_to_an_archive_file_is_read_like_the_file_and_the_folder_is_named_after_the_link() {
    let base = scratch("unzip_alias");
    let src = a_folder_in(&base, "src");
    assert_eq!(compress_within(&base, "src", RUN_LIMIT).unwrap(), "src.zip");
    symlink(base.join("src.zip"), base.join("alias.zip")).unwrap();
    let into = extract_within(&base.join("alias.zip"), RUN_LIMIT).unwrap();
    assert_eq!(into, base.join("alias"));
    assert_eq!(tree(&into.join("src")), tree(&src));
}

#[test]
fn a_zip_that_holds_a_link_is_refused_with_nothing_made() {
    let base = scratch("unzip_link");
    put(&base.join("d/a.txt"), "x");
    symlink("/etc", base.join("d/out")).unwrap();
    assert_eq!(compress_within(&base, "d", RUN_LIMIT).unwrap(), "d.zip");
    let why = extract_within(&base.join("d.zip"), RUN_LIMIT).expect_err("a zip with a link");
    assert!(why.contains("link"), "{why}");
    assert_eq!(names(&base), ["d", "d.zip"], "no folder, the link is only said");
}

#[test]
fn a_name_with_brackets_spaces_and_korean_makes_an_archive_that_comes_back_the_same() {
    let base = scratch("odd_names");
    let src = a_folder_in(&base, "내 자료 [2024] *final*");
    assert_eq!(compress_within(&base, "내 자료 [2024] *final*", RUN_LIMIT).unwrap(), "내 자료 [2024] *final*.zip");
    let back = extract_within(&base.join("내 자료 [2024] *final*.zip"), RUN_LIMIT).unwrap();
    assert_eq!(back, base.join("내 자료 [2024] *final* 2"));
    assert_eq!(tree(&back.join("내 자료 [2024] *final*")), tree(&src));
    put(&base.join("-dash.txt"), "d");
    assert_eq!(compress_within(&base, "-dash.txt", RUN_LIMIT).unwrap(), "-dash.txt.zip", "a name that starts with a dash is not an option");
    std::fs::remove_file(base.join("-dash.txt")).unwrap();
    let from_dash = extract_within(&base.join("-dash.txt.zip"), RUN_LIMIT).unwrap();
    assert_eq!(from_dash, base.join("-dash.txt"));
    assert_eq!(tree(&from_dash), vec![("-dash.txt".to_string(), "d".to_string())]);
}
