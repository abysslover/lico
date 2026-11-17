# M19-P01: the memory guard (plan note and State for the lead)

- Version: 1.0.0, 2026-11-17. Source: lead mail 157. Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.
- Branch `phase/M19-P01-memory-guard`, stacked on the P10 tip `1c56353`. Release at the end: `0.6.0` (only if reached; otherwise the lead does it).
- Money: no stop at 230 or for a stage; the front-end refuses new requests at 235. Commit after every green slice and update "State for the lead" below at each commit.

## What the host already measures (READ)

`src/host/hostd/src/memory.rs`: for each running session the tree of its agent (`children` files, else a `/proc` scan) is summed into `Figure { private_kb, rss_kb, processes }` (`private_kb` = RssAnon + RssShmem, `rss_kb` = VmRSS), cached, refreshed only when a `status` call asks (no background timer), put into the status entry as `memory`. The window's row reads `memory.private_kb` and `memory.rss_kb` (`gui/row.rs`). RSS counts shared library and file pages in every process that maps them, so the sum over a tree over-counts shared pages; the number is honest for "what a process holds in RAM now" and not for "what freeing it would give back" (`private_kb` is nearer to that). The colour follows the LARGEST SINGLE process by RSS, as the owner asked, so the figure `max_rss_kb` is added to `Figure`.

## Design

- **Pure component `lico-memguard`** (this commit): levels (400 / 500 MB defaults, adjustable), the machine level (orange under 20 percent `MemAvailable`, red under 10), episodes (the question and the warning once for each red episode), `ClosedBy`, the one-at-a-time resume queue with a gap (default 20 s), the loop guard (twice in ten minutes), the idle test, the fixed warning texts.
- **Adapter** (next): `max_rss_kb` in `memory.rs` and the status entry; `closed_by` recorded by `session.close` (an optional argument `by`: `person` default, `guard`) and shown in the status entry and the registry; settings in an owner-only `memory.json` of the state folder (`warn_mb`, `limit_mb`, `auto_close`, `warn_main`), read when the host starts and on change of the file's mtime at a status call (no watcher).
- **Window**: colour of the session row's memory by `max_rss_kb` and in the bottom `mem` line; click on a red session asks once per episode "this session uses N MB (limit 500). End it and continue? The conversation resumes; tool jobs that were running are lost." with "end and resume" (`session.close` with `by: guard`, then `session.resume`; if the provider cannot resume, say so and do not close) and "cancel". New code goes in a new file under `gui/` (`ui.rs` is at 1,118 of its 1,120 lines).
- **Who is the main agent** (decision): the session the person marks; until one is marked, the session that created the others (the caller of `session.create` when it is a session). Reason: the host knows the caller of a tool call, and a mark by the person is the only rule that cannot be fooled by an agent. Warnings go to it as a message at its idle moment (the idle test of the component).
- **Gaps and safe defaults**: the resume gap 20 s; the grace before an auto-close 2 minutes; auto-close OFF; the warning to the main agent ON.

## Steps

1. [x] `lico-memguard` 0.1.0 with tests first; mutants 12 of 12 killed.
2. [x] `max_rss_kb` in the host's figure and status (`memory.max_rss_kb`); `closed_by` on `session.close` (`by`, a side table in `guard.rs`, shown in status, cleared by `session.resume`; not kept across a host restart: gap); protocol row.
3. [ ] Window: colour, `mem` line, the click message and its two buttons; screenshots in `private/scratch/202611/20261117/m19_p01_screens/`.
4. [ ] Slice B: the auto-close (global and per-session switch, idle only, loop guard, the line "memory N MB: closed and resumed automatically").
5. [ ] Slice C: the machine line, the warning before a new session, the warning to the main agent.
6. [ ] Slice D: "last activity N min ago".
7. [ ] Spec table "Memory guard", protocol rows, CHANGELOG, README, guide, milestone note; the small real measurement (eight fake sessions against `MemAvailable`); release `0.6.0`.

## State for the lead

- **Built and verified**: step 1 (component `lico-memguard`, `components/memguard`, 7 contract tests, 12 of 12 mutants killed, receipt recorded by `cargo xtask test lico-memguard --record`); step 2 (host unit tests `cargo test --offline -p lico --bin lico -- guard memory ipc`: 10 pass). The host crate does NOT use `lico-memguard` yet (it will in step 3).
- **Next**: step 3, the window (colour by `memory.max_rss_kb`, the click message, the two buttons: close with `by: guard`, then resume), using `lico_memguard::Limits`, `Episodes::ask` and `ResumeQueue`.
- **Commands**: `cargo test --offline -p lico-memguard`; `cargo xtask test lico-memguard --record`; `cargo xtask check`; host tests `cargo test --offline -p lico --bin lico memory`; heavy jobs go through `pri-heavy --label lico --`; window tests `PYTHONPATH=src:. private/venv/bin/python -m pytest -q tests/test_<name>.py` (rebuild the plain binary first with `python -m lico_measure.suite run <file>`; the window process is `private/build/host_hostd/release/lico`, set `lico_measure.captures.LICO` to test another build).
- **Known gaps / decisions**: see "Design"; the main-agent rule is decided here but not built; nothing of Slice B to D exists; the whole-suite margin was 94.7 MiB at P10 (keep new tests lean).
