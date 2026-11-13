//! The colors and labels of the three sidebar tabs (Directories, Chats, Calls): one hue each, so the tabs tell apart at a glance.

/// What the tabs say, in order: Directories (0), Chats (1), Calls (2).
pub(super) const TAB_LABELS: [&str; 3] = ["Directories", "Chats", "Calls"];

/// Fill of the tab, ink of its label and the bar over an active tab, each as 0xAARRGGBB.
#[derive(Clone, Copy, Debug)]
pub(super) struct TabPalette {
    pub fill: u32,
    pub ink: u32,
    pub bar: u32,
}

/// One hue per tab: blue for Directories, amber for Chats, teal for Calls. An inactive tab keeps a faint tint of its own hue.
pub(super) fn palette(tab: usize, active: bool) -> TabPalette {
    let (bar, on_fill, on_ink, off_fill, off_ink) = match tab {
        0 => (0xFF56_B4FF, 0xFF21_3147, 0xFFEA_F0F7, 0xFF15_2230, 0xFF8F_B8DE),
        1 => (0xFFF2_B13C, 0xFF3A_2F17, 0xFFFF_E9B8, 0xFF27_2012, 0xFFD9_B25F),
        _ => (0xFF38_B2AC, 0xFF19_3338, 0xFFE6_F7F5, 0xFF14_2529, 0xFF6C_CFC6),
    };
    if active {
        TabPalette { fill: on_fill, ink: on_ink, bar }
    } else {
        TabPalette { fill: off_fill, ink: off_ink, bar }
    }
}

/// Relative luminance of an 0xAARRGGBB color (WCAG).
pub(super) fn luminance(color: u32) -> f64 {
    let lin = |v: u32| {
        let c = f64::from(v & 0xFF) / 255.0;
        if c <= 0.039_28 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(color >> 16) + 0.7152 * lin(color >> 8) + 0.0722 * lin(color)
}

/// Contrast ratio of two colors (WCAG), 1.0 to 21.0.
pub(super) fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(c: u32) -> (u8, u8, u8) {
        ((c >> 16) as u8, (c >> 8) as u8, c as u8)
    }

    /// Hue in degrees of an 0xAARRGGBB color.
    fn hue(c: u32) -> f64 {
        let (r, g, b) = rgb(c);
        let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let d = max - min;
        assert!(d > 0.0, "a grey has no hue");
        let h = if max == r { ((g - b) / d).rem_euclid(6.0) } else if max == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
        h * 60.0
    }

    #[test]
    fn the_labels_say_chats_and_never_the_old_word() {
        assert_eq!(TAB_LABELS, ["Directories", "Chats", "Calls"]);
    }

    #[test]
    fn each_tab_has_its_own_hue_and_the_active_bars_differ() {
        let bars: Vec<u32> = (0..3).map(|t| palette(t, true).bar).collect();
        assert!((200.0..225.0).contains(&hue(bars[0])), "Directories is blue: {}", hue(bars[0]));
        assert!((35.0..50.0).contains(&hue(bars[1])), "Chats is amber: {}", hue(bars[1]));
        assert!((170.0..185.0).contains(&hue(bars[2])), "Calls is teal: {}", hue(bars[2]));
    }

    #[test]
    fn an_inactive_tab_keeps_a_faint_tint_of_its_own_color() {
        for tab in 0..3 {
            let (on, off) = (palette(tab, true), palette(tab, false));
            assert_ne!(on.fill, off.fill, "tab {tab}: active and inactive fills differ");
            let (hb, hf) = (hue(on.bar), hue(off.fill));
            let gap = (hb - hf).abs().min(360.0 - (hb - hf).abs());
            assert!(gap < 20.0, "tab {tab}: the inactive fill is tinted with the bar's hue (gap {gap})");
            assert!(luminance(off.fill) < luminance(on.fill), "tab {tab}: the inactive fill is fainter than the active one");
        }
    }

    #[test]
    fn the_three_inactive_fills_tell_apart() {
        let fills: Vec<u32> = (0..3).map(|t| palette(t, false).fill).collect();
        assert!(fills[0] != fills[1] && fills[1] != fills[2] && fills[0] != fills[2]);
    }

    #[test]
    fn every_label_reads_on_its_fill_with_a_contrast_of_at_least_4_5() {
        for tab in 0..3 {
            for active in [true, false] {
                let p = palette(tab, active);
                let ratio = contrast(p.ink, p.fill);
                assert!(ratio >= 4.5, "tab {tab} active {active}: contrast {ratio:.2}");
            }
        }
    }
}
