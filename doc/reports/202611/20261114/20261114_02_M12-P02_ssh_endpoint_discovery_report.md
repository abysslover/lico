# M12-P02 Report — The Same Discovery on an ssh Host

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M12-P02-ssh-endpoint-discovery` (stacked on the P01 tip; implementation commit `3810693`)
- Plan: [M12-P02](../../plans/202611/20261114/20261114_02_M12-P02_ssh_endpoint_discovery_plan.md)

## What changed

- **Remote script** (`llm/remote_script.rs`): the P01 probe as one self-contained Python 3 script (Python 3.6 or later, standard library only): the same listing from `<root>/net/tcp` and `tcp6`, the same same-user scope through the inodes under `<root>/<pid>/fd`, the same silent first contact, the same two GETs as HTTP/1.0 with no body, credential, cookie or redirect, the same 64 KiB bound, classes, auth states and model-id cleaning, 64 sockets at most, 16 probes in parallel, 12 seconds of its own budget inside the 20 seconds of the ssh call. Not Linux: `supported: false, os: <name>`; no table: `proc_unreadable`.
- **Runner** (`llm/remote.rs`): the command checks `command -v python3` first, then runs the script through `ssh_remote::run_ssh_bounded` (batch mode, no question, the process group killed at the deadline, output capped at 128 KiB). Words, never a hang: `ssh_unreachable` (unknown host, `offline-box`, `hang-box`), `remote_python_missing`, `remote_timeout` (the runner reports a killed call like an unreachable host; the time it took tells them apart), `remote_output_too_large`, `remote_failed` (an answer that is not the script's), and an answer with `supported: false` for a host that is not Linux or whose table cannot be read. `LICO_LLM_SSH_SECS` shortens the 20 seconds (tests only). In a test `--llm-proc-root DIR` gives the script `DIR/<alias>`; a real run gives `/proc`.
- **Surfaces**: `lico llm discover --ssh-host HOST` and the socket and HTTP requests (`ssh_host`); the dialog gets a host chooser (Left and Right over this machine and the hosts of the ssh configuration, an old answer is dropped when the host changes, nothing is called until Enter, the failure words are shown in plain sentences); the tool gets `host` (the card says `ssh host 'gpu-box'`, the message says `on ssh host '...'` and that `loopback` there means a later use would need a tunnel, a failure is told in words). Every endpoint carries `bind` and `address`.
- Protocol: new error words in `lico_protocol/v1.py`; manual updated.

## Red first (run before the remote path existed)

`tests/test_llm_discover_ssh.py` was run against the P01 host (`private/runs/m12_p02_red1.log`): `4 failed`, all `option_unavailable` for a request with `ssh_host`; then the runner and the script were written and the four went green. The Rust unit tests of `remote.rs`, and the tool and window additions, were written with or after their code and not run red separately (as in P01).

## Evidence (as measured)

| Check | Result |
|---|---|
| Rust unit tests | `llm::remote` 3 (the command with its quoting, the output markers and the failure words, the script run by `python3` with `sys.platform` set to `freebsd14`: `supported: false, os_unknown`); all llm and gui units pass |
| `tests/test_llm_discover_ssh.py` (a fake ssh that runs the remote command on this machine under `/bin/sh` with a private `HOME`, a process table the test wrote) | 4 passed, repeated 3 times, `6.8` to `6.9 s`: **the remote script gives exactly the same endpoints as the local engine** for the same fake servers (normalised equal), with `bind` loopback and all, one ssh call, a speak-first server receives nothing; `offline-box`, `hang-box` and an unknown host: `ssh_unreachable` inside 8 s; a host with a shell and no `python3`: `remote_python_missing`; a 300 KB flood: `remote_output_too_large`; a canned FreeBSD answer and an empty table: `supported: false`; a host that outlasts the budget (`LICO_LLM_SSH_SECS=3`): `remote_timeout` in under 7 s; 70 listeners: 64 probed, 6 skipped, under 12 s |
| `tests/test_llm_tool.py` ssh case, `tests/test_llm_window.py` ssh case (private Xvfb) | passed, repeated 3 times: the card names the host and no ssh call precedes the yes; the message carries the host and the tunnel note; a refused host is told in words; the dialog chooses `gpu-box` with Right, nothing is called until Enter, the server is listed, `offline-box` fails with `could not be reached over ssh` |
| Tool list byte budget (`<= 5,120`, paid in tokens by every session) | the first version of the `host` argument and of the description made the list with `clipboard_write` `5,212` bytes, over the budget; the description and the argument text were shortened until `test_agent_tools` (asserts `<= 5120`) and the Rust list test passed |
| Cold full suite | `532` tests in `109` files, `532 passed`, `122.6 s`, tree peak `1,880.0 MiB` |

## Honest notes

- The changed-suite run before that had two failures that were not mine and that passed alone and in the cold run after it: `tests/test_first_view.py` (`15.6 s`, over its `15 s` budget; alone it takes about `15.5 s`, so it sits at its limit) and `tests/test_picture_paste.py`. They are two of the four tests that failed once each in M10 (`test_call_rooms_and_mentions`, `test_tool_history`, `test_first_view`, `test_picture_paste`); M11 had none in six full runs. The host load average was `6.3` when I looked. P03 reports every cold run.
- The fake ssh is this machine's shell, so the remote script ran under this machine's Python and `/bin/sh`; a real host with a very old Python 3 or an unusual shell was not exercised. `command -v` and `sh -c` are POSIX; the script avoids newer syntax (no assignment expressions, no annotations).
- A remote server that listens on one interface only (not loopback, not all addresses) is probed at that address; the fixture covers loopback and all.
- Not exercised: a real host (by rule).

## Next

P03: end-to-end verification on fake servers and the fake ssh rig, screenshots, three cold runs, idle RSS, hidden growth, `suite acceptance`, `lico 0.3.0`, install with the `0.2.7` backup, the proposed spec requirements.
