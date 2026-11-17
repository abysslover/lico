# lico-memguard 0.1.0

Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.

Pure decisions of the memory guard (M19-P01). No clock (milliseconds are passed in), no `/proc`, no display, no dependency.

1. **Levels.** `Limits` (default 400 and 500 MB; 1 MB = 1024 kB): a session is `Orange` from the warning level and `Red` from the limit, by the RSS of the largest single process of its tree; `valid()` means both above zero and the warning below the limit. `machine_level(available, total)`: orange under 20 percent available, red under 10 percent (exactly the border is not under it); no total is no judgement.
2. **Episodes.** `Episodes::ask` (the question on a click) and `warn_main` (the warning to the main agent) are true once for each red episode of a session; leaving red ends it; `forget` starts a session anew.
3. **Who closed it.** `ClosedBy::{Person, Guard}`. `ResumeQueue` takes only guard-closed sessions, each once, and hands them out one at a time, `gap_ms` after the last resume; `cancel` drops one the person took up.
4. **Loop guard.** `LoopGuard::allowed` is false for a session the guard closed twice within ten minutes.
5. **Idle.** `Idle::ok` only with no turn running, no approval pending and the person not typing.
6. **Texts.** `warning_text` and `machine_warning_text` are fixed templates: numbers, the session id, a name cleaned of control characters and cut at 40 characters, and the exact commands `lico session close ID` and `lico session resume ID`.
7. No thread, file, socket, environment variable or `unsafe`. Budget: the default 256 MiB and 30 s.
