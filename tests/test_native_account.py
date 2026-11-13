"""M11-P01: the provider's own default home as a visible `Native` entry (reserved ids `native-claude` and `native-codex`): closed until the person chooses, listed and
resumed in place once chosen with the provider's own login (a Native session gets the default home and nothing of a managed account), one listing rule for all
accounts, a choice kept in settings, the label beside the target, and the tool `create` that never switches a Native caller to another account. Synthetic homes and
mocked providers only: no real home of the owner is read and no turn is billable."""

from __future__ import annotations

import json
import os
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_accounts import environ_of, status_of
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl, _seed_codex_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import sent as codex_sent, usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args

CODEX_NEW = "019a3333-4444-7000-8000-0000000000c3"


def _host(directory: str, root: Path):
    home = root / "synthetic-home"
    home.mkdir(mode=0o700)
    c_args, c_rec = claude_args(root)
    x_args, x_rec = codex_args(root)
    env = {"HOME": str(home), "CLAUDE_CONFIG_DIR": str(home / ".claude"), "CODEX_HOME": str(home / ".codex")}
    host, log = start(directory, extra=["--text", "--state-dir", str(root / "state"), *c_args, *x_args], sessions=0, producing=0, env_overrides=env)
    return host, log, home, c_rec, x_rec


def _seed_default_homes(home: Path, proj: Path) -> None:
    _seed_claude_jsonl(home / ".claude", "p", "cl-nat-old", str(proj), "native old task", "old answer", 1_760_000_100)
    _seed_claude_jsonl(home / ".claude", "p", "cl-nat-new", str(proj), "native new task", "new answer", 1_760_000_500)
    _seed_codex_jsonl(home / ".codex", CODEX_NEW, str(proj), "codex native task", "codex native answer", 1_760_000_600)


def test_native_is_listed_closed_until_chosen_then_open_and_one_rule_serves_every_account():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, home, _c, _x = _host(directory, root)
        try:
            conn = Conn(host.socket)
            acc_c, _acc_x = account(conn, "ClaudeWork", "claude"), account(conn, "CodexWork", "codex")
            _seed_default_homes(home, proj)

            listed = conn.call(op="account.list")
            assert v1.validate_response("account.list", listed) == [], listed
            nat = {n["provider"]: n for n in listed["native"]}
            assert nat["claude"]["account"] == "native-claude" and nat["claude"]["label"] == "Native (~/.claude)", nat
            assert nat["codex"]["account"] == "native-codex" and nat["codex"]["label"] == "Native (~/.codex)", nat
            assert nat["claude"]["state"] == "ask" and nat["claude"]["chosen"] is None and nat["claude"]["chats"] == 2 and nat["codex"]["chats"] == 1, nat
            assert [a["account"] for a in listed["accounts"]] == [acc_c, _acc_x], "the managed accounts are not touched"

            # not chosen yet: no chat of the default home is listed or read, and a session cannot be made on it
            assert conn.call(op="session.chats", cwd=str(proj))["total"] == 0
            closed = conn.call(op="session.chats", cwd=str(proj), account="native-claude")
            assert closed["ok"] == 0 and closed["error"] == "native_off", closed
            refused = conn.call(op="session.create", kind="claude", account="native-claude", cwd=str(proj), mode="auto")
            assert refused["ok"] == 0 and refused["error"] == "native_off", refused

            # the choice is kept in the settings and is per provider
            assert conn.call(op="settings.set", native_claude="maybe")["ok"] == 0
            on = conn.call(op="settings.set", native_claude="on")
            assert on["ok"] == 1 and on["settings"]["native_claude"] == "on", on
            nat = {n["provider"]: n for n in conn.call(op="account.list")["native"]}
            assert (nat["claude"]["state"], nat["claude"]["chosen"], nat["codex"]["state"]) == ("on", "on", "ask"), nat

            only = conn.call(op="session.chats", cwd=str(proj), account="native-claude")
            assert only["ok"] == 1 and v1.validate_response("session.chats", only) == [], only
            assert [i["provider_session"] for i in only["items"]] == ["cl-nat-new", "cl-nat-old"], only
            assert {i["account"] for i in only["items"]} == {"native-claude"} and {i["account_label"] for i in only["items"]} == {"Native (~/.claude)"}
            assert conn.call(op="session.chats", cwd=str(proj), account=acc_c)["total"] == 0, "a managed account lists only its own home"
            everyone = conn.call(op="session.chats", cwd=str(proj))
            assert [i["provider_session"] for i in everyone["items"]] == ["cl-nat-new", "cl-nat-old"], "no account named: every account that is open, Native included"
            latest = json.loads(cli(host, "chat", "latest", "--cwd", str(proj), "--account", "native-claude").stdout)
            assert [i["provider_session"] for i in latest["items"]] == ["cl-nat-new"], latest
            wrong = conn.call(op="session.create", kind="codex", account="native-claude", cwd=str(proj), mode="auto")
            assert wrong["ok"] == 0 and wrong["error"] == "bad_request", "Native of one provider is not an account of the other"

            # "ask" takes the choice back
            assert conn.call(op="settings.set", native_claude="ask")["settings"].get("native_claude") is None
            assert conn.call(op="session.chats", cwd=str(proj))["total"] == 0
        finally:
            host.stop()


