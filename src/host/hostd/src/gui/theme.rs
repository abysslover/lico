//! The theme of the window (`M24-P11d`, the owner's words of 2026-11-26): dark, light or the desktop's, one icon that goes round the three. The window has no palette (its colours are
//! constants and literals in nearly every file), so the light look is a **mapping** of the colour of what is painted, applied where a colour reaches the pixels: the fills (`ui::rect`),
//! the ink of a glyph (`ui::blit_with`) and the few whole bands painted straight into the buffer. The mapping mirrors the lightness of a colour and keeps its hue and chroma:
//! every channel moves by `255 - (max + min)`, so the channels' range `[min, max]` becomes `[255 - max, 255 - min]`, nothing is clamped, and doing it twice gives the colour back.
//! Dark is the identity. Images (a page of the browser tab, a pasted picture, a colour glyph) are never mapped.
//!
//! The mode the primitives read is a thread-local, set by `compose` at the top of every frame from the window's state; a process-wide variable would leak the theme of one test into the
//! windows of another (the tests run in many threads), and the scroll oracle compares two windows of one thread, which share it by design.

use std::cell::Cell;

use cosmic_text::Color;

/// What the person chose with the icon: `Dark` is the look the window has always had and the one it starts in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Choice {
    #[default]
    Dark,
    Light,
    System,
}

impl Choice {
    /// The next state in the order the owner named: dark, light, system, and dark again.
    pub fn next(self) -> Choice {
        match self {
            Choice::Dark => Choice::Light,
            Choice::Light => Choice::System,
            Choice::System => Choice::Dark,
        }
    }

    /// The word the host keeps and the dump says.
    pub fn word(self) -> &'static str {
        match self {
            Choice::Dark => "dark",
            Choice::Light => "light",
            Choice::System => "system",
        }
    }

    /// The choice a word names; none for any other word (the caller then keeps `Dark`).
    pub fn of_word(word: &str) -> Option<Choice> {
        match word {
            "dark" => Some(Choice::Dark),
            "light" => Some(Choice::Light),
            "system" => Some(Choice::System),
            _ => None,
        }
    }
}

/// What the window is painted in now.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

impl Mode {
    pub fn word(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }
}

/// The mode of a choice: `System` is what the desktop says (`system_dark`), and dark while the answer is not known.
pub fn resolve(choice: Choice, system_dark: Option<bool>) -> Mode {
    match choice {
        Choice::Dark => Mode::Dark,
        Choice::Light => Mode::Light,
        Choice::System if system_dark == Some(false) => Mode::Light,
        Choice::System => Mode::Dark,
    }
}

/// The colour `color` (`0xAARRGGBB`) as it is painted in `mode`.
pub fn tone_with(mode: Mode, color: u32) -> u32 {
    if mode == Mode::Dark {
        return color;
    }
    let (r, g, b) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
    let shift = 255 - (r.max(g).max(b) + r.min(g).min(b)) as i32;
    let moved = |c: u32| (c as i32 + shift) as u32; // never under 0 or over 255: the channels' range is mirrored
    (color & 0xFF00_0000) | moved(r) << 16 | moved(g) << 8 | moved(b)
}

thread_local! {
    static MODE: Cell<Mode> = const { Cell::new(Mode::Dark) };
}

/// Sets the mode the primitives of this thread paint in (`compose` does it at the top of a frame).
pub fn set_mode(mode: Mode) {
    MODE.with(|m| m.set(mode));
}

/// The mode this thread paints in.
pub fn mode() -> Mode {
    MODE.with(Cell::get)
}

/// Keeps the mode of this thread for as long as it lives and puts the one it found back when it ends (`compose` takes one at the top of a frame).
pub struct Scope(Mode);

pub fn scoped(mode: Mode) -> Scope {
    let before = self::mode();
    set_mode(mode);
    Scope(before)
}

impl Drop for Scope {
    fn drop(&mut self) {
        set_mode(self.0);
    }
}

/// A colour as it is painted now.
#[inline]
pub fn tone(color: u32) -> u32 {
    tone_with(mode(), color)
}

/// The ink of a glyph as it is painted now (its alpha is kept).
#[inline]
pub fn tone_ink(ink: Color) -> Color {
    if mode() == Mode::Dark {
        return ink;
    }
    let t = tone_with(Mode::Light, 0xFF00_0000 | (ink.r() as u32) << 16 | (ink.g() as u32) << 8 | ink.b() as u32);
    Color::rgba((t >> 16) as u8, (t >> 8) as u8, t as u8, ink.a())
}

/// What the desktop prefers, from the two values `gsettings get org.gnome.desktop.interface` prints for `color-scheme` and for `gtk-theme` (each as printed, with its quotes; an empty
/// text for a value that could not be read): `Some(true)` for dark, `Some(false)` for light, none when neither says anything. `prefer-dark` is dark and `prefer-light` is light; the
/// scheme `default` (no preference) is light, unless the theme is named as a dark one (`Yaru-dark`); without a scheme only the theme's name is left to go by.
pub fn parse_system(color_scheme: &str, gtk_theme: &str) -> Option<bool> {
    let word = |text: &str| text.trim().trim_matches('\'').trim_matches('"').trim().to_lowercase();
    let (scheme, theme) = (word(color_scheme), word(gtk_theme));
    match scheme.as_str() {
        "prefer-dark" => Some(true),
        "prefer-light" => Some(false),
        "default" => Some(theme.contains("dark")),
        _ if theme.is_empty() => None,
        _ => Some(theme.contains("dark")),
    }
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
