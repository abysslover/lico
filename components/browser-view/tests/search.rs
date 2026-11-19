//! The contract tests of the search half of `lico-browser-view` 0.9.0 (SPEC.md, "Words in the address bar"): what a person typed in the bar is an address or a search (`classify`),
//! the owner's template for a search is checked (`check_template`) and filled (`search_url`). Pure: no file, no socket, no clock.

use lico_browser_view::*;

fn address(url: &str) -> Result<Typed, UrlError> {
    Ok(Typed::Address(url.to_string()))
}

fn search(words: &str) -> Result<Typed, UrlError> {
    Ok(Typed::Search(words.to_string()))
}

const T: &str = "https://x.test/?q=%s";

#[test]
fn what_looks_like_an_address_is_an_address() {
    for (typed, want) in [
        ("https://example.com", "https://example.com"),
        ("HTTPS://Example.com/A", "https://Example.com/A"),
        ("http://example.com/a?b=c#d", "http://example.com/a?b=c#d"),
        ("example.com", "https://example.com"),
        ("docs.rs/lico", "https://docs.rs/lico"),
        ("  example.com  ", "https://example.com"),
        ("localhost", "https://localhost"),
        ("LocalHost/x", "https://LocalHost/x"),
        ("localhost:8080", "https://localhost:8080"),
        ("host:8080", "https://host:8080"),
        ("127.0.0.1", "https://127.0.0.1"),
        ("[::1]", "https://[::1]"),
        ("[::1]:8443/a", "https://[::1]:8443/a"),
        ("example.com?q=a", "https://example.com?q=a"),
        ("example.com#top", "https://example.com#top"),
        ("https://intranet", "https://intranet"),
        ("http://word", "http://word"),
        ("HTTP://word", "http://word"),
        ("Https://Intranet/x", "https://Intranet/x"),
    ] {
        assert_eq!(classify(typed), address(want), "{typed}");
    }
}

#[test]
fn text_with_a_space_or_no_address_in_it_is_a_search() {
    for (typed, words) in [
        ("lico", "lico"),
        ("LICO", "LICO"),
        ("lico 브라우저", "lico 브라우저"),
        ("브라우저", "브라우저"),
        ("c++", "c++"),
        ("what is rust?", "what is rust?"),
        ("example.com is down", "example.com is down"),
        ("https example.com", "https example.com"),
        ("javascript: alert(1)", "javascript: alert(1)"),
        ("a<b", "a<b"),
        ("foo\\bar", "foo\\bar"),
        ("[abc", "[abc"),
        ("a/b", "a/b"),
        ("what?", "what?"),
        ("note#1", "note#1"),
        ("note?v=1.5", "note?v=1.5"),
        ("note/a.b", "note/a.b"),
        ("note#a.b", "note#a.b"),
        ("note?v=a:b", "note?v=a:b"),
    ] {
        assert_eq!(classify(typed), search(words), "{typed}");
    }
}

#[test]
fn the_words_are_cleaned_before_they_are_used() {
    assert_eq!(classify("  lico   브라우저  "), search("lico 브라우저"), "white space is one space");
    assert_eq!(classify("lico\tbrowser\nview"), search("lico browser view"), "a tab and a line break are spaces");
    assert_eq!(classify("lico\u{00A0}browser"), search("lico browser"), "a no-break space is a space");
    assert_eq!(classify("lico\u{2028}browser"), search("lico browser"), "a line separator is a space");
    assert_eq!(classify("li\u{0}co"), search("lico"), "a control character is dropped");
    assert_eq!(classify("li\u{202E}co x"), search("lico x"), "a direction override is dropped");
    assert_eq!(classify("li\u{200B}co x"), search("lico x"), "a zero-width space is dropped");
    assert_eq!(classify("\u{200B} \u{200B}"), Err(UrlError::Empty), "nothing is left to search");
}

