# Provider adapters (M01-P03)

Provider paths are versioned data plus a mapping, never a guess. `codex/capabilities.json`
records what the installed Codex app-server offers (pinned to its exact version, with the hash
of the protocol schema the installed binary generates), how each client request, server request
and notification is handled, what is unsupported and why, and what is **unverified**.
`python -m lico_measure providers codex` regenerates it; a test fails when the installed
provider no longer matches the record.

Everything about an installed provider is read with a private empty home and a minimal
environment: no sign-in, no credential and no model turn. A provider's login state belongs to
the provider and is never read or copied by Lico.

`src/lico_protocol/codex_mapping.py` is the reference mapping from provider messages to host
records and approvals (one UTF-8 line per record, escape sequences and control characters
removed, an approval answer bound to exactly one provider request). The fixtures under
`tests/fixtures/codex_app_server/` are recorded sequences validated against the provider's own
schema; the owned host's adapter is tested against the same fixtures.

## Claude Code (second adapter)

`claude/capabilities.json` pins Claude Code 2.1.310 (the version the owner reported) and lists what is
mapped, refused, unsupported and **unverified**. The message shapes come from the provider's
documentation, not from a recording; `tests/fixtures/claude_stream/` says so, and the permission
request subtype it uses (`can_use_tool`) is an assumption the documentation does not confirm.
`claude/simulator.py` stands in for `claude -p --input-format stream-json` in tests. The adapter
(`hostd/src/claude.rs`) must produce the records `lico_protocol/claude_mapping.py` produces.
