# M20 — Moli Is The Browser Engine, As A Versioned Lico Component

- Status: P01 to P03 done, release `0.7.0` ([report](../../../reports/202611/20261117/20261117_34_M20_moli_engine_jpeg_frames_and_0_7_0_report.md)); M21 waits for the owner. Every approval is the LEAD's under the owner's delegation, to be confirmed by the owner.
- Version: 1.0.0
- Project: Lico
- Updated: 2026-11-17
- Base: the M17-P12 tip `8b7b420`
- Implementation: `phase/M20-P01-moli-engine` (`23d0562`), `phase/M20-P02-jpeg-frames` (`fe313f9`), `phase/M20-P03-release-0-7-0` (stacked; nothing pushed or merged)
- Version of the program: `0.7.0` (released, installed in `~/bin/lico`)

## Goal

The browser tab runs on Moli instead of Obscura, and the engine is a component that Lico versions itself, so that a later change to the engine (a new upstream release, a patch of ours, another engine) does not reach the host except through a contract that Lico owns.

## The owner's words (the source, in the chat, 2026-11-17)

1. "As another alternative, look into moli and put it in Obscura's place, as an experiment." The trial: [the trial note](../../../reports/202611/20261117/20261117_31_M17_moli_trial_note.md).
2. "Make moli the default; the biggest problem, the large frames, has to be solved. Obscura is scrapped." This milestone.
3. "If we can take Moli's strengths and make it our own, that may be better." and "But if we change the code there may be dependency problems later, so we must treat moli or obscura as one component and manage the version internally."

## Decisions (the lead's, to be confirmed)

- **Moli is the only engine of the tab. Obscura is removed** (its pins, its install path, its stealth switch). The stealth setting of M17-P12 goes with it: Moli has no stealth option, and the owner may object to the removal. The old Obscura folder in the owner's home is left alone and named in the report.
- **The engine is the component `lico-engine`** (it replaces `lico-engine-pack`). It holds: the upstream (name, release, source commit, licence, archive and file hashes), a ledger of Lico's patches (empty in M20), Lico's own version number of the engine and the number of the ENGINE CONTRACT (how the host starts the engine, finds its socket, and what frames it gets). The host depends on the contract only; at start it refuses an engine whose contract number is not the one it speaks.
- **M20 ships the official Moli binary, pinned by hash, with no patch.** The things a patch would improve are met on Lico's side: the large frames by JPEG (P02), the missing socket token by the fence and the guard plus an honest line in the tab, the fonts and the CA bundle by the host.
- **The own build is a separate milestone (M21), not part of 0.7.0:** a recipe that builds Moli from a pinned source with Lico's patches, the first patches (PNG compression in `moli-image`, a token on the CDP socket), a way to follow upstream releases, and a place to publish our build. It needs the owner's word (publication) and a measurement of the build (time, disk, the V8 archive pin) that has not been done.

## Phases

| Phase | Subject | Plan and state | Status |
|---|---|---|---|
| M20-P01 | `lico-engine`, the Moli adapter, Obscura and stealth removed | [Plan](../../../plans/202611/20261117/20261117_33_M20-P01_moli_engine_component_plan.md) | done (`23d0562`) |
| M20-P02 | JPEG frames: a bounded baseline JPEG decoder in `lico-frame` 0.2.0, the host asks for JPEG | no separate plan (the steps are in the report, section 4) | done (`fe313f9`) |
| M20-P03 | spec, guide, README, CHANGELOG, size, graph, accept, the whole suite, one real Moli install, release `0.7.0` | no separate plan (the report, sections 5 to 8) | done (`lico-proxy` 0.4.1 refusal-order fix found in its run) |
| M21 | the own build with a patch ledger | not planned yet; the owner's word first | idea |

## What this milestone does not do

- It does not push, open a pull request, merge, or write to Moli's project (a request for a token option or for PNG compression is a publication: the owner's call).
- It does not make the engine's 75 to 138 MiB count against Lico's own budgets (they bind Lico's own modules only).
- It does not add a CDP token: Moli's socket has none (the trial note, section on gaps); the tab says so.
