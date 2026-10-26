//! Which program this is, said the same way wherever a person can look: the window's title, Help > About, `lico --version` and the notice of a host replacement.

/// The commit this executable was built from (see build.rs): short hex, `-dirty` after it when the package had uncommitted changes, or `unknown`.
pub const COMMIT: &str = env!("LICO_BUILT_COMMIT");

/// `NAME VERSION (commit COMMIT, build BUILD)`: the same words for the window (`Lico`) and the command line (`lico`).
fn line_of(name: &str, version: &str, commit: &str, build: &str) -> String {
    format!("{name} {version} (commit {commit}, build {build})")
}

fn title_of(version: &str, commit: &str) -> String {
    format!("Lico {version} (commit {commit})")
}

/// "new build b6e6d218, was 237e404e": both builds by the first eight digits of their ids. A host that had no id, or one that is not hex digits, was
/// built before ids existed: "an older build", never an empty or a made-up id.
fn builds_of(new: &str, old: Option<&str>) -> String {
    let was = old.filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit())).map_or("an older build", |id| &id[..id.len().min(8)]);
    format!("new build {}, was {was}", &new[..new.len().min(8)])
}

/// The identity line of the window: Help > About.
pub fn window_line() -> String {
    line_of("Lico", env!("CARGO_PKG_VERSION"), COMMIT, crate::core::build_id())
}

/// The identity line of the command line: `lico --version`.
pub fn cli_line() -> String {
    line_of("lico", env!("CARGO_PKG_VERSION"), COMMIT, crate::core::build_id())
}

/// The title of the window.
pub fn title() -> String {
    title_of(env!("CARGO_PKG_VERSION"), COMMIT)
}

/// The two builds of a host replacement: this program's, and the id the old host reported (None: it reported none).
pub fn builds(old: Option<&str>) -> String {
    builds_of(crate::core::build_id(), old)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_line_names_version_commit_and_build_in_the_window_form_and_the_command_line_form() {
        assert_eq!(line_of("Lico", "0.0.0", "843cff6", "b6e6d218c8666a29"), "Lico 0.0.0 (commit 843cff6, build b6e6d218c8666a29)");
        assert_eq!(line_of("lico", "0.0.0", "843cff6", "b6e6d218c8666a29"), "lico 0.0.0 (commit 843cff6, build b6e6d218c8666a29)");
        assert_eq!(line_of("lico", "0.0.0", "843cff6-dirty", "b6e6d218c8666a29"), "lico 0.0.0 (commit 843cff6-dirty, build b6e6d218c8666a29)");
        assert_eq!(line_of("Lico", "0.0.0", "unknown", "b6e6d218c8666a29"), "Lico 0.0.0 (commit unknown, build b6e6d218c8666a29)");
    }

    #[test]
    fn the_title_has_the_version_and_the_commit_and_leaves_the_build_to_the_about_panel() {
        assert_eq!(title_of("0.0.0", "843cff6"), "Lico 0.0.0 (commit 843cff6)");
        assert_eq!(title_of("0.0.0", "843cff6-dirty"), "Lico 0.0.0 (commit 843cff6-dirty)");
        assert_eq!(title_of("0.0.0", "unknown"), "Lico 0.0.0 (commit unknown)");
    }

    #[test]
    fn the_notice_of_a_replacement_names_both_builds_by_eight_digits_and_a_host_without_an_id_is_an_older_build() {
        assert_eq!(builds_of("b6e6d218c8666a29", Some("237e404e11112222")), "new build b6e6d218, was 237e404e");
        assert_eq!(builds_of("b6e6d218c8666a29", None), "new build b6e6d218, was an older build");
        assert_eq!(builds_of("b6e6d218c8666a29", Some("")), "new build b6e6d218, was an older build");
        assert_eq!(builds_of("b6e6d218c8666a29", Some("\u{1b}[31m not an id")), "new build b6e6d218, was an older build", "an id is hex digits and nothing else");
        assert_eq!(builds_of("b6e6d218c8666a29", Some("ab12")), "new build b6e6d218, was ab12", "a short id is shown whole");
    }

    #[test]
    fn the_commit_that_is_baked_in_is_lower_case_hex_with_an_optional_dirty_mark_or_unknown() {
        let digits = COMMIT.strip_suffix("-dirty").unwrap_or(COMMIT);
        let hex = digits.len() >= 7 && digits.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
        assert!(hex || COMMIT == "unknown", "{COMMIT:?}");
    }
}
