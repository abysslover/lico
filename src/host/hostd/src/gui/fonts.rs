//! Fonts for the transcript. A line cost about a millisecond to shape because, with the family `monospace`, cosmic-text goes through
//! every monospace face for each word and scans all installed faces again for each word the first font lacks. The face a character
//! lands on never changes, so the first monospace face is named once and each character that it lacks is looked up once. That is only
//! sound where the shape of a character does not depend on its neighbours, so a line takes this path only if every character of it is
//! on a whitelist of such characters (default-deny); any other line is shaped whole with `monospace`, the old search. Inside such a line
//! the decision is made per word, the unit that cosmic-text shapes as one run: a word of which every character is on the first monospace face
//! needs no span, a word of one character is given its face by name, and any other word (one character off the first face among several, or one
//! that does not check out) is left to the old search for that word alone.

use std::collections::HashMap;

use cosmic_text::{fontdb, Attrs, AttrsList, Buffer, Family, FontSystem, Metrics, Shaping, Stretch, Style, Weight};

type Face = (String, Weight, Style, Stretch);

const UNKNOWN: u16 = u16::MAX;

#[derive(Default)]
pub struct Faces {
    pub plain: bool, // the old search for everything (measurement client only), so a test can compare pixels
    prepared: bool,
    primary: Option<Face>, // the first monospace face
    named_base: bool,      // lines are shaped with that face by name, which gives the same glyphs as `monospace` for printable ASCII
    named: Vec<Face>,
    by_char: HashMap<char, u16>, // 0: the first monospace face, n: named[n - 1], UNKNOWN: left to the old search
    scratch: Option<Buffer>,
    pub lines: u64,
    pub probes: u64,
    pub exact: u64, // lines given whole to the old search because a character in them is not known to be independent of its neighbours
    pub w_base: u64,   // words that are all on the first monospace face: nothing to add
    pub w_single: u64, // words of one character, named by the face that `face_of` found for it
    pub w_old: u64,    // words of several characters of which one is on another face (or has none): the old search, for this word only
}

/// The words that cosmic-text 0.19 shapes (`ShapeSpan::build`): the stretches between the UAX #14 break opportunities of the line, each with its
/// trailing white space split off as words of one character. A break between two ASCII punctuation characters is not taken here (cosmic-text
/// takes it unless the pair makes a ligature), so a unit here is never smaller than a word there. Every word is shaped as ONE run when it has
/// one set of attributes, which is what the old search does, so a decision per word keeps the unit of shaping.
fn words(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start_word = 0;
    for (end_lb, _) in unicode_linebreak::linebreaks(text) {
        if end_lb > 0 && end_lb < text.len() {
            if let (Some(a), Some(b)) = (text[..end_lb].chars().last(), text[end_lb..].chars().next()) {
                if a.is_ascii_punctuation() && b.is_ascii_punctuation() {
                    continue;
                }
            }
        }
        let mut start_lb = end_lb;
        for (i, c) in text[start_word..end_lb].char_indices().rev() {
            if c.is_whitespace() {
                start_lb = start_word + i;
            } else {
                break;
            }
        }
        if start_word < start_lb {
            out.push((start_word, start_lb));
        }
        for (i, c) in text[start_lb..end_lb].char_indices() {
            out.push((start_lb + i, start_lb + i + c.len_utf8()));
        }
        start_word = end_lb;
    }
    out
}

