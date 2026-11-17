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

### Local LLM sessions

| ID | Requirement |
|---|---|
| LLM-01 | Discovery of Local LLM endpoints is on demand and read-only: it looks only at sockets that listen on the machine itself (this machine, or an ssh host the person named), with GET requests that have short timeouts, a bounded response, no body, no redirect and no credential, and never in the background; nothing it finds is persisted. |
| LLM-02 | Discovery reads no other application's configuration or credential file (AUTH-08); it uses the list of listening ports and the HTTP answers only. A server that wants a key is reported as such and no key is tried. |
| LLM-03 | A discovered endpoint is not a provider or an account until an explicit later flow makes it one; a key for it lives only in the OS keyring (AUTH-01, AUTH-02). |
| LLM-04 | A Local LLM server becomes a session of its own kind (`local`), chosen by the person next to Claude and Codex, on this machine only. Claude and Codex keep their own sign-in and are never routed to a local model. |
| LLM-05 | A local session connects only to a loopback address, with no redirect, no proxy and no TLS. A server key is read from the OS keyring for each request, sent only as an `Authorization` header, and is never written to a log, a record, a file or a command line. |
| LLM-06 | A local session keeps its conversation in an owner-only record (directory mode 0700, file 0600) that holds only what the person typed, what the model answered and the results of its tool calls (each at most 8 KiB): no key, no raw server response, no reasoning text. A torn last line is repaired on opening; any other damage refuses the record. When the conversation no longer fits the model's window, whole oldest turns are left out of what is sent (never from the record) and the person is told. |
| LLM-07 | The host runs the tool loop: only the tools the host offers can be called, with the approvals every session has (the tools that need none run at once, every other tool leaves its card and ends the turn until the person decides); at most 8 rounds, 4 calls an answer, 16 calls and 10 minutes for one message of the person; a reply that is cut or fails runs no call. |
| LLM-08 | A local session has no access to files or a shell; a tool for either needs its own requirement and the person's approvals first. |

### Memory guard

Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.

| ID | Requirement |
|---|---|
| MEM-01 | Each running session's memory is measured as VmRSS from `/proc` over the whole process tree of its agent (its subagents and tools included); the figure shown in the session list is the tree total, and the colour follows the largest single process of the tree: orange from 400 MB, red from 500 MB (the defaults; the owner may set both in an owner-only `memory.json`). RSS counts shared library and file pages in every process that maps them. The memory is read only while a window or a client looks at the status: nothing wakes when nobody looks. |
| MEM-02 | A click on a red session (not orange) asks once for each red episode: "this session uses N MB (limit 500). End it and continue? The conversation resumes; tool jobs that were running are lost.", with end-and-resume or cancel; cancel opens the session as usual. End-and-resume is `session.close` then `session.resume` in one step; a session with no conversation to come back to is refused before anything is ended. |
| MEM-03 | Who closed a session is recorded (`closed_by`: `person` or `guard`) until it is resumed. Only a session the guard closed is ever resumed by the guard; nothing revives a session the person closed. Resumes by the guard go one at a time, 20 seconds apart. The record and the resume queue are kept while the host runs: if the host restarts between the guard's close and its resume, the session stays closed (it is then like any closed session, and the person resumes it); this is a known gap. If the guard's resume fails, the session stays closed, still marked as closed by the guard, and no note is written. |
| MEM-04 | The auto-close is OFF unless the owner's `memory.json` says `"auto_close": true`; a session may be left out (`auto_close_off`). A red session is closed and resumed automatically only at an idle moment (no turn running, no approval pending, no input from the person for 30 seconds); a session that is red again twice in ten minutes is only warned about. When a main agent is marked and runs, the close waits until the warning has been tried (at most five minutes: the warning goes only at an idle moment of the main agent) and then two minutes more for the main agent to deal with the session itself (no main agent, or one that does not run, or the warning switched off: no wait); a warning that could not be delivered is tried again after 30 seconds, and the two minutes run from the first try. When the machine is red (available memory under 10 percent) the same switch closes and resumes the heaviest IDLE session that is not the main agent, by the RSS of its whole tree, one at a time and at most one every two minutes, after the machine warning and its two minutes. After a successful resume the session shows "[memory N MB: closed and resumed automatically]", where N is the RSS of the session's whole tree when it was closed (the colour and the red decision follow the largest single process, so N may be higher than the limit). |
| MEM-05 | The main agent (the session the owner names as `main_agent` in `memory.json`) is warned by a fixed message that carries numbers, the session id, a cleaned and capped session name and the exact commands `lico session close ID` and `lico session resume ID`, once for each red episode of a session and once for each red episode of the machine (available memory under 10 percent), only at an idle moment of the main agent; ON unless `"warn_main": false`. Nothing an agent wrote is put into the message but the capped, cleaned name. |
| MEM-06 | The window's line under the session list shows the machine's available share first ("free 8% ...") and takes the worst colour when the machine runs low (orange under 20 percent available, red under 10); a session started while it is low is followed by a plain warning in the window (a warning, never a block). The figures come from `machine_memory` of the `status` reply. |
| MEM-07 | A WORKING session that shows no activity for 10 minutes is called stalled (the owner may set `stall_min` in minutes, or `stall_s` in seconds which wins, in `memory.json`; 0 switches it off): the line under the list turns orange and the hint under the mouse says "last activity N min ago". An idle session waits for the person and is never stalled. The host sends the time as `stall_s` in `memory_limits`. *From the owner's evidence file: a worker stood still for 3.5 hours and nobody knew.* |

### Browser tab

