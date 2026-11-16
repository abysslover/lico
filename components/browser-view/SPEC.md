# lico-browser-view 0.4.0 — specification

Written with the P04 plan (M17-P04); approved by the lead under the owner's delegation (the owner away), mail 147. Marks: **MEASURED** (M17-P03: the engine sends PNG frames, a still page one frame, and needs an acknowledgement per frame; `Input.insertText` types committed Korean; there are no composition events), **PROPOSED** (the shapes of the intents and effects: the host adapter of a later phase carries them out and may ask for changes).

## Purpose

The display-free model of one browser tab. It is driven with the person's intents and the engine's events and answers with data to draw and **effects** for the host adapter to carry out. It opens no socket, draws nothing, starts no thread and reads no clock (the time is given to `show` and `tick`). It depends on `lico-frame` only (to decode the frame that is shown).

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `parse_address(&str) -> Result<String, UrlError>`, `UrlError { Empty, TooLong, Scheme, Credentials, Invalid }` | what a person typed as an `http` or `https` address; no scheme means `https://` plus the text (`host:port` is not a scheme); the scheme is lower-cased and the rest is left as typed; `file:`, `javascript:`, `data:`, `about:`, `ftp:`, `mailto:`, `view-source:`, `chrome:`, `blob:`, `ws:` are `Scheme`; a user name or password (`@` in the authority) is `Credentials`; no host, a bad port, a space, a control character, a backslash or one of ``<>"{}|^` `` in the host, an unbalanced `[` are `Invalid`; over `MAX_URL` is `TooLong` |
| `clean(text, max_chars) -> String` | text from a page or the engine made safe to show: control, bidirectional-override and zero-width characters removed, cut at `max_chars` with `…` as the last |
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
| `MAX_FRAME_BYTES` | 4 MiB | the cap of `lico-ws` and `lico-frame`; the largest frame the engine sent was 69,612 bytes |
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
7. **Absent, failed, starting:** with the engine `Absent` or `Failed` no effect is produced for an address, a pointer, a key or a text; an address typed while `Starting` is kept and navigated when `Started` arrives.
8. **Commit-only Korean in the page:** `text` sends committed text (control characters removed, at most `MAX_INSERT`); a pre-edit lives in the address field only; keys and text go to the page only while it has the focus, the pointer always reaches the page inside the viewport.
9. No thread, no file, no environment variable, no clock, no socket, no `unsafe`.

## Versioning and budget

`0.1.0` while the effects have not met the host adapter; `0.4.0` adds reasons and help; `0.3.0` adds the local tab; `0.2.0` adds the notices (M17-P06, approved by the lead under the owner's delegation, mail 150); semantic after. Budget: the default 256 MiB and 30 s.

## Notices (0.2.0)

`EngineEvent::FetchRefused { status, host }` (the fetch guard's own 403, 502, 504; any other status is named), `EngineEvent::LocalAddressRefused` (the engine's own refusal of an address on this computer or a private network, a number or the name `localhost`: P06 measured that it refuses both before it asks the guard; the notice says so and does not tell the person to type localhost) and `EngineEvent::UnpinnedEngine`; `Tab::notice() -> Option<&str>` and `Tab::unpinned() -> bool`; constants `MAX_NOTICE` 300 (a line, not a page) and `UNPINNED_NOTICE`. The notice carries the HOST only (cleaned like any text from outside, cut at 253 characters), never a path, a query or page content; a newer notice replaces the older; leaving the page (a new address, back, forward, reload) clears it; the unpinned mark never clears. Because the guard does not know which tab a fetch was for, the host adapter routes a refusal to the loading tabs with that host, else to every loading tab (said in the adapter's SPEC).

## The local tab (0.3.0)

`EngineEvent::LocalTab`, `Tab::local()` and `LOCAL_NOTE` ("this tab can reach services on this machine and no other site"): the person opened this tab for this machine's own services, by an explicit act (a flag of `browser.open`, later the tab menu), never from a page or a link, off by default. It is a property of the tab and stays across pages; the window shows `LOCAL_NOTE` in the tab's help. Added in M17-P06 by mail 151 (approved by the lead under the owner's delegation).

## Reasons and help (0.4.0)

`EngineEvent::FetchRefused { status, host, port, why }` with `Refusal { Other, NotPublic, LoopbackOnly, Port, Denied }` (the fetch guard's reason of a 403, mapped by the adapter): the notice says the true reason (a refused port names the port and the four that load; a private address says so; a loopback-only guard says the tab opens only this machine's own services; a Lico port says it belongs to Lico); a 403 whose reason is not known is a status, not a story. `Tab::help_lines()`: the identity sentence, the ports, what is refused, that a tab opened with the command line appears at the next list, the Korean input note, and the command for a first look at a real site (`lico browser open https://SITE`; the check itself is the owner's); a local tab puts `LOCAL_NOTE` first. Added in M17-P07 (mail 152, approved by the lead under the owner's delegation).
