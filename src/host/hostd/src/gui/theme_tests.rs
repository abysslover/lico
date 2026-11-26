use super::*;

/// Puts the thread's mode back to dark when a test that set it ends (the harness may run another test on the same thread).
struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        set_mode(Mode::Dark);
    }
}

#[test]
fn the_choices_go_round_in_the_order_the_owner_named() {
    assert_eq!(Choice::Dark.next(), Choice::Light);
    assert_eq!(Choice::Light.next(), Choice::System);
    assert_eq!(Choice::System.next(), Choice::Dark);
    assert_eq!(Choice::Dark.next().next().next(), Choice::Dark, "three presses are back where they began");
}

#[test]
fn a_window_starts_in_dark_as_it_always_looked() {
    assert_eq!(Choice::default(), Choice::Dark);
    assert_eq!(Mode::default(), Mode::Dark);
    assert_eq!(mode(), Mode::Dark, "a thread paints in dark until it is told otherwise");
}

#[test]
fn a_word_names_its_choice_and_no_other_word_does() {
    for choice in [Choice::Dark, Choice::Light, Choice::System] {
        assert_eq!(Choice::of_word(choice.word()), Some(choice));
    }
    assert_eq!(Choice::Dark.word(), "dark");
    assert_eq!(Choice::Light.word(), "light");
    assert_eq!(Choice::System.word(), "system");
    for other in ["", "Dark", "LIGHT", "auto", "dark ", "night", "0"] {
        assert_eq!(Choice::of_word(other), None, "{other:?}");
    }
    assert_eq!(Mode::Dark.word(), "dark");
    assert_eq!(Mode::Light.word(), "light");
}

#[test]
fn the_system_choice_follows_the_desktop_and_is_dark_until_it_is_known() {
    assert_eq!(resolve(Choice::System, None), Mode::Dark, "no answer yet: the look of today");
    assert_eq!(resolve(Choice::System, Some(true)), Mode::Dark);
    assert_eq!(resolve(Choice::System, Some(false)), Mode::Light);
    for system in [None, Some(true), Some(false)] {
        assert_eq!(resolve(Choice::Dark, system), Mode::Dark, "a chosen dark ignores the desktop");
        assert_eq!(resolve(Choice::Light, system), Mode::Light, "a chosen light ignores the desktop");
    }
}

#[test]
fn dark_does_not_move_a_colour() {
    for c in [0xFF10_1418u32, 0xFFE0_E0E0, 0xFF00_0000, 0xFFFF_FFFF, 0xFF2C_4A78, 0x8012_3456, 0] {
        assert_eq!(tone_with(Mode::Dark, c), c);
    }
}

#[test]
fn light_mirrors_the_lightness_and_keeps_the_hue() {
    let cases = [
        (0xFF10_1418u32, 0xFFE7_EBEFu32, "the window's background becomes pale"),
        (0xFFE0_E0E0, 0xFF1F_1F1F, "the text becomes dark"),
        (0xFF00_0000, 0xFFFF_FFFF, "black is white"),
        (0xFFFF_FFFF, 0xFF00_0000, "white is black"),
        (0xFF80_8080, 0xFF7F_7F7F, "a mid grey stays where it is"),
        (0xFFFF_0000, 0xFFFF_0000, "a pure red does not move: it stays a red that reads on the pale ground"),
        (0xFFCD_0000, 0xFFFF_3232, "a dark red becomes a plain red, not a pink"),
        (0xFF15_2232, 0xFFCD_DAEA, "the person's blue band becomes a pale blue"),
    ];
    for (dark, light, why) in cases {
        assert_eq!(tone_with(Mode::Light, dark), light, "{why}: {dark:#010x}");
    }
}

#[test]
fn light_twice_is_the_colour_again_and_never_leaves_the_range() {
    let steps: Vec<u32> = (0..=255).step_by(15).chain([1, 254, 255]).collect();
    for &r in &steps {
        for &g in &steps {
            for &b in &steps {
                let c = 0xFF00_0000 | r << 16 | g << 8 | b;
                let once = tone_with(Mode::Light, c);
                assert_eq!(once & 0xFF00_0000, 0xFF00_0000, "no channel runs into the alpha: {c:#010x}");
                assert_eq!(tone_with(Mode::Light, once), c, "an involution: {c:#010x}");
                let (or, og, ob) = ((once >> 16) & 0xFF, (once >> 8) & 0xFF, once & 0xFF);
                let spread = |a: u32, b: u32, c: u32| a.max(b).max(c) - a.min(b).min(c);
                assert_eq!(spread(or, og, ob), spread(r, g, b), "the chroma is kept: {c:#010x}");
                assert_eq!(or.max(og).max(ob) + or.min(og).min(ob), 510 - (r.max(g).max(b) + r.min(g).min(b)), "the lightness is mirrored: {c:#010x}");
            }
        }
    }
}

