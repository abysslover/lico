# Lico — a lightweight workbench for agentic workflows

Product specification 0.1.0 · 2026-10-23 · Draft for owner review. These are requirements and proposed acceptance budgets, not claims about an implemented product. Product name: Lico. License: MIT, selected by the owner.

## 1. Purpose

Lico gives people a small native GUI for persistent command-line agents. Agents can call tools, edit code, run commands and continue work in the background. People organize sessions, follow progress, provide input and answer approval requests. Closing a view does not stop its agent.

The name combines LIC, a rearrangement of CLI, with O for orchestration. A possible expansion is **Lightweight Interface for CLI Orchestration**. The primary description remains the plain-language subtitle above. Lico is an independent open-source project; provider names identify integrations and do not imply endorsement.

“Agentic” describes work executed through tools and a sequence of actions, with durable sessions and human supervision. It does not promise unbounded autonomy, a new model, or automatic success.

## 2. Scope and platform order

Linux is the first shipping target; Windows follows through explicit platform interfaces. Record the tested display server and desktop. Do not label native Wayland or Windows support complete from an X11 test. Keep terminal I/O, windowing, font/IME, keyring and process ownership behind platform interfaces.

The first usable version includes persistent Claude Code/Codex sessions, a small workspace/session navigator, a terminal or structured conversation view, input/approvals, account selection, reconnection and observable resource use. A structured provider view may coexist with a terminal fallback when the provider exposes a supported protocol.

Full IDE editing, embedded browsers, remote cloud provisioning, a plugin marketplace, remote multi-user access and a general DAG scheduler are outside the first release. Add capabilities through separately reviewed specifications and budgets. Native provider subagents may be shown when supported; that does not require creating an independent scheduling engine.

## 3. Separate execution from presentation

```text
GUI client ── authenticated local control/events ── session host
                                                     ├─ installed agent process
                                                     ├─ PTY or supported provider protocol
                                                     └─ durable event log / current screen state
```

| ID | Requirement |
|---|---|
| SES-01 | The session host owns agent processes, PTYs, output draining and session records independently of the GUI. GUI exit, crash, minimize, tab change or reload does not implicitly terminate agents or their tools. |
| SES-02 | Sessions have stable opaque IDs, an execution-generation identity, workspace/cwd, provider, account binding and explicit lifecycle states. The target name, workspace and account remain visible during input. Mutations validate the target ID and generation; stale targets are refused. |
| SES-03 | Distinguish working, idle, waiting for approval/input, disconnected and exited. Do not infer idle from a static screen or a quiet network interval. Preserve the last known state with its timestamp after disconnect. |
| SES-04 | Read background output continuously even without a GUI subscriber. Apply bounded queues/backpressure and durable sequencing so a hidden view does not fill a PTY pipe or lose output. Persist full history locally according to an explicit retention policy. |
| SES-05 | Reattach using a latest screen/state snapshot plus events after its sequence cursor. Read older history on demand. Do not replay an entire transcript to reconstruct each tab switch. Detect gaps and request a fresh snapshot. |
| SES-06 | Acknowledged input has a request ID and target generation. A reconnect does not blindly resend ambiguous input or tool approvals. Expose accepted, submitted and observed execution separately. |
| SES-07 | Existing external sessions are attachable only through a supported integration. No process adoption, stdin injection or new process is claimed to preserve the old session without actual read-back. A saved conversation resume is distinct from preservation of a live process, tool jobs and REPL state. |
| SES-08 | Prefer reuse of a verified existing host/protocol when licensing, account isolation and target identity meet this contract. Herdr is a candidate adapter, not a mandatory dependency. An unsupported runtime must be shown as unsupported, not simulated by a new unrelated conversation. |

## 4. UI lifecycle and memory

The number of background sessions must not imply the same number of renderers or resident view trees. The default idle policy reclaims presentation resources while allowing agents to work.

| UI state | Required behaviour |
|---|---|
| Visible | Maintain only the visible panes, virtualized history, bounded text/glyph caches and the events needed to draw them. |
| Hidden tab | Stop layout, painting and per-view polling. Release the tab's view tree and private render buffers/textures. Keep small navigation metadata and a replay cursor. Shared bounded caches may remain. |
| Reactivated | Recreate the view from the host snapshot and subsequent deltas; recover scroll position where the referenced history still exists. |
| All windows hidden | Permit a short warm grace interval, then release graphics resources or stop the GUI process. Keep the host and agents alive. The grace policy is configurable. |

