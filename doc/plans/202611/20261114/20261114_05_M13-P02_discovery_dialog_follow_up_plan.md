# M13-P02 Plan — Discovery Dialog Follow-Up: Cache, Speed, Fold, Readable Rows

- Status: planned
- Version: 1.0.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M13-P02-discovery-dialog-follow-up` (stacked on `phase/M13-P01-test-split-and-flaky`)
- Milestone: [M13](../../../milestones/202611/20261114/20261114_02_M13_discovery_follow_up_chat_list_polish_and_test_split_milestone.md)

## Objective

- Cache: the last result of each (host, all-users) pair is kept in the window's memory only (at most 8 entries, nothing on disk), shown as `found N min ago`; opening the dialog shows it without a scan; `Enter` and a `Refresh` button scan again. CLI, HTTP and the tool always scan fresh.
- Speed: measure first on fake servers of the owner's count (46), median and worst of 30. Then a two-stage engine: connect to all selected sockets and wait the 150 ms quiet period once for all of them in parallel, then GET only from the quiet ones, 8 at a time. The quiet wait is not shortened; every other bound stays (64 ports, 1.5 s a request, 8 s in all, 64 KiB, GET only). The remote Python script gets the same structure; parity is kept.
- Noise: servers that answered as OpenAI-compatible or Ollama with at least one model, and those that need a key, are listed; the others sit behind one folded line that opens on a click or a key. A row owned by `ssh` is labelled `via ssh` and never hidden by name.
- Readable rows: two lines per row at the default width, a selection with Up and Down, a detail area with the full model ids and host name, one plain line saying what "no model listed" means, the ssh host name wrapped and never cut.

## Steps

- [ ] S01: Red-first tests (window tests, a timing test, Rust units for the pure parts); the red output kept.
- [ ] S02: The "before" scan measurement on 46 fake ports.
- [ ] S03: Cache and Refresh in `gui/llm_find.rs`; dialog JSON fields.
- [ ] S04: Two-stage engine in `llm/mod.rs` and `llm/http.rs`; the same in `llm/remote_script.rs`; the "after" measurement.
- [ ] S05: Fold, `via ssh`, two-line rows, selection, detail area, plain line, wrapped host name; screenshots with a fake ssh result (70-character alias, 80-character model ids).
- [ ] S06: Report `107`.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Cached result shown without a scan; Refresh and Enter scan again | window test |
| 2 | Scan time measured before and after; every bound unchanged; the speak-first server still receives nothing | timing test, existing discovery tests |
| 3 | No-model servers folded; `via ssh` label; no row hidden by name | window test, Rust units |
| 4 | No cut text unreachable at the default width; the plain line present | window test, screenshots |
