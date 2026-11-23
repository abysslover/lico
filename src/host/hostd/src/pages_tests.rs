//! M22-P05c: the remembered pages (the store): what is kept, in what order, how many, and how the file is read and written.

use super::pages::{forget, load, remember, strip_fragment, Page, FILE, MAX_FILE, MAX_PAGES, MAX_TITLE};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

fn state(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lico_pages_{name}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn page(address: &str, title: &str, time: u64) -> Page {
    Page { address: address.into(), title: title.into(), time }
}

fn put(dir: &Path, text: &str, mode: u32) {
    let file = dir.join(FILE);
    let _ = std::fs::remove_file(&file);
    std::fs::write(&file, text).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn a_new_page_is_first_and_the_same_address_is_one_entry_renewed_and_moved_to_the_front() {
    let dir = state("order");
    assert_eq!(load(&dir), vec![], "no file: nothing");
    remember(&dir, "https://a.test/", "A", 10).unwrap();
    remember(&dir, "https://b.test/x?q=1", "B", 20).unwrap();
    assert_eq!(load(&dir), vec![page("https://b.test/x?q=1", "B", 20), page("https://a.test/", "A", 10)]);
    remember(&dir, "https://a.test/", "A again", 30).unwrap();
    assert_eq!(load(&dir), vec![page("https://a.test/", "A again", 30), page("https://b.test/x?q=1", "B", 20)], "renewed, not doubled");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn at_most_a_hundred_pages_are_kept_and_the_oldest_go_first() {
    let dir = state("cap");
    for i in 0..(MAX_PAGES as u64 + 5) {
        remember(&dir, &format!("https://h{i}.test/"), "t", i).unwrap();
    }
    let pages = load(&dir);
    assert_eq!(pages.len(), MAX_PAGES);
    assert_eq!(pages[0].address, format!("https://h{}.test/", MAX_PAGES + 4), "the newest is first");
    assert_eq!(pages[MAX_PAGES - 1].address, "https://h5.test/", "the five oldest went");
    assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap().matches("\"address\"").count(), MAX_PAGES, "the file itself holds a hundred, so it stays under its size limit");
    let many: Vec<String> = (0..150).map(|i| format!("{{\"address\":\"https://m{i}.test/\",\"title\":\"t\",\"time\":{}}}", 1000 - i)).collect();
    put(&dir, &format!("{{\"v\":1,\"pages\":[{}]}}", many.join(",")), 0o600);
    let read = load(&dir);
    assert_eq!((read.len(), read[0].address.as_str()), (MAX_PAGES, "https://m0.test/"), "a longer file is read as its first hundred");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_a_web_address_is_remembered_and_a_title_is_cleaned_and_cut() {
    let dir = state("clean");
    for bad in ["", "example.com", "ftp://a.test/", "file://x/y", "javascript:alert(1)", "https://u:p@a.test/", "about:blank", "https://a b.test/", "https://", " https://a.test/"] {
        assert!(remember(&dir, bad, "t", 1).is_err(), "{bad:?} is not remembered");
    }
    assert_eq!(load(&dir), vec![]);
    remember(&dir, "https://a.test/", "  two\n\tlines\u{7}  ", 1).unwrap();
    remember(&dir, "https://b.test/", &"é".repeat(MAX_TITLE + 50), 2).unwrap();
    remember(&dir, "https://c.test/", "", 3).unwrap();
    remember(&dir, "HTTPS://D.test/x", "D", 4).unwrap();
    let pages = load(&dir);
    assert_eq!(pages[0].address, "https://D.test/x", "the scheme is written in lower case, as the model keeps it");
    let pages = &pages[1..];
    assert_eq!(pages[2].title, "two lines", "white space collapsed, control characters gone");
    assert_eq!(pages[1].title.chars().count(), MAX_TITLE, "cut at the cap, by characters");
    assert_eq!(pages[0].title, "", "an empty title stays empty");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_file_is_owner_only_and_a_missing_damaged_or_unsafe_file_reads_as_empty() {
    let dir = state("file");
    remember(&dir, "https://a.test/", "A", 1).unwrap();
    let file = dir.join(FILE);
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "mode 0600");
    let good = std::fs::read_to_string(&file).unwrap();
    for (what, text, mode) in [("damaged", "{not json".to_string(), 0o600), ("group-readable", good.clone(), 0o640), ("world-readable", good.clone(), 0o604), ("other version", good.replace("\"v\":1", "\"v\":2"), 0o600), ("too big", format!("{good}{}", " ".repeat(MAX_FILE as usize)), 0o600)] {
        put(&dir, &text, mode);
        assert_eq!(load(&dir), vec![], "{what} file");
    }
    put(&dir, &format!("{good}{}", " ".repeat(MAX_FILE as usize - good.len())), 0o600);
    assert_eq!(load(&dir).len(), 1, "a file of exactly the limit is read");
    put(&dir, &good, 0o600);
    assert_eq!(load(&dir).len(), 1, "the good file reads again");
    // a link is not followed
    let real = dir.join("real.json");
    std::fs::rename(&file, &real).unwrap();
    symlink(&real, &file).unwrap();
    assert_eq!(load(&dir), vec![], "a link says nothing");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_entry_of_the_file_that_is_not_valid_is_skipped_and_the_rest_is_read() {
    let dir = state("skip");
    let text = r#"{"v":1,"pages":[{"address":"https://a.test/","title":"A","time":5},{"address":"ftp://x/","title":"no","time":4},{"address":"https://b.test/","title":7,"time":3},{"address":"https://c.test/","title":"C"},{"address":"https://a.test/","title":"dup","time":2},{"address":"https://d.test/","title":"D","time":1}]}"#;
    put(&dir, text, 0o600);
    assert_eq!(load(&dir), vec![page("https://a.test/", "A", 5), page("https://d.test/", "D", 1)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_page_is_written_through_a_temporary_file_that_does_not_stay_and_over_a_bad_file() {
    let dir = state("write");
    put(&dir, "{damaged", 0o644);
    remember(&dir, "https://a.test/", "A", 1).unwrap();
    assert_eq!(std::fs::metadata(dir.join(FILE)).unwrap().permissions().mode() & 0o777, 0o600, "a loose file is replaced by an owner-only one");
    assert_eq!(load(&dir), vec![page("https://a.test/", "A", 1)]);
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec![FILE.to_string()], "no temporary file is left");
    // a place the file cannot be renamed onto: refused as `not_saved`, and the temporary file does not stay
    std::fs::remove_file(dir.join(FILE)).unwrap();
    std::fs::create_dir(dir.join(FILE)).unwrap();
    assert_eq!(remember(&dir, "https://b.test/", "B", 2), Err("not_saved"));
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec![FILE.to_string()], "the temporary file of the failed write is gone");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn forgetting_empties_the_list_says_how_many_there_were_and_removes_a_damaged_file_too() {
    let dir = state("forget");
    assert_eq!(forget(&dir), 0, "nothing to forget");
    remember(&dir, "https://a.test/", "A", 1).unwrap();
    remember(&dir, "https://b.test/", "B", 2).unwrap();
    assert_eq!(forget(&dir), 2);
    assert_eq!(load(&dir), vec![]);
    assert!(!dir.join(FILE).exists(), "the file is gone");
    put(&dir, "{damaged", 0o600);
    assert_eq!(forget(&dir), 0);
    assert!(!dir.join(FILE).exists(), "a damaged file is removed as well");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn many_threads_remembering_at_once_lose_nothing() {
    let dir = state("threads");
    let handles: Vec<_> = (0..8u64)
        .map(|t| {
            let dir = dir.clone();
            std::thread::spawn(move || {
                for i in 0..5u64 {
                    remember(&dir, &format!("https://t{t}-{i}.test/"), "t", t * 10 + i).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(load(&dir).len(), 40);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- M23-P08: no secret in a remembered address ----

#[test]
fn the_part_after_a_hash_is_never_kept_and_addresses_that_differ_only_there_are_one_page() {
    let dir = state("fragment");
    remember(&dir, "https://a.test/cb#access_token=SECRET123&type=bearer", "Signed in", 1).unwrap();
    remember(&dir, "https://a.test/cb#other", "Signed in", 2).unwrap();
    remember(&dir, "https://b.test/x?q=1#top", "B", 3).unwrap();
    remember(&dir, "https://c.test/#", "C", 4).unwrap();
    assert_eq!(load(&dir), vec![page("https://c.test/", "C", 4), page("https://b.test/x?q=1", "B", 3), page("https://a.test/cb", "Signed in", 2)], "cut, and the two of a.test are one entry");
    let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
    assert!(!text.contains("SECRET123") && !text.contains('#'), "nothing of the fragment reaches the file: {text}");
    assert_eq!(strip_fragment("https://a.test/x#top"), "https://a.test/x");
    assert_eq!(strip_fragment("https://a.test/x"), "https://a.test/x");
    assert_eq!(strip_fragment("https://a.test/x?q=#"), "https://a.test/x?q=");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_page_whose_address_carries_a_secret_by_name_is_not_remembered_at_all() {
    let dir = state("secret");
    for bad in [
        "https://a.test/cb?token=1",
        "https://a.test/cb?access_token=1",
        "https://a.test/cb?accessToken=1",
        "https://a.test/cb?id_token=1&x=2",
        "https://a.test/cb?x=1&API_KEY=2",
        "https://a.test/cb?api-key=2",
        "https://a.test/cb?apikey=2",
        "https://a.test/cb?code=4%2F0AX&state=s",
        "https://a.test/cb?client_secret=1",
        "https://a.test/cb?password=",
        "https://a.test/cb?Passwd=1",
        "https://a.test/cb?pass=1",
        "https://a.test/cb?sig=1",
        "https://a.test/cb?X-Amz-Signature=1",
        "https://a.test/cb?X-Amz-Security-Token=1",
        "https://a.test/cb?sid=1",
        "https://a.test/cb?sessionid=1",
        "https://a.test/cb?JSESSIONID=1",
        "https://a.test/cb?PHPSESSID=1",
        "https://a.test/cb?auth=1",
        "https://a.test/cb?authToken=1",
        "https://a.test/cb?magic=1",
        "https://a.test/cb?invite=1",
        "https://a.test/cb?ticket=ST-1",
        "https://a.test/cb?otp=123456",
        "https://a.test/cb?jwt=a.b.c",
        "https://a.test/cb?reset_password_token=1",
        "https://a.test/cb?to%6Ben=1",
        "https://a.test/cb?a=1;token=2",
        "https://a.test/cb?authCode=1",
        "https://a.test/cb?sessionKey=1",
        "https://a.test/cb?a=1&&token",
        "https://a.test/app;jsessionid=ABC123/next",
        "https://a.test/app;v=1;sessionid=ABC123",
    ] {
        assert_eq!(remember(&dir, bad, "t", 1), Err("secret_address"), "{bad} is a secret's address");
    }
    assert!(!dir.join(FILE).exists(), "not one of them was written");
    assert_eq!(remember(&dir, "ftp://a.test/?token=1", "t", 1), Err("bad_address"), "a bad address is still the other refusal");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ordinary_names_are_kept_because_a_name_is_split_into_words_not_searched_for_letters() {
    let dir = state("ordinary");
    let good = [
        "https://a.test/s?q=rust",
        "https://a.test/s?query=rust&page=2&lang=ko",
        "https://a.test/s?keyword=a",
        "https://a.test/s?author=b",
        "https://a.test/s?encoding=utf8",
        "https://a.test/s?design=1",
        "https://a.test/s?signup=1",
        "https://a.test/s?statement=1",
        "https://a.test/s?passenger=1",
        "https://a.test/s?zipcode=06000",
        "https://a.test/s?monkeys=1",
        "https://a.test/s?id=5&sort=new",
        "https://a.test/a;v=1/b",
        "https://a.test/token/page",
        "https://a.test/s?",
        "https://a.test/s?q=100%",
        "https://a.test/s?%6",
        "https://a.test/s?%",
        "https://a.test/s?tokenizer=1",
        "https://a.test/s?=x",
    ];
    for (i, g) in good.iter().enumerate() {
        remember(&dir, g, "t", i as u64).unwrap_or_else(|e| panic!("{g} is an ordinary address: {e}"));
    }
    assert_eq!(load(&dir).len(), good.len(), "all kept, each as it was written");
    assert_eq!(load(&dir)[0].address, "https://a.test/s?=x");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_of_an_older_run_has_its_secret_pages_hidden_and_its_fragments_cut_and_the_next_write_drops_them() {
    let dir = state("older");
    let text = r#"{"v":1,"pages":[
        {"address":"https://a.test/cb?token=SECRET123","title":"Reset","time":9},
        {"address":"https://b.test/x#access_token=SECRET456","title":"B","time":8},
        {"address":"https://b.test/x","title":"B plain","time":7},
        {"address":"https://c.test/","title":"C","time":6}]}"#;
    put(&dir, text, 0o600);
    assert_eq!(load(&dir), vec![page("https://b.test/x", "B", 8), page("https://c.test/", "C", 6)], "the secret page is not there; the fragment is cut and the same page is one entry");
    remember(&dir, "https://d.test/", "D", 10).unwrap();
    let written = std::fs::read_to_string(dir.join(FILE)).unwrap();
    assert!(!written.contains("SECRET"), "the next write leaves no secret in the file: {written}");
    assert_eq!(load(&dir).len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_word_of_the_list_and_every_ending_is_a_secrets_name() {
    let dir = state("words");
    // the list is spelt out here, not read from the program: a word changed or dropped there shows here
    for word in [
        "token", "secret", "password", "passwd", "pwd", "pass", "passcode", "auth", "authorization", "session", "sessionid", "sessid", "sid", "sig", "signature", "key", "apikey", "code", "otp", "jwt",
        "ticket", "credential", "credentials", "bearer", "magic", "invite", "csrf",
    ] {
        assert_eq!(remember(&dir, &format!("https://a.test/cb?{word}=1"), "t", 1), Err("secret_address"), "the word {word}");
        assert_eq!(remember(&dir, &format!("https://a.test/cb?{}=1", word.to_ascii_uppercase()), "t", 1), Err("secret_address"), "the word {word} in capitals");
        // not the last word of its name: only the list of words can see it here, not the endings
        assert_eq!(remember(&dir, &format!("https://a.test/cb?{word}_x=1"), "t", 1), Err("secret_address"), "the word {word} first in a name");
        assert_eq!(remember(&dir, &format!("https://a.test/cb?{}_X=1", word.to_ascii_uppercase()), "t", 1), Err("secret_address"), "the word {word} in capitals first in a name");
    }
    // a name written as one word that ends with one of the endings
    for name in ["accesstoken", "clientsecret", "newpassword", "userpasswd", "myapikey", "JSESSIONID", "PHPSESSID"] {
        assert_eq!(remember(&dir, &format!("https://a.test/cb?{name}=1"), "t", 1), Err("secret_address"), "the name {name}");
    }
    assert!(!dir.join(FILE).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_secret_that_stands_after_the_hash_goes_with_the_fragment_and_does_not_refuse_the_page() {
    let dir = state("route");
    remember(&dir, "https://d.test/app#/inbox?token=zzz", "App", 1).unwrap();
    assert_eq!(load(&dir), vec![page("https://d.test/app", "App", 1)], "a page that routes by its fragment is the page before it");
    let _ = std::fs::remove_dir_all(&dir);
}