#[test]
fn an_address_that_is_wrong_is_an_error_and_never_a_search() {
    for (typed, why) in [
        ("", UrlError::Empty),
        ("   ", UrlError::Empty),
        ("\t\n", UrlError::Empty),
        ("ftp://x.test/", UrlError::Scheme),
        ("file:///etc/hosts", UrlError::Scheme),
        ("javascript:alert(1)", UrlError::Scheme),
        ("mailto:a@b.test", UrlError::Scheme),
        ("data:text/html,x", UrlError::Scheme),
        ("about:blank", UrlError::Scheme),
        ("https://user:pw@example.com", UrlError::Credentials),
        ("user@example.com", UrlError::Credentials),
        ("https://exa mple.com", UrlError::Invalid),
        ("HTTPS://exa mple.com", UrlError::Invalid),
        ("  https://exa mple.com  ", UrlError::Invalid),
        ("Http://a b", UrlError::Invalid),
        ("http://", UrlError::Invalid),
        ("https://x.test:99999/", UrlError::Invalid),
    ] {
        assert_eq!(classify(typed), Err(why), "{typed:?}");
    }
    assert_eq!(classify(&"a".repeat(MAX_URL + 1)), Err(UrlError::TooLong));
    assert_eq!(classify(&format!("a b{}", "c".repeat(MAX_URL))), Err(UrlError::TooLong), "words too long to take are not cut");
    assert!(classify(&format!("a {}", "c".repeat(MAX_URL - 2))).is_ok(), "exactly the cap is taken");
}

#[test]
fn the_default_search_is_google_and_is_a_valid_template() {
    assert_eq!(DEFAULT_SEARCH, "https://www.google.com/search?q=%s");
    assert_eq!(check_template(DEFAULT_SEARCH), Ok(()));
    // the two other engines of the probe of M22-P05b are valid templates as well: they are what the owner names if the default is ever blocked
    for t in ["https://html.duckduckgo.com/html/?q=%s", "https://www.bing.com/search?q=%s"] {
        assert_eq!(check_template(t), Ok(()), "{t}");
    }
}

#[test]
fn the_words_are_percent_encoded_into_the_template() {
    let google = |w: &str| search_url(DEFAULT_SEARCH, w);
    assert_eq!(google("lico"), Ok("https://www.google.com/search?q=lico".to_string()));
    assert_eq!(google("lico 브라우저"), Ok("https://www.google.com/search?q=lico%20%EB%B8%8C%EB%9D%BC%EC%9A%B0%EC%A0%80".to_string()));
    assert_eq!(google("AZaz09-._~"), Ok("https://www.google.com/search?q=AZaz09-._~".to_string()), "the unreserved characters stay");
    assert_eq!(google("a&b=c#d/e?f+g%h"), Ok("https://www.google.com/search?q=a%26b%3Dc%23d%2Fe%3Ff%2Bg%25h".to_string()));
    assert_eq!(google("\"<>'`{}|\\^[]@:;,!*()$"), Ok("https://www.google.com/search?q=%22%3C%3E%27%60%7B%7D%7C%5C%5E%5B%5D%40%3A%3B%2C%21%2A%28%29%24".to_string()));
    assert_eq!(google("a\nb\u{0}"), Ok("https://www.google.com/search?q=a%0Ab%00".to_string()));
    assert_eq!(google("😀"), Ok("https://www.google.com/search?q=%F0%9F%98%80".to_string()));
    assert_eq!(search_url("https://x.test/s?q=%s&hl=ko", "a b"), Ok("https://x.test/s?q=a%20b&hl=ko".to_string()), "the words go where %s is, and the rest of the template stays");
    assert_eq!(search_url("https://x.test/find/%s/page", "a b"), Ok("https://x.test/find/a%20b/page".to_string()));
    assert_eq!(search_url("https://x.test?q=%s", "a"), Ok("https://x.test?q=a".to_string()), "the host part ends at the question mark");
    assert_eq!(search_url("https://x.test:8443/s?q=%s", "a"), Ok("https://x.test:8443/s?q=a".to_string()));
    assert_eq!(search_url("https://x.test/s?q=%20%s", "a"), Ok("https://x.test/s?q=%20a".to_string()), "another percent sequence in the template stays as written");
}

