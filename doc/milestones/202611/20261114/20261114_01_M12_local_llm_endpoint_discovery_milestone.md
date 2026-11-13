# M12 — Find OpenAI-Compatible Local LLM Endpoints on This Machine and on ssh Hosts (read-only, on demand) (`lico 0.3.0`)

- Status: approved, in progress
- Version: 1.0.0
- Approved: 2026-11-14 (G1 for M12 and G2 for M12-P01 to M12-P03 approved by the owner on 2026-11-14)
- Project: Lico
- Updated: 2026-11-14
- Base: `main` at `ece5706` (M10 and M11 merged and published); later phases are stacked on the previous phase tip
- Milestone branch: `milestone/M12-local-llm-endpoint-discovery` (created by the lead at the closing commit)
- Implementation: `phase/M12-P01-local-endpoint-discovery` .. `phase/M12-P03-verification-and-0-3-0` (`lico 0.3.0`)
- Next gate: G3 and G4 stay with the owner

## Goal

Local LLM servers (llama.cpp, vLLM, Ollama, LM Studio, litellm and others) often already listen on a port of this machine or of an ssh-connected machine, but connecting a tool to one takes manual work. Step one is to find them: list the listening TCP ports of a machine that answer like an OpenAI-compatible (or Ollama) server, with their model names. Discovery is read-only, on demand (a dialog button, `lico llm discover`, a tool call that the person approves) and never runs in the background. Local machine and ssh hosts work the same way. Default scope is the ports opened by processes of the same user; `--all-users` is an explicit option.

## Rules kept

GET only, short timeouts, a response read to at most 64 KiB, no request body, no redirect followed, no credential sent; a 401 or 403 with an OpenAI-style error body is reported as "key needed" and no key is tried. Only sockets that listen on the machine itself. No other application's configuration or credential file is read (AUTH-08). Bounded total time and bounded count of ports probed; nothing is stored. Tests never probe a real service: the `/proc` root of the machine is injectable (`--llm-proc-root`), fake HTTP servers listen on ephemeral loopback ports, ssh uses the fake-ssh fixtures. Idle RSS `< 50 MiB`, one executable, GUI for the human with CLI and tool parity.

## Done when

1. `lico llm discover` (socket, CLI, `POST /v1/llm/discover`) lists the same-user listening sockets of this machine with class (`openai`, `ollama`, `http-other`, `not-http`, `unknown`), bind scope, auth state, model ids, owning process name and probe time; `--all-users` lists every listening socket; a server that speaks first receives nothing; redirects are not followed; every probe is a bounded GET.
2. The same discovery runs on an ssh host through a bounded remote script, with clean messages for an unreachable host, a missing `python3`, a timeout, an oversize output and a non-Linux host, and says per endpoint whether it listens on loopback only.
3. A centred dialog (Session menu) scans on `Enter` or a click, never by itself; the tool `llm_endpoints` asks the person per call.
4. Cold full suite `<= 180.0 s` and `<= 2,048.0 MiB`, idle RSS `< 50.0 MiB`, hidden growth `<= +10.0 MiB`, two-clone `suite acceptance`, `lico 0.3.0` installed to `~/bin/lico` (the `0.2.7` binary kept as a backup). Numbers only as measured. The proposed spec requirements (LLM-01 to LLM-03) are written into the P03 report; the spec is not edited without the owner's yes.

## Out of scope

- Using a found endpoint: no custom base URL in the providers, no ssh tunnel, no proxy, no stored endpoint or account, no keys. The result carries the fields a later step would need (host, port, bind and address, candidate base URL, class and apis, auth state, model ids, process).
- Display polish of the chat list and a Native entry for ssh hosts (a separate item).
- Push, pull request, merge or publication (owner G4).

## Phases

| ID | Phase | Plan | Report | Status |
|---|---|---|---|---|
| M12-P01 | Discovery engine for this machine: wire op, CLI, HTTP, tool, dialog | [Plan](../../plans/202611/20261114/20261114_01_M12-P01_local_endpoint_discovery_plan.md) | [Report](../../reports/202611/20261114/20261114_01_M12-P01_local_endpoint_discovery_report.md) | implemented |
| M12-P02 | The same discovery on an ssh host through a bounded remote script | [Plan](../../plans/202611/20261114/20261114_02_M12-P02_ssh_endpoint_discovery_plan.md) | [Report](../../reports/202611/20261114/20261114_02_M12-P02_ssh_endpoint_discovery_report.md) | implemented |
| M12-P03 | End-to-end verification, measurements, and `lico 0.3.0` | [Plan](../../plans/202611/20261114/20261114_03_M12-P03_verification_and_0_3_0_plan.md) | pending | planned |
