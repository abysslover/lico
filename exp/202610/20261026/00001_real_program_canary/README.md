# 00001 real_program_canary

Version 1.0.1

**Question.** When a person picks a folder for a Lico session, can that folder make the installed `claude` or `codex` program
load or run its own configuration (settings hook, MCP server, notify hook, skills, agents, commands, model provider)? Lico
starts Claude with `--setting-sources user`, and starts every Codex thread read-only with the mode carried by each turn.
The mocks in the test suite cannot say whether the real programs honour that; this can.

**Workload.** `real_program_canary.py`, opt-in, not part of the suite: about 5 minutes (295 s measured, load average about 4), no model call and no sign-in.
Each start uses an empty throwaway home and a dead proxy or dead model port. The folder holds canary scripts that append a
line to a file when they run. Rows, one `PASS/FAIL` line each:

- Claude, `--setting-sources user`, auto and default permission mode: the canary must not run and no folder skill, agent or
  command may be named in the program's output.
- Claude positive control, `--setting-sources user,project,local`: the settings hook and the MCP canary MUST run, and the
  folder's skill, agent and command are named; otherwise the method is blind and the row fails.
- Codex, plain folder and folder with `.git`: `old-auto` (thread writable, the behaviour before F1) is the positive control
  and MUST load the folder's `.codex/config.toml`; `new-auto`, `new-default`, `resume-auto`, `resume-default` must leave the
  project layer disabled, run no canary and write no trust entry.
- Codex through the Lico host (`--lico PATH`): auto and default sessions, create, input, close, resume, input; no trust entry
  and canary 0.

**Tested versions.** `claude` 2.1.310, `codex-cli` 0.164.0 (the script prints both on its first lines). The answer is only as
good as these versions: both programs change their loading rules between releases. Re-run when either one updates.

**Scope.** What the controls can see: a Claude settings hook and MCP server, the names of the folder's skill, agent and
command; for Codex the project config layer, an MCP server, the notify hook and the trust entry. Not checked: the Claude
status line and a skill's own hook (neither fired even with project settings on, in print mode); whether either program reads
the folder's `CLAUDE.md` or `AGENTS.md` as text for the model (needs a model call); a sign-in, a real turn, a folder the person
trusted earlier in their own codex (known, listed in the report); any other version or platform.

**Observed result (2026-10-26).** 15 of 15 rows passed. Claude `user`: canary 0 in both modes, nothing from the folder named;
Claude control: settings hook and MCP canary ran, skill, agent and command named. Codex: the control loads the folder's config
and writes 2 trust entries in both folder kinds; the four Lico-shaped scenarios load nothing and write none; through the Lico
host canary 0 and trust 0. Mutation 1: with `thread/start` and `thread/resume` made writable for auto sessions, the two Lico-host
rows fail (canary ran 4 times, trust entries 2) and the other 10 rows still pass. Mutation 2: with a name the folder does not have added
to the list of names the Claude control must see, the control row fails with "the method is blind" and the two `user` rows still pass.

**Run.** `python3 -I real_program_canary.py all --lico PATH_TO_LICO_BINARY` (Python 3.10 or newer; `claude` and `codex` on
`PATH`). Exit 0 all rows held, 1 a row did not, 2 a program or the given binary is missing.
