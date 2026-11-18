//! The invariants of the spec as tests: it never panics, cutting does not matter, the state stays valid, memory is bounded, it is deterministic.

mod common;
use common::*;
use lico_vt::{Attrs, Modes, Term, View, MAX_COLS, MAX_ROWS, MAX_SCROLLBACK_CELLS};

/// A stream that reaches most of what the component understands: strings, SGR, wide characters, regions, scrolling, the alternate screen, replies.
fn sample() -> Vec<u8> {
    let mut s = String::from("\x1bc");
    s += "\x1b]0;a title\x07\x1b[?2004h\x1b[?1h\x1b[?1000h\x1b[?1006h";
    s += "plain text line one\r\nline two with \x1b[1;31mred bold\x1b[0m and \x1b[38;2;10;20;30mtrue\x1b[48;5;200mcolour\x1b[0m\r\n";
    s += "한글 테스트 😀 wide e\u{301}\u{200d} end\r\n";
    for i in 0..30 {
        s += &format!("scroll {i} {}\r\n", "x".repeat(i % 25));
    }
    s += "\x1b[?1049h\x1b[2J\x1b[H\x1b[2;4r\x1b[4;1H\n\n\n\x1bM\x1b[r\x1b[S\x1b[T\x1b[3L\x1b[2M";
    s += "\x1b[5;10H\x1b[K\x1b[1J\x1b[2K\x1b[3@\x1b[2P\x1b[4X\x1b[3b\x1b[2I\x1b[Z\x1bH\x1b[g";
    s += "\x1b(0lqqk\x1b(B\x1b7\x1b[10;10H한x\x1b8\x1b[?7l123456789012345678901234567890\x1b[?7h";
    s += "\x1b[1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18;19;20;21;22;23;24;25;26;27;28;29;30;31;32;33;34;35m";
    s += "\x1bPq#0;2;0;0;0\x1b\\\x1b_apc\x1b\\\x1b^pm\x1b\\\x1bXsos\x1b\\\x1b]8;;http://x\x07\x1b]52;c;AAAA\x07";
    s += "\x1b[6n\x1b[c\x1b[5n\x1b[?25l\x1b[?25h\x1b[4h\x1b[4l\x1b[?6h\x1b[?6l\x1b[3J\x1b[999;999H\x1b[0;0H";
    s += "\x1b[?1049l\x1b[2;3r\x07\x08\t\x0e\x0f\x1b[10;1Htail 한글 text\r\n";
    s.into_bytes()
}

/// The structure the spec promises, after any call.
fn check(t: &Term) {
    let (cols, rows) = t.size();
    let (r, c) = t.cursor();
    assert!(r < rows && c < cols, "the cursor {r},{c} is inside {cols}x{rows}");
    for scroll in [0usize, 1, 7, 1000, usize::MAX] {
        let v = t.view(scroll);
        assert_eq!(v.lines.len(), rows as usize);
        assert_eq!(v.wrapped.len(), rows as usize);
        assert!(v.top + rows as usize <= v.total, "the view lies inside the total");
        for l in &v.lines {
            assert_eq!(l.len(), cols as usize, "every line has the width");
            for (i, cell) in l.iter().enumerate() {
                if cell.attrs.contains(Attrs::WIDE) {
                    assert!(i + 1 < l.len() && l[i + 1].attrs.contains(Attrs::TAIL), "a wide cell has its tail");
                }
                if cell.attrs.contains(Attrs::TAIL) {
                    assert!(i > 0 && l[i - 1].attrs.contains(Attrs::WIDE), "a tail has its head");
                    assert_eq!(cell.ch, ' ');
                }
            }
        }
    }
    assert!(t.view(0).total >= rows as usize);
}

#[derive(PartialEq, Debug)]
struct Snap {
    view: View,
    oldest: View,
    cursor: (u16, u16),
    modes: Modes,
    title: String,
    reply: Vec<u8>,
    bells: u64,
    text: String,
}

fn snap(t: &mut Term) -> Snap {
    Snap {
        view: t.view(0),
        oldest: t.view(usize::MAX),
        cursor: t.cursor(),
        modes: t.modes(),
        title: t.title().to_string(),
        reply: t.take_reply(),
        bells: t.bells(),
        text: t.text_between((0, 0), (usize::MAX, u16::MAX)),
    }
}

#[test]
fn the_sample_leaves_a_valid_state_at_every_size() {
    for (c, r) in [(1, 1), (1, 5), (5, 1), (2, 2), (3, 3), (20, 6), (80, 24), (MAX_COLS, MAX_ROWS)] {
        let mut t = term(c, r);
        t.feed(&sample());
        check(&t);
    }
}

#[test]
fn the_same_bytes_cut_anywhere_give_the_same_state() {
    let s = sample();
    let mut whole = term(40, 10);
    whole.feed(&s);
    let want = snap(&mut whole);
    // every two-way split of the sample
    for cut in 0..=s.len() {
        let mut t = term(40, 10);
        t.feed(&s[..cut]);
        t.feed(&s[cut..]);
        assert_eq!(snap(&mut t), want, "cut at {cut}");
    }
    // one byte at a time
    let mut t = term(40, 10);
    for b in &s {
        t.feed(std::slice::from_ref(b));
    }
    assert_eq!(snap(&mut t), want, "byte by byte");
    // random cuts
    let mut rng = Rng(0x9e3779b97f4a7c15);
    for _ in 0..50 {
        let mut t = term(40, 10);
        let mut i = 0;
        while i < s.len() {
            let n = 1 + rng.below(40) as usize;
            let end = (i + n).min(s.len());
            t.feed(&s[i..end]);
            i = end;
        }
        assert_eq!(snap(&mut t), want);
    }
}

