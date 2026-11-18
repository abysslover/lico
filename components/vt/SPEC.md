# lico-vt 0.1.0 — specification

0.1.0 (M23-P01): the emulator of a terminal and the encoding of its input, written for the terminal tab of the window (M23); approved by the lead under the owner's chat of 2026-11-18 ("choose Terminal in New session is the next step"). Marks: **KNOWN FROM DOCUMENTATION** (ECMA-48, the DEC VT100 to VT220 manuals, and the xterm control-sequence list; the places where this component follows xterm are named), **MEASURED** (nothing yet: the measures of what real programs send are taken in P02 and P03 and added here in a later version).

## Purpose

Take the bytes that a program writes to its terminal and keep the screen they describe: a grid of cells, a scrollback, a cursor, a title and the modes the program switched on; take the keys, a paste and the mouse of the person and give back the bytes the program expects. It owns the meaning of the bytes and nothing else: **no process, no pty, no file, no clock, no environment variable, no thread, no `unsafe`, no allocation that the limits below do not bound**. It knows no font and no pixel: a colour is a name (`Default`, a palette index or red-green-blue) that the window turns into its palette. Std only.

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `Term::new(cols, rows) -> Term`, `Term::with_scrollback(cols, rows, lines)` | a blank screen; `cols` is clamped to `1..=MAX_COLS` and `rows` to `1..=MAX_ROWS`; the scrollback holds at most `lines` lines (default `DEFAULT_SCROLLBACK`, at most `MAX_SCROLLBACK`) and at most `MAX_SCROLLBACK_CELLS` cells together |
| `feed(&mut self, &[u8])` | what the program wrote; any cutting of the stream into calls gives the same state as one call; an empty call changes nothing |
| `resize(&mut self, cols, rows)` | the new size (clamped as above); see "Resize" |
| `take_reply(&mut self) -> Vec<u8>` | the bytes the program asked for (the answers of "Replies"), oldest first, and the buffer is empty after the call; the caller writes them to the program's input |
| `size() -> (u16, u16)` | `(cols, rows)` |
| `cursor() -> (u16, u16)` | the cursor's `(row, col)` on the screen shown, whether it is hidden or not |
| `view(&self, scroll: usize) -> View` | the rows to show, `scroll` rows above the bottom (clamped to what the scrollback holds, and to 0 on the alternate screen) |
| `View { lines: Vec<Vec<Cell>>, wrapped: Vec<bool>, top: usize, total: usize, scroll: usize, cursor: Option<(u16, u16)> }` | exactly `rows` lines of exactly `cols` cells; `wrapped[i]`: the line goes on in the next (the text was wrapped by the right edge, not ended by the program); `top`: the absolute index of the first line (0 is the oldest line of the scrollback); `total`: the absolute count (scrollback + rows); `scroll`: the distance used; `cursor`: the cell of the cursor, `None` while it is hidden or the view is scrolled away from the bottom |
| `text_between(&self, from: (usize, u16), to: (usize, u16)) -> String` | the text of a selection: positions are (absolute line, column), either order, both ends included; a line's trailing blanks are cut, a wrapped line joins the next without a break, the others are joined with `\n`, a tail cell adds nothing; positions past the end are clamped. Valid until the next `feed` or `resize` |
| `modes() -> Modes` | `Modes { app_cursor, app_keypad, bracketed_paste, cursor_visible, alt_screen, autowrap, origin, insert, focus_events, mouse: MouseMode, mouse_sgr }`; `MouseMode { Off, Press, Drag, Any }` |
| `title() -> &str` | the title the program set (OSC 0 or 2; OSC 1 sets it too), empty at first |
| `generation() -> u64` | moves up whenever a `feed` of one byte or more, or a `resize` that changes the size, may have changed what is shown; never goes down |
| `bells() -> u64` | how many bells (`BEL` outside a string) the program rang |
| `Cell { ch: char, fg: Color, bg: Color, attrs: Attrs }`, `Cell::BLANK` | a space, default colours, no attributes |
| `Color { Default, Indexed(u8), Rgb(u8, u8, u8) }` | indexes 0 to 7 are the dark colours, 8 to 15 the bright ones, 16 to 255 the cube and the greys of xterm |
| `Attrs` (a `u16` of flags: `BOLD DIM ITALIC UNDERLINE BLINK INVERSE HIDDEN STRIKE WIDE TAIL`), `contains`, `bits` | `WIDE`: this cell holds a character two cells wide and the next cell is its `TAIL`; a tail has `ch == ' '` and is drawn as nothing |
| `char_width(char) -> u8` | 0 (zero width: a combining mark, a joiner, a variation selector, a control), 1 or 2 (East Asian wide and fullwidth, Hangul, most emoji) |
| `Key { Char(char), Enter, Backspace, Tab, Escape, Up, Down, Left, Right, Home, End, PageUp, PageDown, Insert, Delete, F(u8) }`, `Mods { shift, ctrl, alt }` | the keys the window gives; a key the person types as text is `Char` |
| `encode_key(Key, Mods, &Modes) -> Vec<u8>` | the bytes for the program; empty when the key has none |
| `encode_paste(&str, &Modes) -> Vec<u8>` | the bytes of a paste, see "Keys" |
| `MouseEvent { kind: MouseKind, col: u16, row: u16, mods: Mods }`, `MouseKind { Press(Button), Release(Button), Move(Option<Button>), WheelUp, WheelDown }`, `Button { Left, Middle, Right }`, `encode_mouse(MouseEvent, &Modes) -> Vec<u8>` | the mouse as the program asked for it (`col` and `row` are 0-based cells); empty when the mode does not report it |

