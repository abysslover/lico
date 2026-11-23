//! M22-P05c: the remembered pages in the browser adapter: when a page is remembered, `browser.reopen` and `browser.forget`. The engine is never started: a tab with no engine
//! is in the `absent` phase and still has its address.

use super::ops::tests::world;
use super::ops::valid;
use super::pages_ops::{note, to_reopen};
use crate::driver::Ops;
use crate::pages::{load, remember, Page, FILE};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

fn req(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

fn state(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lico_pages_ops_{name}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_page_is_noted_only_when_it_has_loaded_in_a_tab_that_is_not_local() {
    let dir = state("note");
    assert_eq!(note(&dir, true, true, "https://a.test/", "A", 1), Err("local"), "a local tab never");
    assert_eq!(note(&dir, false, false, "https://a.test/", "A", 1), Err("not_ready"), "a page still loading never");
    assert_eq!(note(&dir, false, true, "about:blank", "", 1), Err("bad_address"));
    assert_eq!(note(&dir, false, true, "chrome-error://chromewebdata/", "x", 1), Err("bad_address"));
    assert!(!dir.join(FILE).exists(), "nothing was written by the refused ones");
    assert_eq!(note(&dir, false, true, "https://a.test/", "A", 7), Ok(()));
    assert_eq!(load(&dir), vec![Page { address: "https://a.test/".into(), title: "A".into(), time: 7 }]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_two_operations_take_no_fields_of_their_own() {
    for op in ["browser.reopen", "browser.forget"] {
        assert!(valid(op, &req(json!({"op": op}))));
        assert!(valid(op, &req(json!({"op": op, "v": 1, "id": "x"}))));
        for bad in [json!({"op": op, "url": "https://a.test/"}), json!({"op": op, "tab": "b1"}), json!({"op": op, "v": 2}), json!({"op": op, "id": ""})] {
            assert!(!valid(op, &req(bad.clone())), "{bad}");
        }
    }
}

#[test]
fn reopen_opens_the_newest_remembered_page_that_no_tab_shows_and_forget_empties_the_list() {
    let (_home, browser, shared) = world("pages_ops");
    let call = |op: &str, body: Value| {
        let mut r = req(body);
        r.insert("op".into(), json!(op));
        browser.call(&shared, op, &r)
    };
    let dir = browser.state_dir_of(&shared).unwrap();
    let urls = || call("browser.list", json!({})).unwrap()["tabs"].as_array().unwrap().iter().map(|t| t["url"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(call("browser.reopen", json!({})), Err("no_page"), "nothing remembered");
    remember(&dir, "https://a.test/", "A", 1).unwrap();
    remember(&dir, "https://b.test/", "B", 2).unwrap();
    remember(&dir, "https://c.test/", "C", 3).unwrap();
    // the newest first; a page a tab shows is passed over; the reply is the open reply with the address
    let first = call("browser.reopen", json!({})).unwrap();
    assert_eq!(first["url"], json!("https://c.test/"));
    assert!(first["tab"].as_str().is_some() && first["local"] == json!(false));
    assert_eq!(urls(), vec!["https://c.test/"]);
    assert_eq!(call("browser.reopen", json!({})).unwrap()["url"], json!("https://b.test/"), "c is shown, so b");
    assert_eq!(call("browser.reopen", json!({})).unwrap()["url"], json!("https://a.test/"));
    assert_eq!(call("browser.reopen", json!({})), Err("no_page"), "every remembered page is shown");
    assert_eq!(urls().len(), 3);
    // a closed tab can be reopened: the page is still in the list
    let b = call("browser.list", json!({})).unwrap()["tabs"].as_array().unwrap().iter().find(|t| t["url"] == json!("https://b.test/")).unwrap()["tab"].as_str().unwrap().to_string();
    call("browser.close", json!({"tab": b})).unwrap();
    assert_eq!(call("browser.reopen", json!({})).unwrap()["url"], json!("https://b.test/"));
    // a tab for this machine's own services does not hide a page of the same address
    call("browser.open", json!({"url": "https://d.test/", "local": true})).unwrap();
    remember(&dir, "https://d.test/", "D", 9).unwrap();
    assert_eq!(call("browser.reopen", json!({})).unwrap()["url"], json!("https://d.test/"));
    // forget
    assert_eq!(call("browser.forget", json!({})).unwrap()["forgotten"], json!(4));
    assert_eq!(load(&dir), vec![]);
    assert_eq!(call("browser.forget", json!({})).unwrap()["forgotten"], json!(0));
    assert_eq!(call("browser.reopen", json!({})), Err("no_page"));
}

// ---- M23-P08 ----

#[test]
fn a_page_with_a_secret_in_its_address_is_not_noted_and_the_fragment_is_not_kept() {
    let dir = state("secret_note");
    assert_eq!(note(&dir, false, true, "https://a.test/reset?token=abc", "Reset", 1), Err("secret_address"));
    assert!(!dir.join(FILE).exists(), "nothing was written");
    assert_eq!(note(&dir, false, true, "https://a.test/inbox#access_token=abc", "Inbox", 2), Ok(()));
    assert_eq!(load(&dir), vec![Page { address: "https://a.test/inbox".into(), title: "Inbox".into(), time: 2 }]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tab_that_shows_a_remembered_page_with_a_fragment_is_still_showing_it() {
    let list = vec![Page { address: "https://a.test/x".into(), title: "A".into(), time: 3 }, Page { address: "https://b.test/".into(), title: "B".into(), time: 2 }];
    let shown = vec!["https://a.test/x#top".to_string()];
    assert_eq!(to_reopen(&list, &shown).map(|p| p.address.as_str()), Some("https://b.test/"), "the page the tab shows is passed over although the tab says a fragment");
    assert_eq!(to_reopen(&list, &[]).map(|p| p.address.as_str()), Some("https://a.test/x"));
}