#[test]
fn the_same_bytes_give_the_same_state() {
    let (mut a, mut b) = (term(33, 7), term(33, 7));
    a.feed(&sample());
    b.feed(&sample());
    assert_eq!(snap(&mut a), snap(&mut b));
}

#[test]
fn every_prefix_and_every_single_byte_change_of_the_sample_is_survived() {
    let s = sample();
    for cut in 0..=s.len() {
        let mut t = term(20, 6);
        t.feed(&s[..cut]);
        if cut % 7 == 0 {
            check(&t);
        }
    }
    for i in 0..s.len() {
        for b in [0x00u8, 0x1b, 0xff, 0x9b, b'[', b'?', b';', b'0', 0x18] {
            let mut m = s.clone();
            m[i] = b;
            let mut t = term(20, 6);
            t.feed(&m);
            if (i + b as usize).is_multiple_of(11) {
                check(&t);
            }
        }
    }
}

/// Bytes that are random, and bytes that are the pieces of the sequences, so that the deep states are reached.
fn noise(rng: &mut Rng, n: usize) -> Vec<u8> {
    const PIECES: [&[u8]; 24] = [b"\x1b[", b"\x1b]", b"\x1bP", b"\x1b_", b"\x1b(", b"\x1b\\", b"\x07", b";", b":", b"?", b"1", b"38;5;", b"38;2;", b"m", b"H", b"J", b"K", b"r", b"@", b"L", b"\r\n", "한".as_bytes(), "😀".as_bytes(), b"\x1b[?1049h"];
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        if rng.below(3) == 0 {
            out.push(rng.next() as u8);
        } else {
            out.extend_from_slice(PIECES[rng.below(PIECES.len() as u64) as usize]);
        }
    }
    out
}

#[test]
fn random_bytes_in_random_chunks_with_resizes_never_break_the_state() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut t = term(30, 10);
    let mut total = 0;
    while total < 300_000 {
        let n = 1 + rng.below(400) as usize;
        let bytes = noise(&mut rng, n);
        t.feed(&bytes);
        total += bytes.len();
        if rng.below(4) == 0 {
            t.resize(1 + rng.below(60) as u16, 1 + rng.below(25) as u16);
        }
        if rng.below(10) == 0 {
            let _ = t.take_reply();
        }
        check(&t);
    }
}

#[test]
fn a_resize_between_any_two_bytes_of_the_sample_is_survived() {
    let s = sample();
    let mut rng = Rng(77);
    let mut t = term(40, 12);
    for chunk in s.chunks(5) {
        t.feed(chunk);
        t.resize(1 + rng.below(70) as u16, 1 + rng.below(30) as u16);
    }
    check(&t);
}

#[test]
fn a_wide_character_in_a_screen_one_column_wide_does_not_loop_or_break() {
    let mut t = term(1, 3);
    feed(&mut t, "한글😀abc한\r\n한");
    check(&t);
    let mut t = term(2, 1);
    feed(&mut t, "한한한a한");
    check(&t);
}

#[test]
fn memory_stays_bounded_whatever_the_stream() {
    let mut t = term(500, 200);
    // a million parameters, a million sequence starts, strings that never end
    let mut s = String::from("\x1b[");
    s += &"1;".repeat(1_000_000);
    s += "m";
    t.feed(s.as_bytes());
    t.feed("\x1b[".repeat(1_000_000).as_bytes());
    t.feed(b"\x1bP");
    t.feed(&vec![b'x'; 2_000_000]);
    t.feed(b"\x1b\\\x1b]2;");
    t.feed(&vec![b'y'; 2_000_000]);
    t.feed(b"\x07");
    t.feed("\x1b[6n".repeat(1_000_000).as_bytes());
    check(&t);
    assert!(t.take_reply().len() <= lico_vt::MAX_REPLY);
    assert!(t.title().chars().count() <= lico_vt::MAX_TITLE);
    // the scrollback of a very wide screen
    let mut t = Term::with_scrollback(500, 200, 20_000);
    let line = "w".repeat(500);
    for _ in 0..4000 {
        feed(&mut t, &format!("{line}\r\n"));
    }
    let kept = t.view(0).total - 200;
    assert!(kept * 500 <= MAX_SCROLLBACK_CELLS, "{kept} lines of 500 cells");
}

#[test]
fn the_generation_never_goes_down() {
    let mut rng = Rng(5);
    let mut t = term(20, 5);
    let mut last = t.generation();
    for _ in 0..500 {
        let n = rng.below(30) as usize;
        let bytes = noise(&mut rng, n);
        t.feed(&bytes);
        if rng.below(5) == 0 {
            t.resize(1 + rng.below(30) as u16, 1 + rng.below(10) as u16);
        }
        assert!(t.generation() >= last);
        last = t.generation();
    }
}

#[test]
fn nothing_but_the_listed_replies_is_ever_written_to_the_program() {
    let mut rng = Rng(99);
    let mut t = term(20, 5);
    for _ in 0..200 {
        let bytes = noise(&mut rng, 300);
        t.feed(&bytes);
        let reply = t.take_reply();
        let mut i = 0;
        while i < reply.len() {
            let rest = &reply[i..];
            let n = if rest.starts_with(b"\x1b[0n") {
                4
            } else if rest.starts_with(b"\x1b[?62;22c") {
                8
            } else if rest.starts_with(b"\x1b[") && rest.iter().position(|b| *b == b'R').is_some_and(|p| rest[2..p].iter().all(|b| b.is_ascii_digit() || *b == b';')) {
                rest.iter().position(|b| *b == b'R').unwrap() + 1
            } else {
                panic!("an unlisted reply byte sequence: {rest:?}");
            };
            i += n;
        }
    }
}
