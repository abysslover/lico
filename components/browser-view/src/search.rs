//! Words in the address bar (0.9.0): what a person typed is an address or a search (`classify`), the owner's template for a search is checked (`check_template`) and filled
//! (`search_url`). Pure: no file is read here (the host reads the setting and gives the template), no socket, no clock.

use crate::{hidden_format_char, parse_address, UrlError, MAX_URL};

/// The engine used when the owner has named none: the one the probe of M22-P05b found that the Moli engine can use (the results page opened, the Korean query arrived whole).
pub const DEFAULT_SEARCH: &str = "https://www.google.com/search?q=%s";
/// The longest template, in characters: what is left of an address is the room of the words.
pub const MAX_TEMPLATE: usize = 512;

/// What the bar's text is: an address to load, or the words to search for (white space collapsed, control and hidden characters dropped).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typed {
    Address(String),
    Search(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchError {
    /// The template is not `https://HOST...` with exactly one `%s` after the host part.
    Template,
    /// No words.
    Empty,
    /// The words, encoded in the template, are longer than `MAX_URL`.
    TooLong,
}

fn explicit_web_scheme(text: &str) -> bool {
    let starts = |prefix: &str| text.as_bytes().get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()));
    starts("http://") || starts("https://")
}

/// Whether an address taken by `parse_address` from text with no scheme and no space is one the person meant: a host with a dot, a port or an IPv6 number in it, or `localhost`
/// (the engine's fence explains those); a lone word (`lico`) is a search.
fn meant_as_address(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    authority.contains(['.', ':']) || authority.eq_ignore_ascii_case("localhost")
}

fn words(text: &str) -> Result<Typed, UrlError> {
    let spaced: String = text.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).filter(|c| !c.is_control() && !hidden_format_char(*c)).collect();
    let words = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.is_empty() {
        Err(UrlError::Empty)
    } else {
        Ok(Typed::Search(words))
    }
}

/// What a person typed in the bar of a tab that may search: an explicit `http://` or `https://` is an address (a bad one stays an error); another explicit scheme, and a scheme
/// with no space and no `//` (`javascript:`, `mailto:`, `data:`), is an error, never a search; a user name or password in the text, a text longer than `MAX_URL` and an empty text
/// are errors; text with a space is a search; text with no space is an address when `parse_address` takes it and it has a dot, a port or `localhost`, and a search when it is
/// one word with none of them or cannot be an address at all. (`parse_address` is asked first of every text: it answers `Empty` and `TooLong` before it looks further, and
/// answers `Invalid` for white space, which is how a text with a space becomes a search.)
pub fn classify(typed: &str) -> Result<Typed, UrlError> {
    let text = typed.trim();
    let parsed = parse_address(text);
    if explicit_web_scheme(text) {
        return parsed.map(Typed::Address);
    }
    match parsed {
        Ok(url) if meant_as_address(&url) => Ok(Typed::Address(url)),
        Ok(_) | Err(UrlError::Invalid) => words(text),
        Err(e) => Err(e),
    }
}

/// Whether `template` is an address the owner's search may use: `https://`, no white space (a control character is refused by `parse_address` below; white space at the end is
/// not, it trims), at most `MAX_TEMPLATE` characters, exactly one `%s` and it after the host part (the words never become the host or the port), and the template with a word in place
/// of `%s` is an address `parse_address` takes (no user name, a port in range).
pub fn check_template(template: &str) -> Result<(), SearchError> {
    let Some(rest) = template.strip_prefix("https://") else { return Err(SearchError::Template) };
    if template.chars().count() > MAX_TEMPLATE || template.chars().any(char::is_whitespace) || template.matches("%s").count() != 1 {
        return Err(SearchError::Template);
    }
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if rest.find("%s").is_some_and(|at| at < host_end) {
        return Err(SearchError::Template);
    }
    parse_address(&template.replace("%s", "x")).map(|_| ()).map_err(|_| SearchError::Template)
}

fn encode(words: &str) -> String {
    let mut out = String::with_capacity(words.len() * 3);
    for b in words.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The address of the search for `words` in `template`: the words percent-encoded (the unreserved `A-Za-z0-9-._~` as they are, every other byte of the UTF-8 text as `%XX`, a
/// space as `%20`) where `%s` is. A bad template is `Template` (said before the words are looked at), no words `Empty`, an address over `MAX_URL` characters `TooLong`.
pub fn search_url(template: &str, words: &str) -> Result<String, SearchError> {
    check_template(template)?;
    if words.is_empty() {
        return Err(SearchError::Empty);
    }
    let url = template.replacen("%s", &encode(words), 1);
    if url.chars().count() > MAX_URL {
        return Err(SearchError::TooLong);
    }
    Ok(url)
}
