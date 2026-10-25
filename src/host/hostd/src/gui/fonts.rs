//! Fonts for the transcript. A line cost about a millisecond to shape because, with the family `monospace`, cosmic-text goes through
//! every monospace face for each word and scans all installed faces again for each word the first font lacks. The face a character
//! lands on never changes, so the first monospace face is named once and each character that it lacks is looked up once; text that
//! needs more than a lookup per character (combining marks, joined and right-to-left scripts, Indic, Thai, Jamo, flags) keeps the old search.

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

/// Characters that are drawn together with the one before them: combining marks, joiners, variation selectors, skin tones.
fn extends(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2 | 0x05C4..=0x05C5 | 0x05C7
        | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x200B..=0x200F | 0x20D0..=0x20FF | 0x3099..=0x309A | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F
        | 0x1F3FB..=0x1F3FF | 0xE0100..=0xE01EF)
}

/// Scripts whose shape depends on the neighbours (joining, reordering, stacking, conjoining) or pairs that become one glyph (flags):
/// not looked up one character at a time.
fn left_to_search(c: char) -> bool {
    matches!(c as u32, 0x0600..=0x0FFF | 0x1000..=0x11FF | 0x1780..=0x18AF | 0xA960..=0xA97F | 0xD7B0..=0xD7FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF | 0x1F1E6..=0x1F1FF)
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
        let mut chars: Vec<(usize, usize, u16)> = Vec::new(); // byte range and face of each character
        let mut joined = false;
        for (i, c) in text.char_indices() {
            let together = extends(c) || joined;
            if together {
                if let Some(last) = chars.last_mut() {
                    last.2 = UNKNOWN; // the cluster is shaped as a whole, by the old search
                }
            }
            let k = if together || left_to_search(c) {
                UNKNOWN
            } else if c.is_ascii() {
                0
            } else {
                self.face_of(fs, c)
            };
            joined = c == '\u{200D}';
            chars.push((i, i + c.len_utf8(), k));
        }
        let mut run: Option<(usize, usize, u16)> = None;
        for (start, end, k) in chars {
            match &mut run {
                Some(r) if r.2 == k => r.1 = end,
                _ => {
                    self.span(&mut list, run.take());
                    run = Some((start, end, k));
                }
            }
        }
        self.span(&mut list, run);
        list
    }

    fn base(&self) -> Attrs<'_> {
        match &self.primary {
            Some(face) if self.named_base => Attrs::new().family(Family::Name(&face.0)).weight(face.1).style(face.2).stretch(face.3),
            _ => Attrs::new().family(Family::Monospace),
        }
    }

    fn span(&self, list: &mut AttrsList, run: Option<(usize, usize, u16)>) {
        match run {
            Some((start, end, UNKNOWN)) if self.named_base => list.add_span(start..end, &Attrs::new().family(Family::Monospace)),
            Some((start, end, k)) if k != 0 && k != UNKNOWN => {
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