def test_a_native_session_resumes_in_place_with_the_default_home_and_nothing_of_a_managed_account():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, home, c_rec, x_rec = _host(directory, root)
        try:
            conn = Conn(host.socket)
            acc_c, acc_x = account(conn, "ClaudeWork", "claude"), account(conn, "CodexWork", "codex")
            _seed_default_homes(home, proj)
            assert conn.call(op="settings.set", native_claude="on", native_codex="on")["ok"] == 1

            made = conn.call(op="session.create", kind="claude", account="native-claude", cwd=str(proj), mode="auto", resume_latest=True)
            assert made["ok"] == 1 and made["resumed"] is True and made["resumed_chat"]["provider_session"] == "cl-nat-new", made
            wait_for(lambda: any("--resume" in av and "cl-nat-new" in av for av in argv_of(c_rec)))
            env = environ_of(host, conn, made["session"])
            assert env["HOME"] == str(home) and env["CLAUDE_CONFIG_DIR"] == str(home / ".claude"), env
            assert not any(acc_c in v or acc_x in v for v in env.values()), "no path of a managed account reaches a Native session"
            entry = status_of(conn, made["session"])
            assert entry["native"] == "Native (~/.claude)" and entry.get("account") is None, entry

            made_x = conn.call(op="session.create", kind="codex", account="native-codex", cwd=str(proj), mode="auto", resume_latest=True)
            assert made_x["ok"] == 1 and made_x["resumed_chat"]["provider_session"] == CODEX_NEW, made_x
            wait_for(lambda: any(m.get("method") == "thread/resume" and m.get("params", {}).get("threadId") == CODEX_NEW for m in codex_sent(x_rec)))
            env_x = environ_of(host, conn, made_x["session"])
            assert env_x["HOME"] == str(home) and env_x["CODEX_HOME"] == str(home / ".codex"), env_x
            assert status_of(conn, made_x["session"])["native"] == "Native (~/.codex)"

            # the chat is now open in a tab of ours: the list says so, and a second Resume shows that tab again
            again = conn.call(op="session.create", kind="claude", account="native-claude", cwd=str(proj), mode="auto", resume_latest=True)
            assert again["ok"] == 1 and again["already_open"] is True and again["session"] == made["session"], again

            # a session keeps the account it was started with: turning Native off hides the entry for new sessions only
            assert conn.call(op="settings.set", native_claude="off")["ok"] == 1
            assert status_of(conn, made["session"])["native"] == "Native (~/.claude)"
            gone = conn.call(op="session.create", kind="claude", account="native-claude", cwd=str(proj), mode="auto")
            assert gone["ok"] == 0 and gone["error"] == "native_off", gone
        finally:
            host.stop()


def test_the_first_run_without_a_managed_account_still_uses_the_default_home_and_it_is_labelled_native():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, home, c_rec, _x = _host(directory, root)
        try:
            conn = Conn(host.socket)
            _seed_default_homes(home, proj)
            nat = {n["provider"]: n for n in conn.call(op="account.list")["native"]}
            assert (nat["claude"]["state"], nat["claude"]["chosen"]) == ("on", None), "nothing else to choose: the legacy first run, now visible"
            listed = conn.call(op="session.chats", cwd=str(proj))
            assert [i["provider_session"] for i in listed["items"]] == [CODEX_NEW, "cl-nat-new", "cl-nat-old"] and {i["account"] for i in listed["items"]} == {"native-claude", "native-codex"}, listed
            made = conn.call(op="session.create", kind="claude", cwd=str(proj), mode="auto")
            assert made["ok"] == 1 and status_of(conn, made["session"])["native"] == "Native (~/.claude)"
            # the first managed account closes the default home until the person says yes: the old rule, but visible and answerable
            account(conn, "ClaudeWork", "claude")
            assert {n["provider"]: n["state"] for n in conn.call(op="account.list")["native"]}["claude"] == "ask"
            after = conn.call(op="session.chats", cwd=str(proj))["items"]
            assert [i["provider_session"] for i in after if i["provider"] == "claude" and i["open_session"] is None] == [], "Claude: a managed account exists and nothing was chosen"
            assert [i["provider_session"] for i in after if i["provider"] == "codex"] == [CODEX_NEW], "Codex has no managed account yet: its default home is still the only one"
        finally:
            host.stop()


