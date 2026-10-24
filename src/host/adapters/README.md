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
