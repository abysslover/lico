# lico-browser-view 0.8.0 — specification

Written with the P04 plan (M17-P04); approved by the lead under the owner's delegation (the owner away), mail 147. Marks: **MEASURED** (M17-P03: the engine sends PNG frames, a still page one frame, and needs an acknowledgement per frame; `Input.insertText` types committed Korean; there are no composition events), **PROPOSED** (the shapes of the intents and effects: the host adapter of a later phase carries them out and may ask for changes).

## Purpose

The display-free model of one browser tab. It is driven with the person's intents and the engine's events and answers with data to draw and **effects** for the host adapter to carry out. It opens no socket, draws nothing, starts no thread and reads no clock (the time is given to `show` and `tick`). It depends on `lico-frame` only (to decode the frame that is shown).

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `parse_address(&str) -> Result<String, UrlError>`, `UrlError { Empty, TooLong, Scheme, Credentials, Invalid }` | what a person typed as an `http` or `https` address; no scheme means `https://` plus the text (`host:port` is not a scheme); the scheme is lower-cased and the rest is left as typed; `file:`, `javascript:`, `data:`, `about:`, `ftp:`, `mailto:`, `view-source:`, `chrome:`, `blob:`, `ws:` are `Scheme`; a user name or password (`@` in the authority) is `Credentials`; no host, a bad port, a space, a control character, a backslash or one of ``<>"{}|^` `` in the host, an unbalanced `[` are `Invalid`; over `MAX_URL` is `TooLong` |
| `clean(text, max_chars) -> String` | text from a page or the engine made safe to show: control, bidirectional-override and zero-width characters removed, cut at `max_chars` with `…` as the last |
| `display_address(&str, max_chars) -> String` | the address as the address bar shows it while the bar does not have the keys (0.8.0): `https://` dropped, an address longer than `max_chars` characters cut in the middle so that the host name (with its slash) and the end stay, never longer than `max_chars`, counted in characters |
| `AddressField`: `text`, `cursor`, `preedit`, `display`, `set_text`, `insert`, `backspace`, `delete`, `left`, `right`, `home`, `end`, `set_preedit`, `clear_preedit`, `commit` | the address row's text field, in characters (Korean included); control characters never enter; at most `MAX_URL` characters; a pre-edit is shown at the cursor and is not part of the text |
| `Tab::new()`, `phase`, `loading`, `url`, `title`, `history`, `can_back`, `can_forward`, `shown`, `viewport`, `viewport_capped`, `no_font`, `focus`, `focus_address`, `focus_page`, `address`, `address_mut` | the state to draw |
| `show(area, now_ms)`, `hide()`, `resize(area)`, `submit_address(&str)`, `back()`, `forward()`, `reload()`, `tick(now_ms)` | the intents, each answering `Vec<Effect>` |
| `engine_event(EngineEvent) -> Vec<Effect>` | the engine's events: `Started`, `Navigated(url)`, `LoadStarted`, `LoadFinished`, `Title(text)`, `Frame { id, png }`, `Screenshot(png)`, `TargetGone`, `Crashed`, `Missing { expected_sha256, release }`, `NoFontFound` |
| `paint() -> Option<&Image>`, `dropped_frames`, `too_big_frames`, `decode_errors`, `decodes`, `decoded_bytes`, `kept_bytes` | the picture of the shown tab, decoded now if it has not been; the counters and the memory held |
| `map_point(x, y)`, `mouse(...)`, `key(...)`, `text(&str)` | the person's input mapped to the page (`Effect::Mouse`, `Effect::Key`, `Effect::InsertText`) |
| `Effect { SetViewport, StartScreencast, StopScreencast, Ack(id), CaptureScreenshot, Navigate(url), Reload, Mouse, Key, InsertText }`, `Phase { Starting, Ready, Loading, Absent { expected_sha256, release }, Failed(msg) }`, `Focus { Page, Address }`, `MouseKind`, `Button`, `KeyKind` | the words |

## The limits (named constants, with their reasons)

| Constant | Value | Reason |
|---|---|---|
| `MAX_HISTORY` | 100 | a person's back and forward, not an archive (the lead's figure) |
| `MAX_URL` | 2048 | a real address is a few hundred characters; browsers stop near 2,000 |
| `MAX_TITLE` | 200 characters | a tab label is short; the rest of a long title is not shown |
| `MAX_FRAME_BYTES` | 4 MiB | the cap of `lico-ws` and `lico-frame`; the first engine's largest frame was 69,612 bytes; Moli's PNG frame of a 1920 x 1200 page is 3.77 MB and its JPEG (quality 90) 132 KB |
| `MAX_VIEWPORT_W` x `MAX_VIEWPORT_H` | 1920 x 1200 | bounds one decoded picture to 9.2 MB in Lico's process (the owner's rule: Lico's own memory counts) |
| `MIN_VIEWPORT` | 64 | a page cannot be laid out in less |
| `FALLBACK_AFTER_MS` | 500 | a still page sends its one frame at once, so half a second without a frame is a slow moment: one screenshot is asked for |
| `MAX_INSERT` | 4096 characters | the most committed text sent to the page at once |