#[test]
fn the_alpha_of_a_colour_is_kept() {
    assert_eq!(tone_with(Mode::Light, 0x8010_1418), 0x80E7_EBEF);
    assert_eq!(tone_with(Mode::Light, 0x0000_0000), 0x00FF_FFFF);
}

#[test]
fn the_primitives_read_the_mode_of_their_own_thread() {
    let _restore = Restore;
    assert_eq!(tone(0xFF10_1418), 0xFF10_1418);
    set_mode(Mode::Light);
    assert_eq!(mode(), Mode::Light);
    assert_eq!(tone(0xFF10_1418), 0xFFE7_EBEF);
    let other = std::thread::spawn(|| (mode(), tone(0xFF10_1418))).join().unwrap();
    assert_eq!(other, (Mode::Dark, 0xFF10_1418), "another thread is not told");
    set_mode(Mode::Dark);
    assert_eq!(tone(0xFF10_1418), 0xFF10_1418);
}

#[test]
fn the_ink_of_a_glyph_follows_the_mode_and_keeps_its_alpha() {
    let _restore = Restore;
    let ink = Color::rgba(0xE0, 0xE0, 0xE0, 0x80);
    assert_eq!(tone_ink(ink), ink, "dark: the same ink");
    set_mode(Mode::Light);
    let mapped = tone_ink(ink);
    assert_eq!((mapped.r(), mapped.g(), mapped.b(), mapped.a()), (0x1F, 0x1F, 0x1F, 0x80));
    let blue = tone_ink(Color::rgb(0x58, 0xA6, 0xFF));
    assert_eq!((blue.r(), blue.g(), blue.b(), blue.a()), (0x00, 0x4E, 0xA7, 0xFF), "a light blue ink is a dark blue one");
}

#[test]
fn the_desktops_two_answers_say_dark_light_or_nothing() {
    let cases = [
        ("'prefer-dark'", "'Adwaita'", Some(true), "the scheme says dark"),
        ("'prefer-dark'", "'Yaru'", Some(true), "the scheme wins over the theme"),
        ("'prefer-light'", "'Yaru-dark'", Some(false), "the scheme wins over the theme"),
        ("'default'", "'Adwaita'", Some(false), "no preference is light"),
        ("'default'", "'Yaru-dark'", Some(true), "no preference, a theme named dark"),
        ("'default'", "'Adwaita-DARK'", Some(true), "the name's case does not matter"),
        ("'default'", "", Some(false), "no preference and no theme is light"),
        ("", "'Yaru-dark'", Some(true), "no scheme: the theme's name"),
        ("", "'Adwaita'", Some(false), "no scheme: a theme that is not named dark"),
        ("", "", None, "nothing said"),
        ("  \n", "\n", None, "blank answers say nothing"),
        ("'something-new'", "'Yaru-dark'", Some(true), "a scheme this window does not know: the theme's name"),
        ("'something-new'", "", None, "a scheme this window does not know and no theme"),
        ("prefer-dark\n", "", Some(true), "quotes are not needed and a newline is trimmed"),
        ("\"prefer-light\"", "", Some(false), "double quotes too"),
    ];
    for (scheme, theme, want, why) in cases {
        assert_eq!(parse_system(scheme, theme), want, "{why}: {scheme:?} {theme:?}");
    }
}

#[test]
fn a_scope_sets_the_mode_for_its_life_and_puts_back_what_was_there() {
    let _restore = Restore;
    assert_eq!(mode(), Mode::Dark);
    {
        let _light = scoped(Mode::Light);
        assert_eq!(mode(), Mode::Light);
        {
            let _dark = scoped(Mode::Dark);
            assert_eq!(mode(), Mode::Dark);
        }
        assert_eq!(mode(), Mode::Light, "the inner scope puts back the light it found");
    }
    assert_eq!(mode(), Mode::Dark, "and the outer one the dark");
}
