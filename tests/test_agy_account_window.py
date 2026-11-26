"""M25-P11 in the real window: the provider of a new account is chosen by a picture (Claude, agy, Codex), then the account gets a name, and an account of agy is signed in with the program's own
sign-in (the double of `tests/fixtures/login/fake_agy_login.py`; never the real program, never a real Google account). The window's PATH holds a script named `agy` so that the picture can be chosen
(the window looks for the program before it lets the picture be taken), and the host of the window runs the simulator of `agy acp` as its `agy`."""

from __future__ import annotations

import sys
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import centre
from tests.test_agy_accounts import FAKE, URL
from tests.test_agy_session import simulator_args
from tests.test_agy_tile import fake_agy
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_terminal_tab import rig

CODE = "CANARYCODE9"


def host_args(root: Path) -> list[str]:
    return [*simulator_args(root / "agy_received.txt"), "--login-bin", sys.executable, "--login-arg", "-I", "--login-arg", str(FAKE), "--login-arg", "code"]


def pictures(w: ProductWindow) -> list[dict]:
    """The pictures of the provider step of a new account, once they are drawn."""
    return wait_for(lambda: (p := w.ui()["panel"]) and p["acct"]["providers"] and all(t["rect"] for t in p["acct"]["providers"]) and p["acct"]["providers"])


def test_a_new_account_is_started_by_the_picture_of_its_provider_and_an_account_of_agy_is_named_and_signed_in_like_the_others():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        fake_agy(root / "bin")
        with ProductWindow(**rig(root, path_prefix=str(root / "bin"), host_args=host_args(root), env={"LICO_NO_AUTO_OPEN": "1"})) as w:
            conn = Conn(w.socket)
            w.key("ctrl+a")
            wait_for(lambda: (u := w.ui())["panel"] is not None and u["panel"]["title"] == "Accounts")
            w.key("a")
            wait_for(lambda: w.ui()["panel"]["title"] == "New claude account")
            shown = pictures(w)
            assert [(t["provider"], t["icon"], t["word"]) for t in shown] == [("claude", "claude", "Claude"), ("agy", "agy", "agy"), ("codex", "codex", "Codex")], "the owner's order: Claude, agy, Codex"
            assert [t["enabled"] for t in shown] == [True, True, True] and [t["chosen"] for t in shown] == [True, False, False]
            sides = {(round(t["rect"][2] - t["rect"][0]), round(t["rect"][3] - t["rect"][1])) for t in shown}
            assert len(sides) == 1 and next(iter(sides))[0] >= 40, "three squares of one size, as the wizard's"
            body = "\n".join(w.ui()["panel"]["body"])
            assert "Target:" in body and "local (this machine)" in body, "Claude and Codex may run on an ssh host"
            # the keys: Shift+Tab goes back round the three (Claude, then Codex), Tab forward again (with all three there, a step back is not a step forward)
            w.key("shift+Tab")
            wait_for(lambda: w.ui()["panel"]["title"] == "New codex account")
            w.key("Tab")
            wait_for(lambda: w.ui()["panel"]["title"] == "New claude account")
            # the picture of agy chooses agy: its title, no host to choose, and the other two are not chosen
            w.click(*centre(shown[1]["rect"]))
            wait_for(lambda: w.ui()["panel"]["title"] == "New agy account")
            assert [t["chosen"] for t in pictures(w)] == [False, True, False]
            assert "Target:" not in "\n".join(w.ui()["panel"]["body"]), "the program is local: no host to choose"
            w.click(*centre(shown[2]["rect"]))
            wait_for(lambda: w.ui()["panel"]["title"] == "New codex account")
            w.click(*centre(shown[0]["rect"]))
            wait_for(lambda: w.ui()["panel"]["title"] == "New claude account")
            w.click(*centre(shown[1]["rect"]))
            wait_for(lambda: w.ui()["panel"]["title"] == "New agy account" and w.ui()["panel"]["text_entry"])
            # the name, then the account: provider agy, no host, and its page is the page of any account
            w.type_text("Google")
            w.key("Return")
            wait_for(lambda: w.ui()["panel"]["title"] == "Account: agy / Google")
            [made] = [a for a in conn.call(op="account.list")["accounts"] if a["provider"] == "agy"]
            assert made["label"] == "Google" and made["ssh_host"] is None and made["default"] is True
            assert not [a for a in conn.call(op="account.list")["accounts"] if a["provider"] != "agy" and a["label"] == "Google"]
            # the sign-in is the program's own: the address is shown, the code is asked for, typed as stars, and the account is signed in
            w.key("s")
            wait_for(lambda: (u := w.ui())["panel"]["title"] == "Sign in: agy / Google" and URL in u["panel"]["body"] and u["panel"]["text_entry"])
            w.type_text(CODE)
            masked = wait_for(lambda: (u := w.ui())["panel"]["input_len"] == len(CODE) and u)
            assert masked["panel"]["input"] == "*" * len(CODE) and CODE not in str(masked)
            w.key("Return")
            wait_for(lambda: (u := w.ui())["panel"]["title"] == "Account: agy / Google" and "Signed in." in u["panel"]["body"])
            assert (Path(w.directory) / "state" / "lico" / "accounts" / made["account"] / "home" / ".gemini" / "antigravity-cli" / "signed_in").exists(), "what the program kept lands in the account's own home"
            # the list shows it with the picture of agy
            w.key("Escape")
            listed = wait_for(lambda: (u := w.ui())["panel"]["title"] == "Accounts" and u["panel"]["acct"]["cards"] and u)
            assert [c["icon"] for c in listed["panel"]["acct"]["cards"] if not c["plus"]] == ["agy"]


def test_the_picture_of_agy_is_grey_when_the_program_is_not_on_this_machine_and_says_so_and_tab_goes_by_the_ones_that_can_be_chosen():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)                                                                                   # no `agy` anywhere the window looks
        with ProductWindow(**rig(root, host_args=host_args(root), env={"LICO_NO_AUTO_OPEN": "1"})) as w:
            w.key("ctrl+a")
            wait_for(lambda: (u := w.ui())["panel"] is not None and u["panel"]["title"] == "Accounts")
            w.key("a")
            shown = pictures(w)
            assert [t["enabled"] for t in shown] == [True, False, True], shown
            w.click(*centre(shown[1]["rect"]))
            wait_for(lambda: "agy is not installed on this machine." in w.ui()["panel"]["body"])
            assert w.ui()["panel"]["title"] == "New claude account", "a grey picture is not taken"
            w.key("Tab")
            wait_for(lambda: w.ui()["panel"]["title"] == "New codex account")
            w.key("Tab")
            wait_for(lambda: w.ui()["panel"]["title"] == "New claude account")
            w.key("shift+Tab")
            wait_for(lambda: w.ui()["panel"]["title"] == "New codex account")