## Invariants

1. **Acknowledge at once, keep the newest, decode only what is shown (lead's decision):** every `Frame` is answered by `Ack(id)` whatever happens to it (kept, too big, tab hidden); only the NEWEST compressed frame is kept (at most `MAX_FRAME_BYTES`; a larger one is counted in `too_big_frames` and not kept); a frame that replaces one that was never painted is counted in `dropped_frames`; decoding happens in `paint`, only for a shown tab and only when the kept frame is newer than the decoded one; a frame that does not decode is counted, the old picture stays, and the same bad frame is not tried again.
2. **Memory in Lico's process is bounded and visible:** at most one decoded picture (4 bytes a pixel, at most 1920 x 1200) and one compressed frame (at most 4 MiB) per tab; `hide` frees the picture and keeps the compressed frame; `decoded_bytes` and `kept_bytes` say what is held.
3. **Show and hide:** `show` returns `SetViewport` and `StartScreencast` once the engine is up (`Ready` or `Loading`), nothing before; `hide` returns `StopScreencast` only if a screencast was started; a hidden tab is not resized, paints nothing, takes no pointer, key or text. The viewport is the area clamped to `MIN_VIEWPORT`..`MAX_VIEWPORT`; `viewport_capped` says the area was larger than the cap (the painter shows that).
4. **No first frame, one screenshot:** after `show`, if no frame or screenshot has arrived within `FALLBACK_AFTER_MS`, `tick` returns `CaptureScreenshot` once; a new `show` starts the wait again.
5. **History:** at most `MAX_HISTORY` entries, the oldest dropped; a new address after a back drops what was forward; the same address twice in a row is one entry; back and forward move an index and answer `Navigate` (the model keeps its own history, the engine's is not used); what the engine reports (`Navigated`, a link the person clicked) is added.
6. **Nothing a page says is trusted:** the address and the title the engine reports are cleaned before they are kept; an address typed by the person is parsed by the rules above; an engine report does not overwrite what the person is typing in the address row while it has the focus.
7. **Absent, failed, starting, installing:** with the engine `Absent` or `Failed` no effect is produced for an address, a pointer, a key or a text; an address typed while `Starting` or `Installing` is kept and navigated when `Started` arrives.
8. **Commit-only Korean in the page:** `text` sends committed text (control characters removed, at most `MAX_INSERT`); a pre-edit lives in the address field only; keys and text go to the page only while it has the focus, the pointer always reaches the page inside the viewport.
9. No thread, no file, no environment variable, no clock, no socket, no `unsafe`.

## Versioning and budget

`0.1.0` while the effects have not met the host adapter; `0.8.0` (M22-P05a) takes the click ring out (`click_mark`, `MARK_RADIUS`, `MARK_THICKNESS`), adds `display_address` and moves the help's sentences from "the header" to "the bottom row"; `0.7.3` (M20-P02) says in the comments that a `Frame` holds a JPEG or a PNG (the host asks Moli for JPEG; the field keeps the name `png`) and gives the sizes behind the cap; `0.7.1` adds two help sentences (M17-P12); `0.7.0` adds the cues of a live page (M17-P11); `0.6.0` adds the install states (M17-P10); `0.5.0` adds the frame reads; `0.4.0` adds reasons and help; `0.3.0` adds the local tab; `0.2.0` adds the notices (M17-P06, approved by the lead under the owner's delegation, mail 150); semantic after. Budget: the default 256 MiB and 30 s.

## Notices (0.2.0)

`EngineEvent::FetchRefused { status, host }` (the fetch guard's own 403, 502, 504; any other status is named), `EngineEvent::LocalAddressRefused` (the engine's own refusal of an address on this computer or a private network, a number or the name `localhost`: P06 measured that it refuses both before it asks the guard; the notice says so and does not tell the person to type localhost) and `EngineEvent::UnpinnedEngine`; `Tab::notice() -> Option<&str>` and `Tab::unpinned() -> bool`; constants `MAX_NOTICE` 300 (a line, not a page) and `UNPINNED_NOTICE`. The notice carries the HOST only (cleaned like any text from outside, cut at 253 characters), never a path, a query or page content; a newer notice replaces the older; leaving the page (a new address, back, forward, reload) clears it; the unpinned mark never clears. Because the guard does not know which tab a fetch was for, the host adapter routes a refusal to the loading tabs with that host, else to every loading tab (said in the adapter's SPEC).

## The local tab (0.3.0)

`EngineEvent::LocalTab`, `Tab::local()` and `LOCAL_NOTE` ("this tab can reach services on this machine and no other site"): the person opened this tab for this machine's own services, by an explicit act (a flag of `browser.open`, later the tab menu), never from a page or a link, off by default. It is a property of the tab and stays across pages; the window shows `LOCAL_NOTE` in the tab's help. Added in M17-P06 by mail 151 (approved by the lead under the owner's delegation).

## Reasons and help (0.4.0)

`EngineEvent::FetchRefused { status, host, port, why }` with `Refusal { Other, NotPublic, LoopbackOnly, Port, Denied }` (the fetch guard's reason of a 403, mapped by the adapter): the notice says the true reason (a refused port names the port and the four that load; a private address says so; a loopback-only guard says the tab opens only this machine's own services; a Lico port says it belongs to Lico); a 403 whose reason is not known is a status, not a story. `Tab::help_lines()`: the identity sentence, the ports, what is refused, that a tab opened with the command line appears at the next list, the Korean input note, and the command for a first look at a real site (`lico browser open https://SITE`; the check itself is the owner's); a local tab puts `LOCAL_NOTE` first. Added in M17-P07 (mail 152, approved by the lead under the owner's delegation).

## Reading the newest frame (0.5.0)

`Tab::frame_seq()` (0 until a frame is kept; +1 for each kept frame; a frame over the cap changes nothing) and `Tab::newest_frame()` (the compressed bytes as they came): for a window that decodes the frame itself; neither decodes anything, and a hidden tab keeps both. Added in M17-P07 (mail 152).

## The help text (0.5.1, M17-P09, mail 153)

`help_lines()` also says that F1 shows and hides the help, that a refused or failed load shows a notice in the header and not an error page, and that Korean typed in a page's input fields may show as boxes (the engine's own drawing of form controls: MEASURED in P08, the text the page holds is right).

## The install states (0.6.0, M17-P10, mail 155)

Requested by the owner in the chat, 2026-11-17 (the engine installs itself when a tab is opened); the spec wording approved by the lead, to be confirmed by the owner. `Phase::Installing { size_mb, source, hash12 }` and `EngineEvent::Installing` (the tab says so, asks no question, produces no effect and takes no pointer, key or text; an address given meanwhile is kept for `Started`), `EngineEvent::Starting` (the engine is there and is being started), `EngineEvent::InstallFailed { why, security, expected_sha256, release }` (the phase becomes `Absent`, the pending address is dropped, and the notice is "the engine could not be installed: WHY", with "SECURITY: " in front for a hash that is not the pinned one or an archive entry that was refused; cleaned and cut like any notice). The model does not know a URL or a hash it could change: the adapter passes the pinned ones.

`help_lines()` also says that the first tab installs the browser engine by itself from its pinned release (about 72 MB, checked by SHA-256) without asking, and that a failed install is explained in the bottom row (0.8.0; the header before) with the way to install by hand.

## The cues of a live page (0.7.0, M17-P11)

Why: the engine draws no caret and no focus ring, so a page with the keys looked like a picture ("it feels like only a picture", the owner, 2026-11-17), and the address row kept the first address after a link. The helpers are pure (three of the four remain from 0.8.0).

- `keys_hint(Focus) -> &'static str`: "typing goes to the page (Ctrl+L: address)" or "typing goes to the address row (click the page to type in it)"; the window puts it in the bottom row of a ready or loading tab (the sentence that was the header before 0.8.0).
- `follows_page(row, last_put, page_url) -> bool`: the address row takes the page's address when that address is not empty, differs from the last one put in the row, and the row is empty or still holds the last one put (the person has not edited it); a row the person edited or cleared is never overwritten.
- ~~`click_mark(radius, thickness)`~~ (removed in 0.8.0, M22-P05a: the owner asked for the click ring to go; the window draws no ring).
- `help_lines()` also says that the engine draws no cursor (the bottom row says where typing goes), that contenteditable boxes take no typing while `<input>` and `<textarea>` do, and that some sites, Google search among them, refuse this engine and show their own block page.

The crosshair pointer over the page is the window's own drawing (the click ring of 0.7.0 is gone in 0.8.0) and cost nothing in the engine; Lico passes nothing to the engine for them. While the page has the keys the composition of the input method is shown in the bottom row ("composing: ..."), not in the address bar. Added in M17-P11 (the lead, under the owner's delegation).

## The help of the owner's stealth setting (0.7.1, M17-P12)

`help_lines()` also says that a search that works in this engine is `https://html.duckduckgo.com/html/?q=WORDS` (MEASURED in M17-P12) and that stealth is the owner's setting, off by default. The component knows nothing else of stealth: the setting, the engine argument and the header text belong to the host adapter (BROW-05).

## The help for the Moli engine (0.7.2, M20-P01)

`help_lines()` is the help of the engine the tab now runs (Moli, M20). It says "Moli engine" and no longer "Obscura"; it drops what was MEASURED of Obscura only (Korean boxes in an `<input>`, no `contenteditable` typing, no cursor, the Google block, the stealth setting); it keeps the line that a page may still see a Windows platform (MEASURED again with Moli: `navigator.platform` is `Win32`, its own discovery page says `Windows NT 10.0`; the user-agent Lico sets is honest and `Emulation.setUserAgentOverride` works), says that a site may refuse an automated browser, names a search to try (`https://www.google.com/search?q=WORDS`, MEASURED to load and to take Korean text in the Moli trial), says that the engine's own socket has no password (only programs on this machine can reach it: the known limit of the engine's contract), and gives the engine's size as "about 46 MB". The size is the pin's (`lico-engine::size_mb()`); this component does not know the pin, so the host has a test that the two agree. The test of the help lists what must be said and what must be gone.