def test_the_choice_is_set_over_the_command_line_and_the_default_home_files_are_only_listed_never_opened():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, home, _c, _x = _host(directory, root)
        try:
            conn = Conn(host.socket)
            account(conn, "ClaudeWork", "claude")
            _seed_default_homes(home, proj)
            secret = home / ".claude" / ".credentials.json"
            secret.write_text('{"canary": "never-read"}\n')
            secret.chmod(0)  # a credential file nobody may open: counting and listing must not need it
            done = json.loads(cli(host, "settings", "--native-claude", "on").stdout)
            assert done["ok"] == 1 and done["settings"]["native_claude"] == "on", done
            assert json.loads(cli(host, "settings", "--native-claude", "ask").stdout)["settings"].get("native_claude") is None
            nat = {n["provider"]: n for n in conn.call(op="account.list")["native"]}
            assert nat["claude"]["chats"] == 2, "the count came from the transcripts, not from the credential file"
            assert conn.call(op="settings.set", native_claude="on")["ok"] == 1
            assert conn.call(op="session.chats", cwd=str(proj), account="native-claude")["total"] == 2
            secret.chmod(0o600)
        finally:
            host.stop()


def test_the_tool_create_keeps_a_native_caller_native_and_an_agent_may_ask_for_native_only_when_it_is_on():
    from tests.test_claude_adapter import status_of as claude_status
    from tests.test_tool_connection import World
    from tests.test_tool_create import call, rows, wants
    from tests.test_tool_send import decide, host_of

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = (root / "work").resolve()
        project.mkdir(mode=0o700)
        host, log, received = host_of(directory)
        try:
            w = World(directory, host, log, received)
            managed = account(w.conn, "ClaudeWork", "claude")
            assert w.conn.call(op="settings.set", native_claude="on")["ok"] == 1
            nat = w.conn.call(op="session.create", kind="claude", account="native-claude", cwd=str(project), name="Nat")
            man = w.conn.call(op="session.create", kind="claude", account=managed, cwd=str(project), name="Man")
            assert nat["ok"] == 1 and man["ok"] == 1
            native_caller, native_gen = nat["session"], nat["generation"]

            # a Native caller's new session of the same kind stays Native: it is not moved to the managed default
            before = {r["session"] for r in rows(w)}
            number = wants(w, native_caller)
            assert decide(w, native_caller, native_gen, number, "allow")["outcome"] == "done"
            [new] = [r for r in rows(w) if r["session"] not in before]
            assert new.get("account") is None and new["native"] == "Native (~/.claude)", new

            # an agent on a managed account may ask for Native by name: the card says so, and one yes makes one Native session
            before = {r["session"] for r in rows(w)}
            number = wants(w, man["session"], account="native")
            card = claude_status(w.conn, man["session"])["approvals"][0]
            assert "account: Native (~/.claude)" in card["prompt"], card
            assert decide(w, man["session"], man["generation"], number, "allow")["outcome"] == "done"
            [asked] = [r for r in rows(w) if r["session"] not in before]
            assert asked.get("account") is None and asked["native"] == "Native (~/.claude)", asked

            # any other account is not an agent's choice (it is ignored), and Native that is off is refused at the call
            before = {r["session"] for r in rows(w)}
            number = wants(w, man["session"], account=managed)
            card = claude_status(w.conn, man["session"])["approvals"][0]
            assert "account:" not in card["prompt"], "an agent cannot name a managed account: the card says nothing of one"
            w.conn.call(op="approval.answer", session=man["session"], generation=man["generation"], approval_id=number, answer="deny")
            assert w.conn.call(op="settings.set", native_claude="off")["ok"] == 1
            failed, text = call(w.config(man["session"]), "create", account="native")
            assert failed and "native_off" in text, text
        finally:
            host.stop()
