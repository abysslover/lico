import sys
import time

from lico_measure.p02 import ProtoConn as Conn, start
from lico_measure.rig import REPO, Xvfb, private_dir

from tests.test_account_helpers import account
from tests.test_codex_adapter import wait_for
from tests.test_first_view import ui, xdo
from tests.test_login import home_of
from tests.test_paste_open import opened, own, sign_in_panel, window
from tests.test_login_window import panel_of

REAL = REPO / "tests" / "fixtures" / "login" / "real_shape_login.py"
URL = "https://claude.example.invalid/oauth/authorize?code=true&client_id=FAKECLIENT&state=FAKESTATE1&code_challenge=FAKECHALLENGE2"


def real_shape_host(directory):
    extra = ["--login-bin", sys.executable, "--login-arg", "-I", "--login-arg", str(REAL)]
    return start(directory, extra=extra, sessions=0, producing=0, history=0)


def test_the_host_reads_a_prompt_without_a_newline_and_a_link_in_an_osc_8_sequence():
    with private_dir() as directory:
        host, log = real_shape_host(directory)
        try:
            conn = Conn(host.socket)
            who = account(conn, "Work", "claude")
            assert conn.call(op="account.login.start", account=who)["ok"] == 1
            shown = {}
            wait_for(lambda: shown.update(conn.call(op="account.login.status", account=who)) or shown.get("needs_code"))
            assert shown["url"] == URL and shown["state"] == "code"            # the clean address, not the link text glued to it
            assert conn.call(op="account.login.input", account=who, code="CANARYCODE9")["ok"] == 1
            wait_for(lambda: conn.call(op="account.login.status", account=who)["state"] == "succeeded")
            account_dir = home_of(directory, who)
            assert (account_dir / "home" / "received_raw").read_bytes() == b"CANARYCODE9\n"      # exactly the code and a newline
            assert "BROWSER" in __import__("json").loads((account_dir / "home" / "seen.json").read_text())["env"]   # one opener only: the window's
        finally:
            host.stop()


def test_the_window_takes_a_pasted_code_for_the_real_prompt_and_opens_one_clean_address():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log = real_shape_host(directory)
            conn = Conn(host.socket)
            who = account(conn, "Work", "claude")
            client, calls = window(host, xvfb.display, directory)
            sign_in_panel(xvfb.display)
            shown = panel_of(client, lambda p: URL in p["body"] and p["text_entry"])
            assert any("paste it below" in line for line in shown["body"])
            own(xvfb.display, "CANARYCODE9")
            xdo(xvfb.display, "key", "ctrl+v")
            ui(client, lambda u: u["panel"]["input_len"] == 11)
            xdo(xvfb.display, "key", "Return")
            state = ui(client, lambda u: u["panel"] is not None and "Signed in." in u["panel"]["body"])
            assert "CANARYCODE9" not in __import__("json").dumps(state)
            assert (home_of(directory, who) / "home" / "received_raw").read_bytes() == b"CANARYCODE9\n"
            time.sleep(1.5)
            assert opened(calls) == [URL]                                       # once, the clean address, never the code
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
