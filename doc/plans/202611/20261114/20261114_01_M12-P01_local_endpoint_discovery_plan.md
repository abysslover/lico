# M12-P01 Plan — Discovery Engine for This Machine

- Status: implemented (see report)
- Version: 1.0.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M12-P01-local-endpoint-discovery`
- Milestone: [M12](../../../milestones/202611/20261114/20261114_01_M12_local_llm_endpoint_discovery_milestone.md)

## Names

Wire op `llm.discover` (`ssh_host?`, `all_users?`; `state: host`, `mutates: False`), `POST /v1/llm/discover`, CLI `lico llm discover [--ssh-host HOST] [--all-users]`, tool `llm_endpoints` (optional `host`; asks the person per call), window entry `Session > Find Local LLM servers...`, host test option `--llm-proc-root DIR` (default `/proc`).

## The probe

- Listing: `/proc/net/tcp` and `tcp6`, state `0A`; default scope `same_user` through the inodes of `socket:[N]` links under `/proc/<pid>/fd` of the user's processes; `--all-users` lists every listening socket. Other OS: `supported: false`.
- Connect to the machine's own address only (`127.0.0.1`, `[::1]`, or the bound address). Connect timeout 300 ms, then listen 150 ms: a server that speaks first is `not-http` and receives nothing.
- `GET /v1/models`, then `GET /api/tags` when needed, `HTTP/1.0`, no body, no credential, no redirect, at most 64 KiB read. Classes `openai`, `ollama`, `http-other`, `not-http`, `unknown`; auth `none_needed`, `key_needed`, `unknown`. Model ids only (at most 50).
- Bounds: 64 sockets probed, 8 in parallel, 1.5 s per request, 8 s in total. Nothing is stored.

## Steps

- [x] S01: Pure move of `AsyncJob` and `AsyncDone` out of `gui/panel.rs` into `gui/jobs.rs` (suite green).
- [x] S02: Red tests: Rust units (tables, inode map, classes, caps, scan of truncated lists), Python host tests with fake servers, CLI/HTTP/socket parity, the tool with its approval card, the window.
- [x] S03: `llm` module (net, http, classify, run), wire, CLI, HTTP, schemas.
- [x] S04: Tool `llm_endpoints`; dialog `gui/llm_find.rs`.
- [x] S05: Report with the red output, mailbox.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Every class and auth state is reported from fake servers; the request set is exactly the two GETs | host tests that record what each fake server received |
| 2 | A speak-first server receives nothing; a redirect target is never requested; a silent server ends inside the budget | host tests |
| 3 | Default scope lists same-user sockets, `--all-users` all | fake `/proc` root tests |
| 4 | The tool asks the person; the dialog scans only on request | tool and window tests |