Process freeze and swap-out are not the default background-agent policy. They can prevent useful agent work and do not describe UI resource disposal. OS paging may happen normally; correctness and switching latency must not depend on swapping a complete GUI image back in.

### Proposed performance acceptance

All numbers below are initial product targets. The first architecture experiment must measure them and either meet them or return an explicit budget decision before framework selection.

| ID | Target and measurement scope |
|---|---|
| PERF-01 | Idle GUI process tree RSS ≤50MiB with one visible synthetic session at a documented 1200×800 viewport. Report PSS separately. Include renderer/helper descendants and toolkit/driver resident pages; exclude agents, the host and shared external services as separate categories. |
| PERF-02 | With the same visible view, increasing 10→100 synthetic background sessions adds ≤10MiB to GUI RSS after inactive views are released. Record host memory independently; do not move a frontend buffer into a helper to hide its cost. |
| PERF-03 | Warm tab activation: p95 first correct frame ≤100ms over at least100 switches, including large-history and actively producing hidden sessions. Measure on the GUI/compositor path, not just API response time. |
| PERF-04 | GUI restart/reattach: proposed p95 first correct frame ≤500ms under a declared warm-filesystem condition. Record cold-start latency separately. Gracefully show a placeholder rather than blocking input against an uncertain target. |
| PERF-05 | Steady idle GUI CPU ≤0.5% of one core over a documented60s interval. No continuous animation/polling to make an idle screen appear live. Measure a noisy background-agent case separately. |
| PERF-06 | A bounded synthetic output stream continues while the GUI is absent. On reattach, persisted sequence counters and the latest snapshot show no gaps/duplicates. Keep the test's byte/rate/duration bounds in the report. |
| PERF-07 | Record binary/package size, peak RSS, idle RSS/PSS, startup/switch latency, font/display configuration and GPU allocation where observable. Treat package size, process RSS, physical memory attribution and GPU memory as separate metrics. |

Use repeatable synthetic workloads first, then a bounded installed-agent smoke test. Do not create model turns for a renderer benchmark. Reuse passed measurements unless a change, failure or mismatch justifies another run. A framework that misses the baseline budget requires an explicit design decision; “native” alone is not acceptance.

## 5. Providers and authentication

Provider capability is versioned data: transport, auth methods, session resume, approvals, account isolation and unsupported operations. Detect and pin the tested CLI/protocol version. A generic OAuth button must not imply that every provider offers a public OAuth integration.

| Provider path | Required initial approach |
|---|---|
| Codex | Evaluate the official app-server protocol for sessions/events and documented browser/device/API-key authentication. Enforce its OS-keyring credential mode where used and test the installed version. Support for experimental externally supplied tokens is a separate decision. |
| Claude Code | Delegate installed-CLI sign-in to the official CLI. Do not implement an independent subscription-token login by extracting another application's credentials or impersonating its identity. Validate any profile/isolation mechanism before advertising simultaneous accounts. |
| Anthropic API | Use documented API-key or supported cloud-provider authentication. Treat subscription use by an independent API client as a separate, documented capability rather than assuming native CLI login grants it. |
| Other providers | Implement through a capability adapter with documented endpoints and auth. Mark unavailable login methods clearly; no guessed endpoints or silent fallback that changes billing/account. |

For native OAuth implemented by Lico: use the external system browser, authorization-code flow with PKCE where supported, state validation and an exact redirect allow-list. Bind an ephemeral callback only on loopback for the lifetime of that login. Provide cancellation, timeout, callback-replay rejection, refresh/revocation handling and distinct failure states. Device authorization is offered only where documented. No embedded password capture.

Access/refresh tokens owned by Lico are keyring secrets. Serialize refresh per account, handle refresh-token rotation atomically and do not retry forever. Retrying authentication must not repeat an agent's last billable operation automatically.

## 6. OS keyring and multiple accounts