/// The font list of the installed fonts, with faces that share family, weight, style and stretch given distinct family names
/// (otherwise a name does not pick one face: three "Unifont Sample" faces, one of them with the emoji).
pub fn system(plain: bool) -> FontSystem {
    let fs = FontSystem::new(); // scans the installed fonts
    if plain {
        return fs;
    }
    let (locale, mut db) = fs.into_locale_and_db();
    let mut seen: HashMap<(String, u16, u8, u16), u32> = HashMap::new();
    let mut renamed = Vec::new();
    for f in db.faces() {
        let n = seen.entry((f.families[0].0.clone(), f.weight.0, f.style as u8, f.stretch as u16)).or_insert(0);
        *n += 1;
        if *n > 1 {
            renamed.push((f.id, *n));
        }
    }
    for (id, n) in renamed {
        let mut info = db.face(id).expect("face").clone();
        db.remove_face(id);
        for family in info.families.iter_mut() {
            family.0 = format!("{} ~{n}", family.0);
        }
        db.push_face_info(info);
    }
    FontSystem::new_with_locale_and_db(locale, db)
}

/// Characters whose face and shape do not depend on their neighbours: Latin, Greek, Cyrillic, punctuation, symbols, kana, ideographs, Hangul syllables,
/// full-width forms, emoji. A line made only of these is split into spans by character; a line with any other character (right-to-left, joined, stacked,
/// conjoining or combining text, joiners and other zero-width or directional controls, variation selectors, flags, skin tones, tags, private use, unassigned)
/// is shaped whole with the family `monospace`, the old search, so it draws what it always drew.
const INDEPENDENT: [(u32, u32); 22] = [
    (0x20, 0x7E), (0xA0, 0xAC), (0xAE, 0x2FF), (0x370, 0x482), (0x48A, 0x52F), (0x1E00, 0x1FFF), (0x2010, 0x2027), (0x2030, 0x205E), (0x2070, 0x20CF),
    (0x2100, 0x2BFF), (0x3000, 0x3029), (0x3030, 0x3098), (0x309B, 0x33FF), (0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0xAC00, 0xD7A3), (0xF900, 0xFAFF),
    (0xFF01, 0xFFEF), (0x1F000, 0x1F1E5), (0x1F200, 0x1F3FA), (0x1F400, 0x1FAFF), (0x20000, 0x2FA1F),
];

fn independent(c: char) -> bool {
    let c = c as u32;
    INDEPENDENT.iter().any(|&(low, high)| (low..=high).contains(&c))
}

impl Faces {
    pub fn new(plain: bool) -> Self {
        Faces { plain, ..Default::default() }
    }

    /// The attributes of a line: the face of the first monospace font by name, and spans for the characters that it lacks.
    pub fn line(&mut self, fs: &mut FontSystem, text: &str) -> AttrsList {
        self.lines += 1;
        if self.plain {
            return AttrsList::new(&Attrs::new().family(Family::Monospace));
        }
        self.prepare(fs);
        let base = self.base();
        let mut list = AttrsList::new(&base);
        if text.bytes().all(|b| (0x20..0x7F).contains(&b)) {
            return list;
        }
        if !text.chars().all(independent) {
            return self.whole();
        }
        // one decision per word: all on the first monospace face -> nothing; one character -> its face by name (the old search ranks the faces by
        // this one character, which is what `face_of` asked); anything else -> the old search for this word
        for (start, end) in words(text) {
            let (mut n, mut all_primary, mut last) = (0usize, true, 0u16);
            for c in text[start..end].chars() {
                let k = if c.is_ascii() { 0 } else { self.face_of(fs, c) };
                n += 1;
                last = k;
                all_primary &= k == 0;
            }
            if all_primary {
                self.w_base += 1;
            } else if n == 1 && last != UNKNOWN {
                self.w_single += 1;
                self.span(&mut list, Some((start, end, last)));
            } else {
                self.w_old += 1;
                list.add_span(start..end, &Attrs::new().family(Family::Monospace));
            }
        }
        list
    }

    /// The old search for a whole line.
    fn whole(&mut self) -> AttrsList {
        self.exact += 1;
        AttrsList::new(&Attrs::new().family(Family::Monospace))
    }

