"""M24-P08: a Chats card shows what the person said and not what a program wrote for them. A slash command is stored by Claude Code as markup (`<command-name>/model</command-name>`),
its output as another record, and Orca types its wake-up sentence ("You have 1 orchestration message. ...") into an agent's terminal, so the agent's own record keeps it as a prompt.
None of those is a message: not the card's title, not its count of turns. The test host has a private HOME, so these tests write synthetic records and never read the owner's."""

from __future__ import annotations

import json
import os
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.test_agy_chats import A, write_history, prompt
from tests.test_codex_adapter import usual_umask

WAKE_UP = "You have 1 orchestration message. Run `orca orchestration check --terminal term_1`."
COMMAND = "<command-name>/model</command-name>\n            <command-message>model</command-message>\n            <command-args></command-args>"
COMMAND_WITH_WORDS = "<command-message>orchestration</command-message>\n<command-name>/orchestration</command-name>\n<command-args>fix the bug</command-args>"
OUTPUT = "<local-command-stdout>Kept model as `Sonnet`</local-command-stdout>"


def claude_chat(home: Path, sid: str, cwd: str, records: list[tuple[str, str]], mtime: int) -> Path:
    """A Claude record of `records` (role, text), the first one with the folder."""
    folder = home / "projects" / "proj-alpha"
    folder.mkdir(mode=0o700, parents=True, exist_ok=True)
    lines = []
    for n, (role, text) in enumerate(records):
        body = {"role": role, "content": text if role == "user" else [{"type": "text", "text": text}]}
        lines.append(json.dumps({"type": role, "message": body, **({"cwd": cwd} if n == 0 else {})}))
    path = folder / f"{sid}.jsonl"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    os.utime(path, (mtime, mtime))
    return path


def test_a_card_and_its_count_show_what_the_person_said_and_not_a_command_markup_or_orcas_wake_up():
    from lico_measure.p02 import start
    from lico_protocol import v1
    from tests.account_helpers import account
    from tests.test_new_session_screen import claude_args

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700, parents=True)
        c_args, _rec = claude_args(root)
        host, _log_dir = start(directory, extra=["--text", "--state-dir", str(root / "state"), *c_args], sessions=0, producing=0)
        try:
            conn = Conn(host.socket)
            acc = account(conn, "LocalClaude", "claude")
            home = root / "state" / "accounts" / acc / "claude"
            cwd = str(proj)
            claude_chat(home, "cl-cmd", cwd, [("user", COMMAND), ("user", OUTPUT), ("user", "plan the release"), ("assistant", "ok")], 1_760_000_010)
            claude_chat(home, "cl-bare", cwd, [("user", COMMAND), ("user", OUTPUT), ("user", WAKE_UP)], 1_760_000_020)
            claude_chat(home, "cl-words", cwd, [("user", COMMAND_WITH_WORDS), ("user", "second thing"), ("assistant", "ok")], 1_760_000_030)
            claude_chat(home, "cl-wake", cwd, [("user", WAKE_UP), ("user", "the real question"), ("assistant", "ok")], 1_760_000_040)
            claude_chat(home, "cl-plain", cwd, [("user", "You have 3 apples"), ("assistant", "ok")], 1_760_000_050)
            write_history(root, [
                prompt(WAKE_UP, 1_760_000_060_000, "/work/one", A),
                prompt("the real agy question", 1_760_000_061_000, "/work/one", A),
            ])
            reply = conn.call(op="session.chats", all=True, agy=True)
            assert reply["ok"] == 1 and v1.validate_response("session.chats", reply) == [], reply
            by = {it["provider_session"]: it for it in reply["items"]}
            seen = {sid: (it["title"], it["turns"]) for sid, it in by.items()}
            assert seen["cl-cmd"] == ("plan the release", 2), "the command and its output are no title and no turn"
            assert seen["cl-bare"] == ("claude (cl-bare)", 0), "nothing the person said: the provider and the id, no turn"
            assert seen["cl-words"] == ("second thing", 3), "a command with words is a turn, never a title"
            assert seen["cl-wake"] == ("the real question", 2), "the wake-up sentence is no title and no turn"
            assert seen["cl-plain"] == ("You have 3 apples", 2), "a sentence that only begins alike is kept"
            assert by[A]["title"] == "the real agy question", "an agy conversation of the wake-up sentence and a question"
            shown = json.dumps(reply)
            assert "<command-name>" not in shown and "orchestration message" not in shown, "no markup and no wake-up sentence anywhere in the answer"
        finally:
            host.stop()
