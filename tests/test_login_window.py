import json
import sys
import time

from lico_measure.p02 import ProtoConn as Conn, start
from lico_measure.rig import Xvfb, private_dir
from lico_measure.xpixel import PixelReader

from tests.account_helpers import account
from tests.test_codex_adapter import FIXTURE, SIM, wait_for
from tests.test_first_view import start_client, ui, xdo
from tests.test_login import FAKE, URL, home_of


def panel_host(directory):
    received = f"{directory}/received.jsonl"
    extra = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", str(SIM), "--codex-arg", "--received", "--codex-arg", received,
             "--codex-arg", "--fixture", "--codex-arg", str(FIXTURE),
             "--login-bin", sys.executable, "--login-arg", "-I", "--login-arg", str(FAKE), "--login-arg", "code"]
    return start(directory, extra=extra, sessions=0, producing=0, history=0)


def panel_of(client, until):
    return ui(client, lambda u: u["panel"] is not None and until(u["panel"]))["panel"]


def test_new_session_screen_creates_a_session_bound_to_the_chosen_account():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log = panel_host(directory)
            conn = Conn(host.socket)
            first = account(conn, "Work")
            work = account(conn, "Other")                                     # the second account: choosing it must matter
            client = start_client(host, xvfb.display)
            assert ui(client)["panel"] is None                                # nothing is open until a person asks
            xdo(xvfb.display, "key", "ctrl+n")
            screen = panel_of(client, lambda p: p["form"] is not None)
            assert screen["title"] == "New session" and screen["form"]["provider"] == "codex" and screen["form"]["account"] == first
            assert any("< Work (default) >" in line for line in screen["body"])
            xdo(xvfb.display, "key", "Up")                                    # from the folder row to the account row
            xdo(xvfb.display, "key", "Right")
            assert panel_of(client, lambda p: p["form"]["account"] == work)["form"]["focus"] == "Account"
            assert conn.call(op="status")["sessions"] == []                  # choosing creates nothing
            xdo(xvfb.display, "key", "Return")
            state = ui(client, lambda u: u["panel"] is None and len(u["rows"]) == 1)
            made = conn.call(op="status")["sessions"]
            assert len(made) == 1 and made[0]["account"] == work and made[0]["provider"] == "codex"
            assert first != work and state["rows"][0]["label"] == "Other" and state["selected"] == made[0]["session"]
            # a new account is made on the screen, with the input line as its text field, and backing out creates no session
            xdo(xvfb.display, "key", "ctrl+n")
            assert panel_of(client, lambda p: p["form"] is not None)["form"]["account"] == work      # the screen starts from what was used last
            xdo(xvfb.display, "key", "Up")
            xdo(xvfb.display, "key", "Right")                                 # after the last account: add a new one
            assert panel_of(client, lambda p: p["form"]["account"] == "")["form"]["problem"].startswith("add an account")
            xdo(xvfb.display, "key", "Return")
            assert panel_of(client, lambda p: p["text_entry"] and p["form"] is not None)["title"] == "New codex account"
            xdo(xvfb.display, "type", "Home")
            xdo(xvfb.display, "key", "Return")
            back = panel_of(client, lambda p: p["title"] == "New session" and p["form"]["account"] != "")["form"]
            home = [a["account"] for a in conn.call(op="account.list")["accounts"] if a["label"] == "Home"]
            assert back["focus"] == "SignIn" and [back["account"]] == home
            assert sorted(a["label"] for a in conn.call(op="account.list")["accounts"]) == ["Home", "Other", "Work"]
            xdo(xvfb.display, "key", "Escape")
            ui(client, lambda u: u["panel"] is None)
            assert len(conn.call(op="status")["sessions"]) == 1
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_sign_in_panel_shows_the_url_takes_a_pasted_code_and_clears_it():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = reader = None
        try:
            host, log = panel_host(directory)
            conn = Conn(host.socket)
            work = account(conn, "Work")
            client = start_client(host, xvfb.display)
            reader = PixelReader(xvfb.display)
            class Recording:                                                   # everything the client prints, to look for the code in it
                def __init__(self, queue):
                    self.queue, self.seen = queue, []

                def __getattr__(self, name):
                    return getattr(self.queue, name)

                def get(self, *args, **kwargs):
                    message = self.queue.get(*args, **kwargs)
                    self.seen.append(message)
                    return message

            client.lines = Recording(client.lines)
            xdo(xvfb.display, "key", "ctrl+a")
            assert "1  codex / Work  (default)" in panel_of(client, lambda p: p["title"] == "Accounts")["body"]
            xdo(xvfb.display, "key", "1")
            panel_of(client, lambda p: p["title"] == "Account: codex / Work")
            xdo(xvfb.display, "key", "s")
            shown = panel_of(client, lambda p: URL in p["body"])               # the provider's own address, as the host cleaned it
            assert shown["title"] == "Sign in: codex / Work"
            assert reader.selection_owner("CLIPBOARD") == 0                    # showing the address copies nothing
            xdo(xvfb.display, "key", "ctrl+shift+c")        # a chord, because the input line is already taking the code
            wait_for(lambda: reader.selection_owner("CLIPBOARD") != 0)
            assert reader.selection_text("CLIPBOARD") == URL
            panel_of(client, lambda p: p["text_entry"])                        # the provider asked for a code
            xdo(xvfb.display, "type", "CANARYCODE98")                        # a typo, taken back with Backspace in the same line
            ui(client, lambda u: u["panel"]["input_len"] == 12)
            xdo(xvfb.display, "key", "BackSpace")
            ui(client, lambda u: u["panel"]["input_len"] == 11)
            xdo(xvfb.display, "key", "Return")
            state = ui(client, lambda u: u["panel"] is not None and "Signed in." in u["panel"]["body"])
            assert state["panel"]["input"] == "" and "CANARYCODE9" not in json.dumps(state)      # cleared from the window at once
            assert not [m for m in client.lines.seen if m.get("ev") == "ime" and m.get("text")], "the input line was echoed to standard output while the code was typed"
            account_dir = home_of(directory, work)
            assert (account_dir / "home" / "received_code").read_text() == "CANARYCODE9" and (account_dir / "codex" / "auth.json").exists()
            assert state["panel"]["title"] == "Account: codex / Work"            # back where the sign-in was asked for
            xdo(xvfb.display, "key", "Escape")
            panel_of(client, lambda p: p["title"] == "Accounts")
            xdo(xvfb.display, "key", "Escape")
            ui(client, lambda u: u["panel"] is None)
        finally:
            if reader:
                reader.close()
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
