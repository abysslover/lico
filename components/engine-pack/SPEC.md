# lico-engine-pack 0.1.0 — specification

Written with the M17-P10 plan; requested by the OWNER in the chat on 2026-11-17 (the engine installs itself when a tab is opened); the wording approved by the lead (mail 155), to be confirmed by the owner. Marks: **MEASURED** (the real archive's size and hashes, M17-P01 and P09), **KNOWN FROM DOCUMENTATION** (the format of GNU `tar -tv`, curl's exit codes).

## Purpose

The pinned browser engine release as data, and the pure decisions of installing it. It runs no process, reads no file and has no dependency; the adapter that runs `curl` and `tar` asks it.

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `RELEASE`, `ARCHIVE_URL`, `ARCHIVE_SHA256`, `ENGINE_SHA256`, `WORKER_SHA256`, `ENGINE_FILE`, `WORKER_FILE` | the pin; the URL is https and compiled in; nothing changes it: no variable, file or argument |
| `ARCHIVE_BYTES` 71,782,128, `MAX_ARCHIVE_BYTES` 80 MiB, `MAX_FILE_BYTES` 120 MiB, `MAX_LISTING_BYTES` 16 KiB | the size of the real archive and the caps that stop a server that sends more |
| `hash12`, `size_mb` | what the tab shows: the first twelve characters of a digest, "about N MB" |
| `verdict(actual, pinned) -> bool` | equal in every character, lowercase hexadecimal; an empty digest never matches |
| `check_listing(&str) -> Result<Vec<String>, Failure>` | the listing of `tar -tv` holds exactly the two named regular files (a `./` in front, and the archive's own `./` entry, are fine), each once, each within the cap, and nothing else: no `..`, no absolute path, no link, no device, no fifo, no other folder, at most four entries |
| `Failure`, `Failure::text()`, `Failure::security()`, `manual_steps()` | why an install failed, in words; a hash mismatch and a bad entry are security notices; every failure is followed by the manual way |
| `failure_of_curl_exit(i32) -> Failure` | curl's exit codes (6 and 7 network, 28 timeout, TLS codes, 63 over the size cap, 23 no space, 18 and 56 truncated, 22 an error answer) |

## Invariants

1. The pin cannot be changed at run time; a new engine version is a new release of Lico (no self-update).
2. A digest matches only when it is the pinned one in full; no prefix, no case folding.
3. An archive is accepted only when its listing is exactly the two named regular files.

## Versioning and budget

`0.1.0`. Budget: the default 256 MiB and 30 s.