| ID | Requirement |
|---|---|
| AUTH-01 | Persist API keys and application-owned OAuth secrets only in an OS credential store. Linux uses a supported Secret Service implementation; Windows uses Credential Manager in its platform implementation. A locked/unavailable keyring fails closed with a useful unlock/setup action. No silent plaintext fallback. |
| AUTH-02 | The metadata DB contains opaque credential references and masked labels, never secret values. Keyring entries are namespaced by application, provider, account and credential kind. Do not store secrets in argv, URLs, shell history, project files, logs, crash reports or public fixtures. |
| AUTH-03 | Support several accounts of the same provider concurrently. Use stable local account IDs separate from user-editable labels. Two profiles for the same provider must not overwrite a global auth file or a shared keyring slot. |
| AUTH-04 | Bind each session to an explicit account and auth/profile generation. Changing the default affects new sessions; a running session keeps its binding. Account changes during a turn are refused or scheduled through an explicit supported boundary. |
| AUTH-05 | Verify runtime reuse against account, workspace/cwd, permissions and tool-state isolation. Do not start an engine for every UI tab; do not merge distinct accounts or tool contexts to reduce process count. |
| AUTH-06 | Show provider/account/model beside the input target. An expired account, limit or login failure never silently switches to another account or paid auth mode. Usage values name their provider source and freshness; unknown usage is not zero. |
| AUTH-07 | Removal/revocation identifies affected sessions and offers an explicit lifecycle action. An account deletion does not erase unrelated accounts, histories or running work. Local metadata removal and remote revocation are distinct outcomes. |
| AUTH-08 | Existing provider-managed credentials remain provider-managed. Do not migrate, copy, symlink or decrypt another application's credential store without an explicit supported flow. For adapters that cannot meet managed storage/isolation requirements, report the limitation and gate the feature. |

Required account tests: A/B concurrent login and request routing, independent refresh/revoke/logout, session binding after a default change, keyring locked/missing, refresh races, cancellation/replayed callback, and a canary secret absent from every collected nonsecret sink.

## 7. Control surface and event contract

One canonical host implementation serves GUI, CLI and a local REST API. Command IDs and event cursors are stable within the documented protocol version. Auth is enforced even on loopback. Prefer an owner-only OS IPC channel for the GUI; an optional HTTP listener defaults to loopback and validates credentials, Host and Origin. Do not expose remote access in the first release.

The following are proposed contracts to finalize in the approved implementation plan, not existing endpoints:

| Capability | CLI shape | REST shape |
|---|---|---|
| Host status/capabilities | `lico status --json` | `GET /v1/status` |
| Session list/create | `lico session list/create` | `GET/POST /v1/sessions` |
| Session detail/input | `lico session show/send ID` | `GET /v1/sessions/{id}`; `POST /v1/sessions/{id}/input` |
| Output/reconnect | `lico session watch ID --after N` | `GET /v1/sessions/{id}/events?after=N` and snapshot endpoint |
| Approval response | `lico approval answer ID` | `POST /v1/approvals/{id}/response` |
| UI view attach/detach | `lico view attach/detach` | `POST /v1/views`; `DELETE /v1/views/{id}` |
| Account metadata/login/logout | `lico account list/login/logout` | Account metadata and explicitly authenticated login/logout endpoints |
| Resource report | `lico resources --json` | `GET /v1/resources` |

Secret input uses a protected prompt or stdin/OS IPC, never a literal command argument. Control responses never return raw credentials. Define exit codes, status codes, output schemas, timeouts, idempotency behaviour and event-gap recovery. Maintain an API inventory and test cross-session/account target authorization.

## 8. Everyday interaction

Provide discoverable mouse controls as well as optional shortcuts. Show sessions grouped by workspace and their actual work/approval/error state. Use stable IDs/order so titles or status changes do not move the target beneath the pointer. Confirm target identity at dispatch.

The first view offers a clear selected session, provider/account/model, its latest output and input. Approval requests remain discoverable when their session is hidden. Background completion updates small status metadata and optional notifications without creating a renderer for the hidden session.

Treat terminal content, logs and workspace files as untrusted display data. Sanitize terminal escape features with side effects; explicitly handle hyperlinks, clipboard access and file-opening actions. Support Unicode, Korean IME composition, selection/copy/paste, wide/combining characters, resize and accessibility. Visual tests include these cases and multiple display scales; a rendered screenshot alone does not prove input works.

