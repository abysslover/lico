# lico-engine 0.1.0 — specification

Replaces `lico-engine-pack` 0.1.0 (M17-P10). Written with the M20-P01 plan; requested by the OWNER in the chat on 2026-11-17 ("make moli the default ... obscura is scrapped"; "treat moli or obscura as one component and manage the version internally"); the wording approved by the lead, to be confirmed by the owner. Marks: **MEASURED** (the real archive's size, hashes and listing, the source commit of the tag; M20), **KNOWN FROM DOCUMENTATION** (the format of GNU `tar -tv`, curl's exit codes).

## Purpose

The browser engine as ONE record that Lico versions itself, and the pure decisions of installing it. The host depends on this record and on the engine CONTRACT below, never on the upstream's own numbers: a new upstream release, a patch of Lico's, or another engine is a change of this record and nothing else. It runs no process, reads no file and has no dependency; the adapter that runs `curl` and `tar` asks it.

## The record

| Name | Meaning |
|---|---|
| `UPSTREAM`, `UPSTREAM_NAME`, `RELEASE`, `SOURCE_COMMIT`, `LICENCE` | what the engine is built on: `lexmount/moli`, `v1.1.25`, the commit of that tag, `MIT OR Apache-2.0` |
| `ENGINE_VERSION` | Lico's own version of this record (the crate's version). It moves with the upstream release and with the ledger, never alone; a test pins `(ENGINE_VERSION, CONTRACT, LEDGER.len())` together |
| `CONTRACT` | the number of the engine contract the record satisfies (below). The host has one number; a record that names another does not build into it |
| `LEDGER` | Lico's patches on the upstream, one line each. EMPTY in 0.1.0: the pinned file is the upstream's own, byte for byte (the own build is a later milestone) |
| `label()` | what the tab and the list say: `Moli v1.1.25 (lico-engine 0.1.0, contract 1)`, with the number of patches when the ledger is not empty |

## The engine contract (number 1)

How the host and the engine meet; any engine behind this record must satisfy all of it:

1. Started as `ENGINE serve --host 127.0.0.1 --port N ...` with the arguments the host's `profile.rs` builds; one process for all tabs.
2. The socket path is found from the discovery page (`GET /json/version`, `webSocketDebuggerUrl`), never assumed.
3. The wire is CDP; the host's own methods (`lico-cdp`) are all it uses. The socket has no secret of its own: the tab says so.
4. Frames come as `Page.screencastFrame` images in a format `lico-frame` decodes within its bounds.
5. All traffic leaves by the HTTP proxy the host sets (the fetch guard), and the engine runs inside the fence.

## Pins and caps

| Name | Meaning |
|---|---|
| `ARCHIVE_URL`, `ARCHIVE_SHA256`, `ENGINE_SHA256`, `ARCHIVE_ROOT`, `MEMBER`, `ENGINE_FILE` | the pin: an https URL compiled in (no variable, file or argument reaches it), the digest of the archive and of the one file taken out of it (`MEMBER`, installed as `moli`) |
| `ARCHIVE_BYTES` 45,655,260, `ENGINE_BYTES` 112,357,968 | the real sizes (MEASURED) |
| `MAX_ARCHIVE_BYTES` 64 MiB, `MAX_FILE_BYTES` 128 MiB, `MAX_LISTING_BYTES` 16 KiB, `MAX_ENTRIES` 32 | the caps that stop a server that sends more |
| `hash12`, `size_mb`, `dir_name` | what the tab shows (twelve characters, "about N MB") and the install folder `moli-v1.1.25` |
| `verdict(actual, pinned) -> bool` | equal in every character, lowercase hexadecimal; an empty digest never matches |
| `check_listing(&str) -> Result<Vec<String>, Failure>` | the listing of `tar -tv` of the archive: every entry inside the archive's own root folder (a `./` in front, the root's own entry, and an archive without folder entries are fine), only folders and regular files, each file within the cap, the engine's `MEMBER` exactly once, no link of any kind, no device, no `..`, no absolute path, no name with a space or a NUL, at most 32 entries. Returns `[MEMBER]`: the rest of the archive is judged, never written |
| `Failure`, `Failure::text()`, `Failure::security()`, `manual_steps()` | why an install failed, in words; a hash mismatch and a bad entry are security notices; every failure is followed by the manual way |
| `failure_of_curl_exit(i32) -> Failure` | curl's exit codes (6 and 7 network, 28 timeout, TLS codes, 63 over the size cap, 23 no space, 18 and 56 truncated, 22 an error answer) |

## Invariants

1. The pin cannot be changed at run time; a new engine version is a new release of Lico (no self-update).
2. A digest matches only when it is the pinned one in full; no prefix, no case folding.
3. An archive is accepted only when its listing passes `check_listing`; only the one named member is ever extracted.
4. A change of the upstream release or of the ledger changes `ENGINE_VERSION`, the pins and the tests together, or the contract test fails.

## Versioning and budget

`0.1.0`. Budget: the default 256 MiB and 30 s.
