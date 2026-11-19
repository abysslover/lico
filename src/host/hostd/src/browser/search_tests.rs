//! M22-P05b: words typed in the bar become the engine's search (the host's side): the owner's setting `browser_search` (read from the owner-only setting file, refused when it is
//! set and wrong), the address the host loads for what the bar held (`browser.go` and `browser.open` with `search`), and the fields the two operations accept. The engine is
//! never started: a tab with no engine is in the `absent` phase and still has its address.

use super::ops::tests::world;
use super::ops::valid;
use super::search::{address, template};
use crate::driver::Ops;
use serde_json::{json, Map, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const GOOGLE: &str = "https://www.google.com/search?q=";
const KOREAN: &str = "lico%20%EB%B8%8C%EB%9D%BC%EC%9A%B0%EC%A0%80";

fn req(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

fn state(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lico_browser_search_{name}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn setting(dir: &Path, text: &str, mode: u32) {
    let file = dir.join("browser.json");
    let _ = std::fs::remove_file(&file);
    std::fs::write(&file, text).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn the_template_is_the_owners_setting_or_google_and_a_wrong_one_is_refused() {
    let dir = state("template");
    assert_eq!(template(&dir), Ok(format!("{GOOGLE}%s")), "no file: the default");
    let bing = "https://www.bing.com/search?q=%s";
    setting(&dir, &json!({"browser_search": bing}).to_string(), 0o600);
    assert_eq!(template(&dir), Ok(bing.to_string()));
    setting(&dir, &json!({"engine_dir": "/x/engine", "browser_search": bing, "auto_install": false}).to_string(), 0o600);
    assert_eq!(template(&dir), Ok(bing.to_string()), "next to the other keys");
    for none in [r#"{"auto_install": false}"#, r#"{"engine_dir": "/x/engine"}"#, r#"{"stealth": true}"#, "{}", "not json", "", r#"[1]"#] {
        setting(&dir, none, 0o600);
        assert_eq!(template(&dir), Ok(format!("{GOOGLE}%s")), "{none:?} says nothing about the search");
    }
    for wrong in [json!("http://x.test/?q=%s"), json!("https://x.test/"), json!("https://%s.x.test/"), json!(""), json!(5), json!(true), json!(null), json!(["https://x.test/?q=%s"]), json!("https://x.test/?q=%s&r=%s")] {
        setting(&dir, &json!({"browser_search": wrong}).to_string(), 0o600);
        assert_eq!(template(&dir), Err("search_setting"), "{wrong}: a setting that is there and wrong is said, not skipped");
    }
    setting(&dir, &json!({"browser_search": bing}).to_string(), 0o644);
    assert_eq!(template(&dir), Ok(format!("{GOOGLE}%s")), "a file that others can read says nothing");
    setting(&dir, &json!({"browser_search": bing}).to_string(), 0o660);
    assert_eq!(template(&dir), Ok(format!("{GOOGLE}%s")), "a file that the group can write says nothing");
    let _ = std::fs::remove_file(dir.join("browser.json"));
    std::os::unix::fs::symlink("/dev/null", dir.join("browser.json")).unwrap();
    assert_eq!(template(&dir), Ok(format!("{GOOGLE}%s")), "a link says nothing");
}

#[test]
fn what_the_bar_held_becomes_an_address_or_the_engines_search() {
    let dir = state("address");
    let here = || Some(dir.clone());
    let a = |typed: &str, search: bool, local: bool| address(typed, search, local, here);
    assert_eq!(a("lico 브라우저", true, false), Ok(format!("{GOOGLE}{KOREAN}")));
    assert_eq!(a("lico", true, false), Ok(format!("{GOOGLE}lico")), "one word is a search");
    assert_eq!(a("docs.rs", true, false), Ok("https://docs.rs".to_string()), "an address stays one");
    assert_eq!(a("http://a.test/x", true, false), Ok("http://a.test/x".to_string()));
    assert_eq!(a("  ", true, false), Err("address_empty"));
    assert_eq!(a("ftp://x.test/", true, false), Err("address_scheme"));
    assert_eq!(a("javascript:alert(1)", true, false), Err("address_scheme"));
    assert_eq!(a("user@example.com", true, false), Err("address_credentials"));
    assert_eq!(a("https://exa mple.com", true, false), Err("address_invalid"));
    assert_eq!(a(&format!("a b{}", "c".repeat(2100)), true, false), Err("address_too_long"));
    assert_eq!(a(&format!("a {}", "브".repeat(300)), true, false), Err("address_too_long"), "the words fit the typed limit and not the address");
    // the strict ways: a request that does not say `search`, and a tab for this machine, parse as before
    assert_eq!(a("lico 브라우저", false, false), Err("address_invalid"));
    assert_eq!(a("lico 브라우저", true, true), Err("address_invalid"), "a local tab never searches");
    assert_eq!(a("lico", false, false), Ok("https://lico".to_string()), "strict: a word is a host, as it was");
    assert_eq!(a("lico", true, true), Ok("https://lico".to_string()));
    assert_eq!(a("docs.rs", false, true), Ok("https://docs.rs".to_string()));
    // the owner's engine, and a setting that is wrong: said when a search needs it and not otherwise
    setting(&dir, r#"{"browser_search": "https://search.test/find?q=%s&hl=ko"}"#, 0o600);
    assert_eq!(a("red fox", true, false), Ok("https://search.test/find?q=red%20fox&hl=ko".to_string()));
    setting(&dir, r#"{"browser_search": "http://search.test/find?q=%s"}"#, 0o600);
    assert_eq!(a("red fox", true, false), Err("search_setting"));
    assert_eq!(a("docs.rs", true, false), Ok("https://docs.rs".to_string()), "an address needs no search engine");
    assert_eq!(a("red fox", false, false), Err("address_invalid"), "strict does not read the setting");
    assert_eq!(a("red fox", true, true), Err("address_invalid"), "a local tab does not read the setting");
    assert_eq!(address("red fox", true, false, || None), Err("engine_failed"), "no state folder, no setting to read");
    assert_eq!(address("docs.rs", true, false, || None), Ok("https://docs.rs".to_string()), "an address does not ask for the folder");
    assert_eq!(address("red fox", false, false, || panic!("strict must not ask for the folder")), Err("address_invalid"));
}

#[test]
fn the_two_operations_take_a_search_flag_and_only_a_boolean() {
    assert!(valid("browser.go", &req(json!({"op": "browser.go", "tab": "b1", "address": "a b", "search": true}))));
    assert!(valid("browser.go", &req(json!({"op": "browser.go", "tab": "b1", "address": "a b", "search": false}))));
    assert!(valid("browser.go", &req(json!({"op": "browser.go", "tab": "b1", "address": "a b"}))));
    assert!(valid("browser.open", &req(json!({"op": "browser.open", "url": "a b", "search": true, "local": false}))));
    assert!(valid("browser.open", &req(json!({"op": "browser.open", "url": "a b"}))));
    for (op, bad) in [
        ("browser.go", json!({"op": "browser.go", "tab": "b1", "address": "a b", "search": "yes"})),
        ("browser.go", json!({"op": "browser.go", "tab": "b1", "address": "a b", "search": 1})),
        ("browser.open", json!({"op": "browser.open", "url": "a b", "search": "yes"})),
        ("browser.open", json!({"op": "browser.open", "url": "a b", "search": null})),
        ("browser.nav", json!({"op": "browser.nav", "tab": "b1", "action": "back", "search": true})),
        ("browser.close", json!({"op": "browser.close", "tab": "b1", "search": true})),
    ] {
        assert!(!valid(op, &req(bad.clone())), "{bad}");
    }
}

#[test]
fn open_and_go_with_search_load_the_engines_address_and_say_why_when_they_cannot() {
    let (_home, browser, shared) = world("search_ops");
    let call = |op: &str, body: Value| {
        let mut r = req(body);
        r.insert("op".into(), json!(op));
        browser.call(&shared, op, &r)
    };
    let rows = || call("browser.list", json!({})).unwrap()["tabs"].as_array().unwrap().clone();
    // a search from the new-tab prompt
    let tab = call("browser.open", json!({"url": "lico 브라우저", "search": true})).unwrap()["tab"].as_str().unwrap().to_string();
    assert_eq!(rows()[0]["url"], json!(format!("{GOOGLE}{KOREAN}")));
    // the same words with no flag, as the command line sends them, are refused as before
    assert_eq!(call("browser.open", json!({"url": "lico 브라우저"})), Err("bad_request"));
    assert_eq!(call("browser.open", json!({"url": "ftp://x.test/"})), Err("bad_request"), "strict: the old code");
    assert_eq!(call("browser.open", json!({"url": "ftp://x.test/", "search": true})), Err("address_scheme"), "a window that searches is told why");
    assert_eq!(call("browser.open", json!({"url": "lico 브라우저", "search": true, "local": true})), Err("address_invalid"), "a local tab never searches");
    assert_eq!(rows().len(), 1, "nothing was opened by the refused ones");
    // a search in a tab that is open
    let go = |tab: &str, address: &str, search: bool| call("browser.go", json!({"tab": tab, "address": address, "search": search}));
    assert_eq!(go(&tab, "red fox", true).unwrap()["url"], json!(format!("{GOOGLE}red%20fox")));
    assert_eq!(rows()[0]["url"], json!(format!("{GOOGLE}red%20fox")));
    assert_eq!(go(&tab, "red fox", false), Err("address_invalid"), "no flag, strict");
    assert_eq!(go(&tab, "docs.rs", true).unwrap()["url"], json!("https://docs.rs"));
    assert_eq!(go(&tab, "ftp://x.test/", true), Err("address_scheme"));
    assert_eq!(go("b99", "red fox", true), Err("unknown_tab"));
    // a tab for this machine's own services
    let local = call("browser.open", json!({"url": "http://a.test/", "local": true})).unwrap()["tab"].as_str().unwrap().to_string();
    assert_eq!(go(&local, "red fox", true), Err("address_invalid"), "a local tab keeps the old sentence");
    assert_eq!(go(&local, "a.test", true).unwrap()["url"], json!("https://a.test"));
    // the owner's engine, and a setting that is wrong
    let dir = browser.state_dir_of(&shared).unwrap();
    setting(&dir, r#"{"auto_install": false, "browser_search": "https://search.test/find?q=%s"}"#, 0o600);
    assert_eq!(go(&tab, "red fox", true).unwrap()["url"], json!("https://search.test/find?q=red%20fox"));
    setting(&dir, r#"{"auto_install": false, "browser_search": "nonsense"}"#, 0o600);
    assert_eq!(go(&tab, "red fox", true), Err("search_setting"));
    assert_eq!(call("browser.open", json!({"url": "red fox", "search": true})), Err("search_setting"));
    assert_eq!(go(&tab, "docs.rs", true).unwrap()["url"], json!("https://docs.rs"));
}