## The screen

**Cursor and the pending wrap.** The cursor is `(row, col)`, 0-based. A printable character is put at the cursor and the cursor moves right. At the last column, with auto-wrap on (the default), the cursor stays on the last column and a wrap is *pending*: the next printable character first moves to column 0 of the next row (scrolling if the cursor is on the bottom row of the scroll region), and marks the row it left as wrapped. Any cursor movement, a carriage return, a line feed, an erase of the line or the screen clears the pending wrap. With auto-wrap off the character replaces the last cell. A character of width 2 that finds one cell left wraps first (the last cell stays blank); with auto-wrap off it is dropped. Writing a character over one half of a wide character blanks the other half. In insert mode (4) the cells right of the cursor move right and the last one is lost. A zero-width character is dropped (decision D2). Printing uses the current pen (colours and attributes of SGR) and the character set in use.

**The controls.** `BEL` counts a bell. `BS` moves left one (not past column 0). `HT` moves to the next tab stop, or the last column; the tab stops are every 8 columns, `HTS` (`ESC H`) sets one, `TBC` (`CSI 0 g` and `CSI 3 g`) clears the one here or all. `LF`, `VT` and `FF` move down one row, scrolling at the bottom of the scroll region (a line feed does not return to column 0: the pty's line discipline sends `CR LF`). `CR` goes to column 0. `SO` and `SI` use the character set G1 and G0. The other controls (and the 8-bit C1 controls, which are not valid UTF-8 here) are ignored.

**Escape sequences.** `ESC 7` and `ESC 8` (and `CSI s`, `CSI u`) save and restore the cursor with its pen, its origin mode, its character sets and its pending wrap; restoring with nothing saved goes to the home position with the default pen. `ESC D` index, `ESC E` next line, `ESC M` reverse index (scrolls down at the top of the scroll region), `ESC c` full reset (the screen, the scrollback, the modes, the tab stops, the title kept), `ESC =` and `ESC >` the keypad modes, `ESC ( 0`, `ESC ( B`, `ESC ) 0`, `ESC ) B` the character sets (ASCII and DEC line drawing: `` ` `` to `~` become the line-drawing characters). Any other escape sequence is skipped to its end.

**CSI** (parameters 0 to 65535, saturating; at most `MAX_PARAMS`, more are dropped; a missing or zero parameter takes the default of the sequence). Cursor: `A B C D` (up, down, right, left; stop at the edge, and at the scroll region's margin when the cursor is inside it), `E F` (next and previous line), `G` and `` ` `` (column), `d` (row), `H` and `f` (row and column), `a` and `e` (relative), `I` and `Z` (tab forward and back). Erase, with **the current background colour and no attributes** (xterm's rule): `J` (0 to the end, 1 to the start, 2 all, 3 the scrollback too), `K` (0, 1, 2), `X` (n characters). Edit: `@` insert and `P` delete characters, `L` insert and `M` delete lines (inside the scroll region only), `S` and `T` scroll up and down in the region, `b` repeat the last printed character (at most `cols * rows` times). Region: `r` sets top and bottom (1-based; an empty or reversed region is ignored; the cursor goes home, to the region's corner in origin mode). Modes: `h` and `l` with no prefix: 4 insert; with `?`: 1 cursor keys, 6 origin, 7 auto-wrap, 25 cursor shown, 47 / 1047 / 1049 the alternate screen, 1000 / 1002 / 1003 the mouse, 1004 focus events, 1006 the SGR mouse form, 2004 bracketed paste; every other mode is accepted and ignored. SGR `m`: 0 reset; 1 bold, 2 dim, 3 italic, 4 underline (also 21), 5 and 6 blink, 7 inverse, 8 hidden, 9 strike; 22 to 29 turn them off (22 bold and dim); 30 to 37 and 90 to 97 the foreground, 40 to 47 and 100 to 107 the background, 39 and 49 the default; 38 and 48 followed by `5;n` (palette) or `2;r;g;b` (true colour), also in the colon form (`38:5:n`, `38:2::r:g:b`, `38:2:r:g:b`); a malformed 38 or 48 (a missing argument, an index or a component over 255, a selector other than 2 and 5) changes nothing, and the rest of the parameters are still read. Reports: see "Replies". Every other final byte, and every sequence with a private prefix (`<`, `=`, `>`) or an intermediate other than the ones named, is skipped to its end with no effect and no reply.

**The alternate screen.** `?1049h` saves the cursor, shows a blank alternate screen and clears it; `?1049l` shows the main screen as it was and restores the cursor. `?47` and `?1047` show and leave the alternate screen without saving the cursor. The alternate screen has no scrollback, and a line that scrolls off it is gone. The modes, the cursor, the pen and the tab stops are one set for both screens; the scroll region is reset to the whole screen at every switch.

**The scrollback.** A line that scrolls off the top of the main screen, when the scroll region is the whole screen, goes to the scrollback (stored without its trailing blank cells); a line that scrolls out of a smaller region does not. The oldest lines go when the count of lines or of cells is over the bounds (decision D3). `CSI 3 J` empties it.

**Resize.** The rows and the columns are cut or padded with blank cells, with no reflow (decision D1). When the screen becomes shorter, the top rows go to the scrollback (main screen only) so that the cursor row stays on the screen; when it becomes taller, blank rows are added at the bottom. The cursor is clamped into the new size, the scroll region is reset to the whole screen, the tab stops beyond the old width are every 8, a pending wrap is cleared (when the screen becomes wider the cursor moves to the cell after the old last one, where the next character was to go). A wide character cut in two by the new width is blanked.

**Replies** (decision D5). `CSI 5 n` answers `ESC [ 0 n`; `CSI 6 n` answers `ESC [ row ; col R` (1-based, relative to the scroll region in origin mode); `CSI c` and `CSI 0 c` answer `ESC [ ? 62 ; 22 c`. Nothing else is answered. The reply buffer holds `MAX_REPLY` bytes; the answers that do not fit are dropped.

**Strings.** `OSC 0`, `1` and `2` set the title to the text after the first `;` (at most `MAX_TITLE` characters, control characters removed); every other OSC is ignored (OSC 52, the clipboard, on purpose). An OSC ends with `BEL` or `ESC \`; one longer than `MAX_OSC` bytes is ignored whole. `DCS`, `SOS`, `PM` and `APC` strings are skipped to `ESC \` with nothing stored. `CAN` and `SUB` cancel a sequence or a string, and an `ESC` inside one begins a new sequence (the `ESC \` that ends a string is the only one that does not).

**UTF-8.** The stream is UTF-8, decoded as it arrives (a character cut between two calls is completed by the next). An invalid byte, an overlong form, a surrogate, a value over U+10FFFF, or a sequence cut short by a byte that is not a continuation gives one U+FFFD for the bad part, and the byte that showed it is read again as the start of what follows.

## Keys

`encode_key` (the sequences of xterm; `m` below is `1 + 1 (shift) + 2 (alt) + 4 (ctrl)`, minus the modifiers not held, that is `1` plus the sum of the held ones): a `Char` is its UTF-8; with Ctrl, a letter is the control code `1..=26` (`Ctrl+a` is 1), Space and `@` are 0, `[` 27, `\` 28, `]` 29, `^` 30, `_` 31 and `?` 127, and any other Ctrl+character is the character; with Alt the bytes are preceded by `ESC`. `Enter` is `CR`; `Backspace` is 127 (Ctrl: 8; Alt: preceded by `ESC`); `Tab` is `HT` and Shift+Tab is `ESC [ Z`; `Escape` is 27 (Alt: `ESC ESC`). `Up`, `Down`, `Right`, `Left`, `Home` and `End` are `ESC [ A B C D H F`, or `ESC O` and the same letter when the cursor-keys mode is on (and no modifier is held); with a modifier they are `ESC [ 1 ; m` and the letter. `Insert`, `Delete`, `PageUp` and `PageDown` are `ESC [ 2 ~`, `3 ~`, `5 ~`, `6 ~` (with a modifier `ESC [ n ; m ~`). `F1` to `F4` are `ESC O P Q R S` (with a modifier `ESC [ 1 ; m P Q R S`); `F5` to `F12` are `ESC [ n ~` with n = 15, 17, 18, 19, 20, 21, 23, 24 (with a modifier `ESC [ n ; m ~`); `F(0)` and `F(13)` and up have no bytes.

`encode_paste`: the text with every `ESC` removed (decision D4: the text cannot end a bracketed paste, and carries no sequence); with bracketed paste on it is `ESC [ 200 ~`, the text as it is (line breaks stay), `ESC [ 201 ~`; with it off, every `CR LF` and every `LF` becomes `CR` (a terminal sends a return, not a line feed).

`encode_mouse`: nothing when the mode is `Off`; `Press` reports presses, releases and the wheel; `Drag` also reports a move with a button held; `Any` also reports every move. The button code is 0, 1, 2 for left, middle, right; `+32` for a move; 64 and 65 for the wheel up and down; `+4`, `+8`, `+16` for Shift, Alt, Ctrl; coordinates are 1-based. SGR form (`mouse_sgr`): `ESC [ < code ; x ; y M`, a release `m` with the button's own code. The plain form: `ESC [ M` and the three bytes `32 + code`, `32 + x`, `32 + y` (a release has code 3); a coordinate that does not fit a byte (over 222) gives nothing.

## The limits (named constants, with their reasons)

| Constant | Value | Reason |
|---|---|---|
| `MAX_COLS` | 500 | wider than any window the program draws (a 4K screen at a small font is about 450 columns); bounds the screen at `MAX_COLS * MAX_ROWS` = 100,000 cells (1.6 MB) |
| `MAX_ROWS` | 200 | the same, for the height |
| `DEFAULT_SCROLLBACK` | 2,000 lines | an ordinary session's history |
| `MAX_SCROLLBACK` | 20,000 lines | what a caller may ask for |
| `MAX_SCROLLBACK_CELLS` | 600,000 | about 10 MB: a wide window cannot make the history larger than that (decision D3) |
| `MAX_PARAMS` | 32 | a sequence has a handful; the room stops a stream of parameters |
| `MAX_OSC` | 4,096 bytes | a title is a line; a longer string is ignored whole |
| `MAX_TITLE` | 256 characters | what a tab can show |
| `MAX_REPLY` | 4,096 bytes | a program that asks and never reads cannot grow the buffer |

## Invariants

1. **It never panics, whatever the bytes and whatever the sizes:** random bytes in random chunks, every single-byte change of a long sample, every prefix, sizes from 1 x 1 to the maxima, a resize between any two calls. The state stays valid (2 to 5 below).
2. **Cutting does not matter:** the same bytes cut at every point (and into single bytes) give the same cells, cursor, modes, title, replies and counters as one call.
3. **The cursor is inside the screen** (`row < rows`, `col < cols`) after every call, and the scroll region is always `top < bottom < rows` or the whole screen.
4. **Every row has exactly `cols` cells** on the screen, and a view has exactly `rows` lines of exactly `cols` cells.
5. **A `WIDE` cell is followed by a `TAIL` cell, and a `TAIL` cell is preceded by a `WIDE` cell:** never half a wide character, in the screen, after a resize, and in the scrollback lines a view pads or cuts to the width.
6. **Memory is bounded by the limits above:** the screen by `cols * rows` cells, the scrollback by `MAX_SCROLLBACK_CELLS`, the title, the string being read and the replies by their constants; a string that never ends, a million parameters, and a million `CSI`s do not grow anything.
7. **Deterministic:** the same bytes and calls give the same result; no clock, no random.
8. **A reply is only what "Replies" lists:** no byte a program writes makes the component write anything else to the program, and a pasted text cannot carry an escape sequence.

## Contract tests (`tests/contract.rs`, `tests/screen.rs`, `tests/keys.rs`, `tests/invariants.rs`)

One test per public name, per section above and per invariant; each test's name says the sentence it proves. A mutation harness breaks the code in 25 or more places and every break must fail a test (recorded in the plan's results).

## Dependencies and direction

None. `lico-vt` is a leaf; the host (P02) and the window (P03) use it, it uses nothing of them. Version rule: major when a name or a meaning of this contract changes, minor when a name or a sequence is added, patch when nothing in the contract changes.

## Not here (and said so)

A reflow of lines on a resize (D1); a combining character kept in its cell (D2); the answers to the version, colour, keyboard-protocol and mode queries; sixel and images; hyperlinks (OSC 8); the clipboard sequence (OSC 52); a pty, a child process, a thread, a font and a palette (the host and the window).
