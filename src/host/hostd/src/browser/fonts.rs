//! The font directory for Korean (mail 147 item 6): the first directory of a short fixed list that exists and holds a Korean font by name. No recursion, no `HOME` fonts.

use std::path::{Path, PathBuf};

/// System directories that usually hold a Korean font, in the order they are tried.
pub const DIRS: [&str; 5] = ["/usr/share/fonts/truetype/nanum", "/usr/share/fonts/truetype/noto", "/usr/share/fonts/opentype/noto", "/usr/share/fonts/truetype/unfonts-core", "/usr/share/fonts/truetype/baekmuk"];

/// Whether a file name is that of a font with Korean letters: Nanum, Baekmuk, Noto CJK, or the Un families.
pub fn is_korean_font(file_name: &str) -> bool {
    let n = file_name.to_ascii_lowercase();
    let font = n.ends_with(".ttf") || n.ends_with(".otf") || n.ends_with(".ttc");
    font && (n.contains("nanum") || n.contains("baekmuk") || (n.contains("noto") && n.contains("cjk")) || ["unbatang", "undotum", "ungraphic", "unpilgia", "unshinmun", "unpen"].iter().any(|u| n.starts_with(u)))
}

/// The first of `dirs` for which `list` gives a Korean font file name (`list` is the directory listing; a test gives its own).
pub fn find(dirs: &[&str], list: &dyn Fn(&Path) -> Vec<String>) -> Option<PathBuf> {
    dirs.iter().map(Path::new).find(|d| list(d).iter().any(|n| is_korean_font(n))).map(Path::to_path_buf)
}

/// The system's own directories, read once (not recursively).
pub fn find_on_this_machine() -> Option<PathBuf> {
    find(&DIRS, &|d| std::fs::read_dir(d).map(|r| r.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn korean_fonts_are_known_by_name_and_other_fonts_are_not() {
        for yes in ["NanumGothic.ttf", "NanumBarunGothic.ttf", "NotoSansCJK-Regular.ttc", "NotoSerifCJKkr-Regular.otf", "UnBatang.ttf", "UnDotum.ttf", "baekmuk-batang.ttf", "NANUMGOTHIC.TTF"] {
            assert!(is_korean_font(yes), "{yes}");
        }
        for no in ["DejaVuSans.ttf", "NotoSans-Regular.ttf", "NotoSansMono.ttf", "Unifont.ttf", "NanumGothic.txt", "nanum", "", "Liberation-Regular.ttf", "UnknownDisplay.ttf"] {
            assert!(!is_korean_font(no), "{no}");
        }
    }

    #[test]
    fn the_first_directory_with_a_korean_font_wins_and_none_is_none() {
        let list = |d: &Path| match d.to_str().unwrap() {
            "/usr/share/fonts/truetype/nanum" => vec!["README".to_string()],
            "/usr/share/fonts/truetype/noto" => vec!["NotoSans-Regular.ttf".into()],
            "/usr/share/fonts/opentype/noto" => vec!["NotoSansCJK-Regular.ttc".into()],
            "/usr/share/fonts/truetype/baekmuk" => vec!["baekmuk-batang.ttf".into()],
            _ => Vec::new(),
        };
        assert_eq!(find(&DIRS, &list), Some(PathBuf::from("/usr/share/fonts/opentype/noto")), "a directory without a Korean font is passed over");
        assert_eq!(find(&DIRS[..3], &|_: &Path| Vec::new()), None);
        assert_eq!(find(&[], &list), None);
        assert!(DIRS.iter().all(|d| d.starts_with("/usr/share/fonts/")), "system directories only: no HOME, no network mount");
    }
}