    fn base(&self) -> Attrs<'_> {
        match &self.primary {
            Some(face) if self.named_base => Attrs::new().family(Family::Name(&face.0)).weight(face.1).style(face.2).stretch(face.3),
            _ => Attrs::new().family(Family::Monospace),
        }
    }

    fn span(&self, list: &mut AttrsList, run: Option<(usize, usize, u16)>) {
        match run {
            Some((start, end, k)) if k != 0 => {
                let (name, weight, style, stretch) = &self.named[k as usize - 1];
                list.add_span(start..end, &Attrs::new().family(Family::Name(name)).weight(*weight).style(*style).stretch(*stretch));
            }
            _ => {}
        }
    }

    fn prepare(&mut self, fs: &mut FontSystem) {
        if self.prepared {
            return;
        }
        self.prepared = true;
        self.scratch = Some(Buffer::new(fs, Metrics::new(15.0, 18.0)));
        let query = fontdb::Query { families: &[fontdb::Family::Monospace], ..Default::default() };
        self.primary = fs.db().query(&query).and_then(|id| fs.db().face(id).map(|f| (f.families[0].0.clone(), f.weight, f.style, f.stretch)));
        let Some(face) = self.primary.clone() else { return };
        // the name must give what `monospace` gives for every printable ASCII character
        let ascii: String = (0x20u8..0x7F).map(char::from).collect();
        let by_family = self.glyphs(fs, &ascii, &Attrs::new().family(Family::Monospace));
        let by_name = self.glyphs(fs, &ascii, &Attrs::new().family(Family::Name(&face.0)).weight(face.1).style(face.2).stretch(face.3));
        self.named_base = by_family.len() == ascii.len() && by_family == by_name && by_family.iter().all(|g| g.1 != 0);
    }

    fn glyphs(&mut self, fs: &mut FontSystem, text: &str, attrs: &Attrs) -> Vec<(fontdb::ID, u16, u32)> {
        let scratch = self.scratch.as_mut().expect("scratch");
        scratch.set_text(text, attrs, Shaping::Advanced, None);
        scratch.shape_until_scroll(fs, true);
        scratch.layout_runs().flat_map(|r| r.glyphs.iter().map(|g| (g.font_id, g.glyph_id, g.w.to_bits())).collect::<Vec<_>>()).collect()
    }

    fn font_of(&mut self, fs: &mut FontSystem, text: &str, attrs: &Attrs) -> Option<fontdb::ID> {
        let scratch = self.scratch.as_mut().expect("scratch");
        scratch.set_text(text, attrs, Shaping::Advanced, None);
        scratch.shape_until_scroll(fs, true);
        scratch.layout_runs().next().and_then(|r| r.glyphs.first().filter(|g| g.glyph_id != 0).map(|g| g.font_id))
    }

    /// Where a character is drawn: 0 for the first monospace face, else the numbered face that the old search gives it, or UNKNOWN.
    fn face_of(&mut self, fs: &mut FontSystem, c: char) -> u16 {
        if let Some(&k) = self.by_char.get(&c) {
            return k;
        }
        self.probes += 1;
        let mut utf8 = [0u8; 4];
        let text: &str = c.encode_utf8(&mut utf8);
        let wanted = self.font_of(fs, text, &Attrs::new().family(Family::Monospace));
        let face = wanted.and_then(|id| fs.db().face(id).map(|f| (f.families[0].0.clone(), f.weight, f.style, f.stretch)));
        // the name must pick this very face: shape the character with it and compare; a character that does not check out keeps the old search
        let k = match face {
            None => UNKNOWN,
            Some(face) if Some(&face) == self.primary.as_ref() => 0,
            Some(face) => {
                let by_name = Attrs::new().family(Family::Name(&face.0)).weight(face.1).style(face.2).stretch(face.3);
                if self.font_of(fs, text, &by_name) == wanted {
                    match self.named.iter().position(|n| *n == face) {
                        Some(i) => i as u16 + 1,
                        None => {
                            self.named.push(face);
                            self.named.len() as u16
                        }
                    }
                } else {
                    UNKNOWN
                }
            }
        };
        self.by_char.insert(c, k);
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::{BufferLine, Color, LayoutGlyph, LineEnding, SwashCache};
    use std::collections::HashSet;

    /// How a line was drawn: the glyphs as laid out and every rectangle handed to the blit, which is what the pixels are made of.
    #[derive(PartialEq, Debug)]
    struct Drawn {
        glyphs: Vec<(u16, u32, u32, u32, usize, usize)>,
        rects: Vec<(i32, i32, u32, u32, u32)>,
        fonts: usize,
    }

    /// The fast fonts and the old search side by side, the way the measurement client has them with and without `--plain-fonts`.
    struct Pair {
        fast: FontSystem,
        plain: FontSystem,
        faces: Faces,
        plain_faces: Faces,
        swash: SwashCache,
        plain_swash: SwashCache,
    }

    fn draw(fs: &mut FontSystem, swash: &mut SwashCache, faces: &mut Faces, text: &str) -> Drawn {
        let mut buffer = Buffer::new(fs, Metrics::new(15.0, 18.0));
        buffer.set_size(Some(1400.0), Some(60.0));
        buffer.lines = vec![BufferLine::new(text, LineEnding::None, faces.line(fs, text), Shaping::Advanced)];
        buffer.shape_until_scroll(fs, true);
        let mut glyphs = Vec::new();
        let mut fonts = HashSet::new();
        for run in buffer.layout_runs() {
            for g in run.glyphs {
                let g: &LayoutGlyph = g;
                fonts.insert(g.font_id);
                glyphs.push((g.glyph_id, g.x.to_bits(), g.y.to_bits(), g.w.to_bits(), g.start, g.end));
            }
        }
        let mut rects = Vec::new();
        buffer.draw(fs, swash, Color::rgb(0xE0, 0xE0, 0xE0), |x, y, w, h, color| rects.push((x, y, w, h, color.0)));
        Drawn { glyphs, rects, fonts: fonts.len() }
    }

    impl Pair {
        fn new() -> Self {
            Pair { fast: system(false), plain: system(true), faces: Faces::new(false), plain_faces: Faces::new(true), swash: SwashCache::new(), plain_swash: SwashCache::new() }
        }

        /// None when the two draw the same, else what differs.
        fn differs(&mut self, text: &str) -> Option<String> {
            let a = draw(&mut self.fast, &mut self.swash, &mut self.faces, text);
            let b = draw(&mut self.plain, &mut self.plain_swash, &mut self.plain_faces, text);
            if a.glyphs != b.glyphs || a.rects != b.rects {
                let first = a.glyphs.iter().zip(&b.glyphs).position(|(x, y)| x != y);
                return Some(format!("{:?}: {} glyphs against {}, first difference at {:?}, {} rects against {}", text.escape_unicode().to_string(), a.glyphs.len(), b.glyphs.len(), first, a.rects.len(), b.rects.len()));
            }
            None
        }

        fn fonts_of(&mut self, text: &str) -> usize {
            draw(&mut self.fast, &mut self.swash, &mut self.faces, text).fonts
        }
    }

    /// Every range of the whitelist with its neighbours: the lines that take the fast path.
    const WHITELISTED: &[&str] = &[
        "Hello, world! fn main() { let x = 42; } src/lib.rs:12 --flag=value",
        "파일을 읽습니다 read파일 file파일.rs 한글Hangul 안녕하세요, world",
        "漢字 日本語 ひらがな カタカナ ｶﾀｶﾅ 한자漢字 漢字kana かな漢字",
        "Привет мир Hello Привет Ёё Ђђ Ѣѣ Αβγ ΑΒΓ άέήίόύώ Γειά σου κόσμε",
        "→ ✓ ✗ ★ ☆ © ® ™ € £ ¥ ° ± × ÷ ≤ ≥ ≠ ∞ ∑ √ ∂ ← ↑ ↓ ↔ ⇐ ⇒ ∀ ∃ ∅ ∈ ∉ ∋ ∏",
        "┌─┬─┐ │ │ │ ├─┼─┤ └─┴─┘ ▀▄█▌▐░▒▓ ■□▲△▼▽◆◇○● ⠁⠃⠉⠙⠑ ⣿⣀⡇",
        "🙂😀🚀🎉👍 ✨ ❤ ☀ ⭐ 🀄🃏 🅰🅱🆎🆑🆗 🈁🈂🈚🈯 🌀🌍🍀🍎🎂 🐀🐶👀👑💡💻📁🔥🔑 😁😂🤣😭🙏 🚂🚗🛑 🤔🤖🥳🧠🦄 🪄🪐",
        "ＡＢＣ１２３！？（）　全角 ｱｲｳ ￡￥ ㅋㅋㅋ ㅎㅎ ㅠㅠ ㅇㅇ ㈠㈡ ㊀㊁ ㍿ ㎏㎎㎝",
        "— … “ ” ‘ ’ – • ‹ › « » † ‡ ‰ ¿ ¡ § ¶",
        "Tiếng Việt àáâãäå çèéêë ĀāĂă ŁłŃń Ǆǅ Ạạ Ảả Ấấ Ềề ỹ",
        "豈 更 車 「」『』【】〈〉《》〒〓 〃々〆〇",
        "℃ ℉ № ℮ Ⅰ Ⅱ Ⅲ ⅰ ⅱ ⅓ ⅔ ⌘ ⌥ ⇧ ⌫ ⏎ ⏩ ☀ ☁ ☂ ☃ ★ ☎ ☑ ♠ ♣ ♥ ♦ ⚠ ⚡ ✂ ✅ ✈ ✉ ✔ ✖ ❄ ❌ ❓ ➕ ➖ ➡ ⤴ ⤵ ⬅ ⬆ ⬇ ⬛ ⬜",
        "a가b나c다 한A글B 漢a字b かA な 가漢 나か 다ㅋ 한→글 ✓완료 🙂파일 Привет파일 α한",
        "  leading and trailing spaces  ",
        "tab-free text with Ünïcödé and 한국어 and 日本語 and emoji 🙂 in one line that is long enough to wrap around the edge of the window and go on",
    ];

    /// Lines with a character whose shape depends on its neighbours, or that no font has: all of them go whole to the old search.
    const EXACT: &[&str] = &[
        "x שלום,עולם y",
        "x שלום,123 b",
        "שלום/한",
        "שלום 한국어 world",
        "한국어 שלום, 123",
        "אבג hello",
        "مرحبا بالعالم 123",
        "hello مرحبا, 한국어",
        "مرحبا/한",
        "ab אבג,cd",
        "\u{5D0} starts with a right-to-left letter",
        "각\u{11A8} 한\u{1161} \u{1100}\u{1161}\u{11A8} \u{1112}\u{1161}\u{11AB}",
        "한\u{200B}글 \u{FEFF}한글 한\u{200B}",
        "a\u{200B}b \u{200C}\u{200D} a\u{200E}b \u{202A}c\u{202C}",
        "❤\u{FE0F} ☀\u{FE0F} 1\u{FE0F}\u{20E3}",
        "👨\u{200D}👩\u{200D}👧 👩\u{200D}💻",
        "🇰🇷 🇺🇸 🇯🇵",
        "👍🏽 👍🏿 🧑🏻\u{200D}💻",
        "e\u{301} a\u{300} n\u{303} x\u{20D7}",
        "\u{1D15E}\u{1D165} \u{1D157}\u{1D165}\u{1D16E}",
        "नमस्ते दुनिया",
        "สวัสดีชาวโลก",
        "ជំរាបសួរ ម៉ាស៊ីន",
        "tab\tseparated",
        "\u{E000} private use",
        "tags \u{E0067}\u{E0062}",
    ];

    /// A small generator with a fixed seed, so a failure is the same failure on the next run.
    struct Seed(u64);
    impl Seed {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    #[test]
    fn whitelisted_lines_are_drawn_as_the_old_search_draws_them_and_never_reach_it() {
        let mut pair = Pair::new();
        let mut fonts = 0;
        let mut routed = Vec::new();
        for text in WHITELISTED {
            let before = pair.faces.exact;
            if let Some(why) = pair.differs(text) {
                panic!("{why}");
            }
            if pair.faces.exact != before {
                let odd: Vec<String> = text.chars().filter(|&c| !independent(c) || pair.faces.by_char.get(&c) == Some(&UNKNOWN)).map(|c| format!("U+{:04X}", c as u32)).collect();
                routed.push(format!("{text:?} ({})", odd.join(" ")));
            }
            fonts = fonts.max(pair.fonts_of(text));
        }
        assert!(routed.is_empty(), "whitelisted lines were given to the exact path: {routed:?}");
        assert!(pair.faces.probes > 50, "the lines must reach the lookup of characters ({} probes)", pair.faces.probes);
        assert!(fonts >= 3 || pair.faces.named.len() >= 3, "the corpus must use several faces, or comparing proves nothing");
    }

    #[test]
    fn random_pairs_and_triples_of_whitelisted_characters_are_drawn_as_the_old_search_draws_them() {
        let mut pair = Pair::new();
        let mut seed = Seed(0x9E37_79B9_7F4A_7C15);
        pair.faces.prepare(&mut pair.fast);
        // characters of every range that some face draws, a handful per range, so each range is met with its neighbours
        let mut pool: Vec<char> = Vec::new();
        for &(low, high) in INDEPENDENT.iter() {
            let mut kept = 0;
            for _ in 0..400 {
                if kept == 40 {
                    break;
                }
                let c = char::from_u32(low + (seed.next() % (high - low + 1) as u64) as u32).expect("a scalar value");
                if pair.faces.face_of(&mut pair.fast, c) != UNKNOWN {
                    pool.push(c);
                    kept += 1;
                }
            }
        }
        assert!(pool.len() > 300, "{} characters in the pool", pool.len());
        let mut wrong = Vec::new();
        for i in 0..1200 {
            let mut text = String::new();
            if i % 5 == 0 {
                text.push('a');
            }
            for _ in 0..2 + i % 2 {
                text.push(pool[seed.below(pool.len())]);
            }
            if i % 7 == 0 {
                text.push_str(" x");
            }
            if let Some(why) = pair.differs(&text) {
                wrong.push(why);
            }
        }
        assert!(wrong.is_empty(), "{} lines differ, the first: {}", wrong.len(), wrong[0]);
        assert_eq!(pair.faces.exact, 0, "a line of known whitelisted characters was given to the exact path");
    }

    #[test]
    fn lines_with_a_neighbour_dependent_character_take_the_exact_path_whole_and_look_the_same() {
        let mut pair = Pair::new();
        for text in EXACT {
            if let Some(why) = pair.differs(text) {
                panic!("{why}");
            }
        }
        assert_eq!(pair.faces.exact, EXACT.len() as u64, "every listed line must be shaped whole by the old search");
    }

    #[test]
    fn a_character_that_does_not_check_out_sends_its_word_to_the_old_search_and_nothing_else() {
        // a character whose face the name does not pick (or that no face draws) is UNKNOWN in the memo; here three are made so, whatever the fonts of this machine
        let mut pair = Pair::new();
        pair.faces.prepare(&mut pair.fast);
        let unknown = ['é', '한', '✓'];
        for c in unknown {
            pair.faces.by_char.insert(c, UNKNOWN);
        }
        let mut lines = 0;
        for c in unknown {
            for text in [format!("a{c}b"), format!("글{c}자 x"), format!("{c}{c}"), format!("→{c}😀")] {
                if let Some(why) = pair.differs(&text) {
                    panic!("{why}");
                }
                lines += 1;
            }
        }
        assert_eq!(pair.faces.exact, 0, "the line is whitelisted: only the word with the character goes to the old search");
        assert!(pair.faces.w_old >= lines, "each of these lines holds a word with a character that does not check out ({} words for {lines} lines)", pair.faces.w_old);
        let words = pair.faces.w_old;
        for text in ["no such character here", "글자 → 😀 plain"] {
            assert!(pair.differs(text).is_none());
        }
        assert_eq!(pair.faces.w_old, words, "a line without such a character stays on the fast path");
    }

    #[test]
    fn words_are_cut_where_cosmic_text_cuts_them() {
        fn spoken(text: &str) -> Vec<&str> {
            words(text).iter().map(|&(a, b)| &text[a..b]).collect()
        }
        assert_eq!(spoken("ab cd"), ["ab", " ", "cd"]);
        assert_eq!(spoken("ab  cd"), ["ab", " ", " ", "cd"], "white space is split off one character at a time");
        assert_eq!(spoken("a\u{A0}b"), ["a\u{A0}b"], "a no-break space does not cut");
        assert_eq!(spoken("(a)=>{b}"), ["(a)=>{b}"], "no cut between two ASCII punctuation characters");
        assert_eq!(spoken("漢字かな"), ["漢", "字", "か", "な"], "an ideograph and a kana may break before the next");
        assert!(words("").is_empty());
        assert_eq!(spoken("a"), ["a"]);
    }

    #[test]
    fn what_is_not_known_to_be_independent_is_never_on_the_whitelist() {
        // the classes a line must never be split inside: every code point of them is outside the whitelist
        let excluded: &[(u32, u32)] = &[
            (0x00, 0x1F), (0x7F, 0xA0 - 1), (0xAD, 0xAD), // controls, soft hyphen
            (0x300, 0x36F), (0x483, 0x489), (0x20D0, 0x20FF), (0x3099, 0x309A), (0x302A, 0x302F), (0x1AB0, 0x1AFF), (0x1DC0, 0x1DFF), (0xFE20, 0xFE2F), // combining marks
            (0x590, 0x8FF), (0xFB1D, 0xFDFF), (0xFE70, 0xFEFF), (0x10800, 0x10FFF), (0x1E800, 0x1EFFF), // right-to-left and joining scripts
            (0x900, 0x1CFF), (0xA800, 0xABFF), // Indic, Thai, Lao, Tibetan, Myanmar, Khmer, and the other complex scripts
            (0x1100, 0x11FF), (0xA960, 0xA97F), (0xD7B0, 0xD7FF), // conjoining and extended jamo
            (0x200B, 0x200F), (0x202A, 0x202E), (0x2060, 0x206F), (0xFEFF, 0xFEFF), // zero-width and directional controls
            (0xFE00, 0xFE0F), (0xE0100, 0xE01EF), // variation selectors
            (0x1F1E6, 0x1F1FF), (0x1F3FB, 0x1F3FF), (0xE0000, 0xE007F), // regional indicators, skin tones, tags
            (0xE000, 0xF8FF), (0xF0000, 0x10FFFF), // private use
            (0xD800, 0xDFFF), (0x30000, 0xEFFFF), // surrogates (not chars), unassigned planes
        ];
        let mut wrong = Vec::new();
        for &(low, high) in excluded {
            for code in low..=high {
                if let Some(c) = char::from_u32(code) {
                    if independent(c) {
                        wrong.push(format!("U+{code:04X}"));
                        if wrong.len() > 5 {
                            break;
                        }
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "on the whitelist although they are not independent of their neighbours: {wrong:?}");
    }

    /// Lines on which cutting a word into runs by character drew other glyphs than the old search (found by a differential of 3.5 M generated lines): a halfwidth
    /// sound mark after a base that is not halfwidth, a letter with a stroke after a Greek or Cyrillic letter and a symbol of another face, an extension-B
    /// ideograph without a monospace face in a word with other cut points. The decision per word keeps each of these words in one run.
    const CUT_INSIDE_A_WORD: &[&str] = &[
        "ĥ𤸻 \u{2BCF} Ɏ노ㆌŞﾟ",
        "\u{1F54E}Я\u{2F4} 퇻ﾞ 덷竴\u{1F240}",
        "\u{1F0E7}땡ͻ 탪\u{1F800}ﾟ笥\u{235F}",
        "ш \u{1FA78}ﾟ\u{1F61A}ь \u{25A8}ҳ 훧",
        "ἷℂẪ\u{1F580} 늗ﾟ햺ⓛ",
        "혀螹㊃\u{1FCF} 第 㜳Φﾟ",
        "\u{1F4F9}\u{25FD}\u{319C}醦장 ヌﾟ \u{20BC}",
        "ʏ\u{2B66}\u{1F5A7}ﾟ\u{1F2D6} ㉝蒷 ⅱ",
        "Ω\u{21F3}ɨ",
        "Я\u{21F3}ɨ",
        "Ω\u{2713}ɨ",
        "Я\u{2713}ɨ",
        "Ω\u{2211}ɨ",
        "Я\u{2211}ɨ",
        "Ω\u{21F3}ḭ",
        "Я\u{21F3}ḭ",
        "Ω\u{2713}ḭ",
        "Я\u{2713}ḭ",
        "Ω\u{2211}ḭ",
        "Я\u{2211}ḭ",
        "\u{1F797} Ⓡ Ў\u{21EE}ɨ\u{1F027}ϥ\u{1F6A6}",
        "\u{1F895}䤳 \u{21B0}\u{1F274}\u{2278}\u{2805}\u{2768}ḭ",
        "肋\u{1F752} た \u{223C}자鉘 \u{2893}ḭ",
    ];

    #[test]
    fn lines_on_which_a_cut_inside_a_word_drew_other_glyphs_than_the_old_search_are_drawn_as_it_draws_them() {
        let mut pair = Pair::new();
        let wrong: Vec<String> = CUT_INSIDE_A_WORD.iter().filter_map(|text| pair.differs(text)).collect();
        assert!(wrong.is_empty(), "{} of {} lines differ, the first: {}", wrong.len(), CUT_INSIDE_A_WORD.len(), wrong[0]);
    }

    #[test]
    fn a_sample_of_the_whitelist_next_to_neighbours_is_drawn_as_the_old_search_draws_it_in_every_form() {
        // each sampled character with a base of another script, a symbol of another face and a Katakana letter, in the five forms that move a cut around it:
        // before, after, with a space between, with a no-break space either way
        let mut pair = Pair::new();
        pair.faces.prepare(&mut pair.fast);
        let mut seed = Seed(0x1357_9BDF_2468_ACE0);
        let mut pool: Vec<char> = vec!['\u{FF9E}', '\u{FF9F}', '\u{268}', '\u{1E2D}', '\u{28D8A}', '\u{FF76}', '\u{30AB}', '\u{1F6E5}'];
        for &(low, high) in INDEPENDENT.iter() {
            for _ in 0..6 {
                pool.push(char::from_u32(low + (seed.next() % (high - low + 1) as u64) as u32).expect("a scalar value"));
            }
        }
        let mut wrong = Vec::new();
        let mut lines = 0;
        for context in ["a", "Ω", "Я", "노", "カ", "✓"] {
            for &x in &pool {
                for text in [format!("{context}{x}"), format!("{x}{context}"), format!("{context} {x}"), format!("{context}\u{A0}{x}"), format!("{x}\u{A0}{context}")] {
                    lines += 1;
                    if let Some(why) = pair.differs(&text) {
                        wrong.push(why);
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{} of {lines} lines differ, the first: {}", wrong.len(), wrong[0]);
        assert!(pair.faces.w_single > 0 && pair.faces.w_old > 0 && pair.faces.w_base > 0, "all three decisions must have been taken: {} {} {}", pair.faces.w_base, pair.faces.w_single, pair.faces.w_old);
    }

}