#[test]
fn words_can_never_change_the_host_or_what_the_address_is() {
    let pool = ["a", "Z", "0", " ", "/", "?", "#", "@", ":", "\\", "[", "]", "%", "%s", "%25", ".", "..", "=", "&", "+", "브", "😀", "\n", "\u{0}", "\u{202E}", "'", "\"", "<", ">"];
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for round in 0..2000 {
        let n = 1 + (next() % 12) as usize;
        let words: String = (0..n).map(|_| pool[(next() % pool.len() as u64) as usize]).collect();
        for template in [DEFAULT_SEARCH, "https://x.test:8443/a/%s/b?c=d", "https://x.test?q=%s"] {
            let url = search_url(template, &words).unwrap();
            let head = &template[..template.find("%s").unwrap()];
            let tail = &template[head.len() + 2..];
            assert!(url.starts_with(head) && url.ends_with(tail), "round {round}: {words:?} -> {url}");
            let encoded = &url[head.len()..url.len() - tail.len()];
            assert!(encoded.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'%')), "round {round}: only safe characters stand where the words went: {url}");
            assert_eq!(parse_address(&url).as_deref(), Ok(url.as_str()), "round {round}: still one address: {url}");
        }
    }
}

#[test]
fn a_template_that_could_send_the_words_somewhere_else_is_refused() {
    for (template, why) in [
        ("", "empty"),
        ("https://x.test/", "no %s"),
        ("https://x.test/?q=%s&r=%s", "two %s"),
        ("http://x.test/?q=%s", "http: the words would go in the clear"),
        ("HTTPS://x.test/?q=%s", "the scheme is lower case"),
        ("x.test/?q=%s", "no scheme"),
        ("ftp://x.test/?q=%s", "ftp"),
        ("https://%s.x.test/", "the words would be the host"),
        ("https://x.%s/", "the words would be the host"),
        ("https://x.test%s", "the words would end the host"),
        ("https://x.test:%s/", "the words would be the port"),
        ("https://u@x.test/?q=%s", "a user name"),
        ("https://u:p@x.test/?q=%s", "a user name and password"),
        ("https://@x.test/?q=%s", "an empty user name"),
        ("https:///?q=%s", "no host"),
        ("https://x.test:99999/?q=%s", "a port out of range"),
        ("https://x.test/ ?q=%s", "a space"),
        ("https://x.test/?q=%s\n", "a line break"),
        ("https://x.test/\u{0}?q=%s", "a control character"),
        ("https://x.test/\u{2028}?q=%s", "a line separator"),
    ] {
        assert_eq!(check_template(template), Err(SearchError::Template), "{template:?}: {why}");
        assert_eq!(search_url(template, "a"), Err(SearchError::Template), "{template:?}: {why}");
    }
    let long = format!("https://x.test/{}?q=%s", "a".repeat(MAX_TEMPLATE));
    assert_eq!(check_template(&long), Err(SearchError::Template), "a template over the cap");
    let exact = format!("https://x.test/{}%s", "a".repeat(MAX_TEMPLATE - "https://x.test/%s".len()));
    assert_eq!(exact.chars().count(), MAX_TEMPLATE);
    assert_eq!(check_template(&exact), Ok(()), "exactly the cap");
}

#[test]
fn an_empty_search_and_one_that_does_not_fit_are_refused() {
    assert_eq!(search_url(T, ""), Err(SearchError::Empty));
    let fixed = "https://x.test/?q=".len();
    assert_eq!(search_url(T, &"a".repeat(MAX_URL - fixed)).map(|u| u.len()), Ok(MAX_URL), "exactly the cap");
    assert_eq!(search_url(T, &"a".repeat(MAX_URL - fixed + 1)), Err(SearchError::TooLong), "one over the cap");
    assert_eq!(search_url(T, &"브".repeat(226)), Err(SearchError::TooLong), "the encoding is counted: 226 Korean characters are 2,034 characters of words in an address of 18 more");
    assert!(search_url(T, &"브".repeat(225)).is_ok());
    assert_eq!(search_url("bad", &"a".repeat(MAX_URL * 2)), Err(SearchError::Template), "a bad template is said before the length");
}

#[test]
fn what_the_bar_takes_goes_through_the_same_door() {
    // the window's text, as the host gets it: classify, then the template; the search of a typed text is an address `parse_address` takes as it is
    for typed in ["lico 브라우저", "lico", "what is rust?", "a<b"] {
        let Ok(Typed::Search(words)) = classify(typed) else { panic!("{typed}") };
        let url = search_url(DEFAULT_SEARCH, &words).unwrap();
        assert_eq!(parse_address(&url).as_deref(), Ok(url.as_str()), "{typed}");
    }
}
