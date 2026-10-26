# 00001 config_file_after_connect

Version 1.1.0

**Question.** The config file of a Claude session (`--mcp-config FILE`) holds the secret of that session's tool connection. A program that runs as the same
person and can read files can read the file of ANOTHER session and call the tools as it. If the real `claude` does not need the file after its first
successful `initialize`, the host can delete the file at that moment and the secret lives in two memories only (the host's and the program's). Does the
program read the file again when it connects again?

**Workload.** `config_file_probe.py`, opt-in, not part of the suite: about 2.5 minutes, no model call and no sign-in. An empty throwaway home, a dead
proxy with the loopback exempt (`NO_PROXY` and `no_proxy`, as the host starts the program), the host's flags (`-p`, stream-json in and out,
`--setting-sources user`, `--permission-prompt-tool stdio`, `--mcp-config`, `--allowedTools 'mcp__lico__*'`), control requests only. The tool server is the
script's own minimal JSON-RPC-over-HTTP server on a loopback port; it records every request with its bearer token. Each row starts the program with the
token `tok-old`, waits until `mcp_status` says the server is `connected`, changes the file, asks the program to connect again, and reads from the server
which token the new `initialize` carried.

- Control: file unchanged, `mcp_reconnect` (proves that a reconnect sends a new `initialize`).
- File deleted, or rewritten with another token (`tok-new`), then `mcp_reconnect`; then `mcp_toggle` off and on.
- File removed by the server itself at its first `initialize` (what the host would do), before the program lists the tools, then `mcp_reconnect`; then toggle.
- File deleted, then the control request `initialize` again, or `reload_plugins` (rows for "does any other way of asking read it").

**Observed result (2026-10-27, `claude` 2.1.310).** 9 of 9 rows ran. Every reconnect (`mcp_reconnect` or toggle) sent exactly one new `initialize`, and it
carried the OLD token whether the file was unchanged, deleted, rewritten, or removed by the server at the first `initialize`; the status after was
`connected` every time. The server removed at the first `initialize` was listed afterwards (`tools/list` 2 requests: one before, one after the
reconnect), so the program had read the whole file before its first request. The control request `initialize` and `reload_plugins` made no new connection.
So the program keeps the url and the headers it read at its start and does not read the file again.

**Where else the token lives (added 1.1.0, 2026-10-27, `claude` 2.1.310, the same 9 rows).** Before each throwaway folder is removed, every file under the
program's home, work and temp folders, and its stdout file, is searched for the two token strings (both fakes). The temp folder is moved into the throwaway
root with `TMPDIR` (the host sets no `TMPDIR`, so the real program uses `/tmp`; the move only lets the search see it). The program made 5 files in its home
folder and none in the work and temp folders; **none of them holds a token string, in any row, while the program ran and after it stopped.** The one place the
token string is found is the program's stdout: in the answer to the control request `mcp_status` the probe sends, at `mcpServers/0/config/headers/Authorization`
(9 of 9 rows; no other message holds it). The host never sends `mcp_status`, and it records only the fields it reads from a message (`claude.rs`), so the
token is not written down by it; a message the program prints before its first turn, such as `system/init`, is not in these rows (no model turn is run).
So the token string is in no file of the program's home, work or temp folder after the first `initialize`, and in the program's output only when a client asks
for the server's configuration.

**Scope.** What the rows can see: the control requests `mcp_reconnect`, `mcp_toggle`, `initialize`, `reload_plugins`. **Not tried:** the temp folder at its real place (`/tmp`) and the messages printed in a model turn; a connection that
drops in the middle of a model turn and is reopened by the program on its own (needs a model call); interactive mode (`/mcp`); `mcp_set_servers`;
another version of the program (re-run when it updates); a resumed session (the host starts a new process with a new key and a new file for it).

**Run.** `python3 -I config_file_probe.py` (Python 3.11 or newer; `claude` on PATH). Rows print as `ROW name: ...` followed by two lines on where the token string is found; the exit code is 0 when every row ran. Every row waits a fixed 3 s after
its trigger: "made no new connection" means none within 3 s.