## 9. Repository and directory design

The product must build from a clean public clone without private monorepos, internal absolute paths, local agent instructions or another person's configuration.

Use five non-hidden root directories:

```text
project/
  doc/       product specification, design, plans, reports and user documentation
  exp/       bounded experiment records; generated runs remain local
  private/   local instructions, inspection evidence and scratch; never tracked
  src/       implementation, assets, CLIs and reusable build/check helpers
  tests/     unit, integration, acceptance and synthetic fixtures
```

README, LICENSE, CONTRIBUTING, the build manifest and index files may be root files. Do not add a sixth root directory for tools, scripts or assets; place them under src. Git/tool metadata and generated dependency/build directories are not application modules and are collapsed/excluded from the default project navigator.

Within authored source/document navigation, a node has at most five immediate directory groups. Split larger domains semantically and create folders only when they contain real material. Date/run archives use an index with at most five visible groups/items per page; their storage convention must be recorded rather than disguising modules as hidden directories. The bootstrap plan must resolve any inherited document-layout conflict explicitly and preserve existing links. Do not reorganize other projects to satisfy this project's rule.

## 10. Public-source and release hygiene

Publish under the owner's personal GitHub account. Company branding, infrastructure, private dashboards, raw transcripts, workstation paths and private workflow instructions are not part of the public product.

Local `CLAUDE.md`, `AGENTS.md`, their local variants, agent settings/prompts and orchestration bindings stay outside tracked source from the **first commit**. Use narrow ignore rules and a staged/history/release-artifact guard. `.gitignore` does not remove already tracked content or history. Public contributor documentation is separately authored and self-contained.

Before a first public push or release, inspect the exact tracked tree and reachable history for private instructions, secret canaries, auth state, personal paths and real identities; verify pseudonymous author/committer identity. Confirm public visibility, the intended owner/remote and the selected open-source license. No tokens appear in remotes. Do not copy third-party code without a compatible license and provenance. Provider CLIs are independently installed unless redistribution is explicitly allowed.

The owner selected the MIT license. Include the complete MIT license with the approved pseudonymous copyright holder, retain applicable third-party notices and check dependency compatibility. The name search is a limited discovery check, not a trademark or global uniqueness clearance.

## 11. Delivery and acceptance

Development starts with a reviewed milestone and phase plan. Separate proposed budgets, mock demonstrations, real installed-provider tests and the owner's final use of the app. Use meaningful TDD for behaviour changes, independent review and small reviewable PRs. Keep local visual/input inspection evidence out of public history unless sanitized for intentional publication.

Proposed sequence for the owning project to refine after research:

1. Public-safe bootstrap, capability/license survey and a measured Linux renderer/host architecture experiment.
2. Host/process lifetime and output persistence; GUI can exit and return while synthetic work continues.
3. One real supported agent path, input/approval/reconnect and Unicode/IME behaviour.
4. OS-keyring authentication and verified same-provider multi-account isolation; second provider adapter.
5. Resource acceptance, user-facing workflow/navigation, failure recovery and reproducible Linux packages.
6. Windows backend/credential/windowing implementation with its own measured acceptance.

Do not select a heavy framework and defer resource validation to the final phase. Do not call a session ready from a tab title, login from an HTTP200, or a feature complete from a passing mock. The first approval package must expose unresolved product choices, measured architectural tradeoffs and a concrete first phase.

## 12. Official references checked for this draft

- [Codex app-server](https://learn.chatgpt.com/docs/app-server): supported protocol and authentication paths; implementation must check the installed version.
- [Codex authentication](https://learn.chatgpt.com/docs/auth): credential storage modes, including explicit keyring storage versus automatic fallback.
- [Claude account authentication and third-party tools](https://support.claude.com/en/articles/13189465-log-in-to-your-claude-account): distinguish installed native CLI login from an independent third-party API integration.
- [OAuth for native apps, RFC8252](https://www.rfc-editor.org/info/rfc8252/): external browser, PKCE and native redirect handling.
- [Secret Service specification](https://specifications.freedesktop.org/secret-service/latest/): Linux credential-store interface.

Check provider documentation and applicable integration permissions again when implementing the adapter. These references establish constraints; they do not prove account access, runtime compatibility or final product acceptance.