*Recorded in M17-P09 as approved by the LEAD under the owner's delegation (the owner away); to be confirmed by the owner.*

| ID | Requirement |
|---|---|
| BROW-01 | A browser tab shows a page with the Moli engine, a pinned release held by the component `lico-engine` (upstream release, Lico's own version of the record, the engine contract number and the list of Lico's patches, archive and file SHA-256 compiled in; M20: the official release with no patch). When a tab is opened by the person (the window's `+` menu or `lico browser open`, never by a page, a link or a program in a session) and the engine is not installed, Lico installs it by itself, with no question: the tab says "installing the browser engine" with the size, the source and the start of the pinned SHA-256, then starts the engine and loads the page; nothing is fetched at host start or without a tab, and only the pinned archive from the pinned address is fetched (no variable, file or argument changes the address or the hash; a new engine is a new Lico release). The download is made by the system `curl` into a fresh private staging folder, the archive is checked against its pinned SHA-256 through the open file, listed by the system `tar` (everything inside the archive's own folder, only folders and regular files, the one engine file exactly once, no link, device or path out), the one engine file taken out and checked against its pinned SHA-256, and moved into place with one atomic rename; anything else leaves nothing on disk, and a hash that is not the pinned one is shown as a security notice. A failure (no network, no `curl`, a size over the cap, no room) is said plainly in the tab with the manual steps, one attempt for each opening, never a retry loop; two tabs opened together install once. The engine is found in a folder below the home directory in which every directory is the person's own and not writable by others and the file is the person's own, not a link and not writable by others; its SHA-256 is checked from the open file at every start and that very file is run. No environment variable and no flag skips the check; the only other engine is one named in an owner-only setting file, and every tab of it says so. The same file may say `{"auto_install": false}`, which only turns the self-install off. *Requested by the owner in the chat, 2026-11-17 (it replaces "Lico never downloads the engine" of M17); the spec wording approved by the lead, to be confirmed by the owner.* |
| BROW-02 | The engine runs only while a tab is open (no process, thread or socket otherwise), in a Landlock fence: it may read and run its own folder and the system libraries, write one scratch folder of the host's (mode 0700, removed at the end) and `/dev/null`, connect to its fetch guard's port only and bind its own CDP port; its environment is an empty home that is also its temporary folder, a font configuration file the host writes, and the guard's address. The engine's own socket has no password (the engine has none: it checks the origin of a web page, which stops a page and not a program on this machine), and the tab and the list say so. A kernel that cannot give this means no engine. |
| BROW-03 | Every fetch of a page goes through a fetch guard of Lico's own on a loopback port, started with the engine, that resolves the name itself, connects to the address it checked, allows public addresses on the ports 80, 443, 8080 and 8443 only, refuses private, loopback, link-local and every other address range (a table, with IPv4-mapped forms and a name with any non-public answer), and never connects to a port of this machine that no page may reach (the host's own door, an engine's). `--proxy` is passed to nothing else. |
| BROW-04 | A tab opened by the person for this machine's own services (`--local`, off by default, never from a page or a link) uses its own engine process (the same engine, started the same way); its guard allows loopback addresses ONLY; the tab says "this tab can reach services on this machine and no other site". |
| BROW-05 | No engine of the tab has a stealth switch: Moli has none, and Lico offers none. A `{"stealth": true}` left in the owner-only `browser.json` by release 0.6.2 is read and ignored (an old file is not an error). The user-agent names Lico and the engine and claims no browser version (`Emulation.setUserAgentOverride` works on Moli: MEASURED), and the page may still see a Windows platform, which the engine decides; the help says so. *History: the first wording ("Lico passes no stealth switch") was not the owner's; the owner corrected it on 2026-11-17: "Lico may use stealth, and may not", and release 0.6.2 gave the owner that option for Obscura. The same day the owner decided that Moli is the default engine and Obscura is scrapped; the stealth option went with the engine that had it. It comes back on the day an engine of the tab has a switch. The removal is the lead's reading of that decision, to be confirmed by the owner.*  |
| BROW-06 | Nothing is stored: no cookie or storage is kept by Lico (the engine's own cookie store is a file in its scratch folder and goes with it when the engine ends), no download, file chooser or dialog is accepted. A refused or failed load shows a notice in the tab's header (its true reason); the engine may paint its own error page in the frame as well. |
| BROW-07 | Only the person's input reaches a page (mouse, wheel, keys, committed text, an explicit paste), never input of a program or of another page; Lico's own keys never reach it. Nothing from a page (its text, its title, its address) reaches a Lico operation, a tool, a session or the clipboard. |
| BROW-08 | The frame of a page is a JPEG (the host asks the engine for quality 90, which keeps all the colour: the same 1920 x 1200 page is 132 KB, where Moli's PNG of it is 3.77 MB; M20-P02) or a PNG, of at most 4 MiB, decoded in the window only for the tab that is shown; the host keeps only the newest and counts the rest; nothing in the browser polls or wakes while a still page is shown. |
| BROW-09 | The window makes a page that has the keys look alive, with its own drawing and nothing sent to the engine: the header of a ready or loading tab says where typing goes (the page, or the address row); a ring marks the last click on the page while the page has the keys; the pointer is a crosshair over the page; the address row follows the page's address when the person has not edited it (a row the person edited or cleared is never overwritten); while the page has the keys the composition of the input method is shown in the header, not in the address row. The help says what is known of the engine's limits: a page may still see a Windows platform, and a site may refuse an automated browser and show its own block page (Lico does not hide what it is). *Requested by the owner's reports in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.* |

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
