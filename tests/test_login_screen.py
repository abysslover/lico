"""M25-P13 in the real window: the sign-in is a screen of pictures for every provider (the steps with a tick, the ring and grey; the address in a box with two buttons; a box that the code is pasted
into, with two buttons; a cross and a picture to try again when it failed), and the text lines under it stay the model. The sign-in programs are the doubles of `tests/fixtures/login/` (never the real
programs, never a real account)."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_measure.xpixel import PixelReader
from tests.account_helpers import account
from tests.gui_helpers import centre
from tests.test_agy_accounts import FAKE as FAKE_AGY, URL as URL_AGY
from tests.test_agy_session import simulator_args
from tests.test_agy_tile import fake_agy
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_login import FAKE as FAKE_CODEX, URL as URL_CODEX
from tests.test_terminal_tab import rig

CODE = "CANARYCODE9"


def host_args(root: Path, double: Path, mode: str = "code") -> list[str]:
    return [*simulator_args(root / "agy_received.txt"), "--login-bin", sys.executable, "--login-arg", "-I", "--login-arg", str(double), "--login-arg", mode]


def screen(w: ProductWindow) -> dict:
    """The sign-in screen as the window says it."""
    return (w.ui()["panel"] or {}).get("acct", {}).get("login") or {}


def progress(w: ProductWindow) -> list[str]:
    return [s["progress"] for s in screen(w).get("steps", [])]


def open_sign_in(w: ProductWindow, conn: Conn, account_id: str, title: str) -> None:
    """Accounts, the account's page, then `S`: the sign-in screen."""
    w.key("ctrl+a")
    wait_for(lambda: (u := w.ui())["panel"] is not None and u["panel"]["title"] == "Accounts")
    w.key("1")
    wait_for(lambda: w.ui()["panel"]["title"] == f"Account: {title}")
    w.key("s")
    wait_for(lambda: w.ui()["panel"]["title"] == f"Sign in: {title}")


def test_an_agy_sign_in_is_steps_an_address_and_a_box_to_paste_into_and_the_buttons_work():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        fake_agy(root / "bin")
        with ProductWindow(**rig(root, path_prefix=str(root / "bin"), host_args=host_args(root, FAKE_AGY), env={"LICO_NO_AUTO_OPEN": "1"})) as w:
            conn = Conn(w.socket)
            made = account(conn, "Google", provider="agy")
            open_sign_in(w, conn, made, "agy / Google")
            shown = wait_for(lambda: (s := screen(w)) and s["pictured"] and s["field"] and s["field"]["active"] and s)
            assert [(s["icon"], s["word"]) for s in shown["steps"]] == [("browser", "Open"), ("key", "Sign in"), ("paste", "Code")]
            assert progress(w) == ["done", "done", "active"], "the address is out, the program asks for the code"
            assert shown["address"] == URL_AGY and shown["retry"] is False and shown["device"] is False
            # every part is painted, inside the window, and the text lines under the veil are still the model
            for rect in [s["rect"] for s in shown["steps"]] + [shown["address_rect"], shown["field"]["rect"], *shown["buttons"].values()]:
                assert rect and rect[2] > rect[0] and rect[3] > rect[1], shown
            assert shown["address_rect"][3] <= shown["field"]["rect"][1], "the address is above the box"
            body = "\n".join(w.ui()["panel"]["body"])
            assert URL_AGY in body and "The provider asks for a code" in body
            tiles = {round(r[2] - r[0]) for r in [s["rect"] for s in shown["steps"]]}
            assert len(tiles) == 1 and next(iter(tiles)) >= 40, "pictures of one size, as the wizard's"
            # the address buttons: copy puts the address on the clipboard
            reader = PixelReader(w.xvfb.display)
            try:
                assert reader.selection_owner("CLIPBOARD") == 0, "showing the address copies nothing"
                w.click(*centre(shown["buttons"]["copy"]))
                wait_for(lambda: reader.selection_owner("CLIPBOARD") != 0)
                assert reader.selection_text("CLIPBOARD") == URL_AGY
            finally:
                reader.close()
            # the Paste button takes what is on the clipboard into the box (as stars), and the dump has the length and never the code
            subprocess.run(["xclip", "-selection", "clipboard", "-i"], input=CODE.encode(), env={"DISPLAY": w.xvfb.display, "PATH": "/usr/bin:/bin"}, check=True, timeout=10)
            w.click(*centre(shown["buttons"]["paste"]))
            typed = wait_for(lambda: (s := screen(w)) and s["field"]["stars"] == len(CODE) and s)
            assert typed["steps"][2]["progress"] == "active" and CODE not in str(w.ui())
            panel = w.ui()["panel"]
            assert panel["input"] == "*" * len(CODE) and panel["input_len"] == len(CODE)
            # the Send button sends it, as Enter does
            w.click(*centre(typed["buttons"]["send"]))
            wait_for(lambda: (u := w.ui())["panel"]["title"] == "Account: agy / Google" and "Signed in." in u["panel"]["body"])
            assert (Path(w.directory) / "state" / "lico" / "accounts" / made / "home" / ".gemini" / "antigravity-cli" / "signed_in").exists()
            assert CODE not in str(w.ui()), "what was sent is gone from the window, and the page that follows has an empty input line"


def test_a_sign_in_that_failed_shows_a_cross_and_a_picture_to_try_again_that_starts_it_again():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        fake_agy(root / "bin")
        with ProductWindow(**rig(root, path_prefix=str(root / "bin"), host_args=host_args(root, FAKE_AGY), env={"LICO_NO_AUTO_OPEN": "1"})) as w:
            conn = Conn(w.socket)
            made = account(conn, "Google", provider="agy")
            open_sign_in(w, conn, made, "agy / Google")
            wait_for(lambda: (s := screen(w)) and s["field"] and s["field"]["active"] and s["pictured"])
            w.type_text("WRONGCODE")
            wait_for(lambda: screen(w)["field"]["stars"] == len("WRONGCODE"))
            w.click(*centre(screen(w)["buttons"]["send"]))
            failed = wait_for(lambda: (s := screen(w)) and s["retry"] and s)
            assert [s["progress"] for s in failed["steps"]] == ["failed", "todo", "todo"] and failed["steps"][0]["icon"] == "browser"
            assert failed["address"] is None and failed["field"] is None and failed["retry_rect"]
            assert "did not work" in failed["note"] and "Again" in failed["note"]
            assert "Code sent." not in "\n".join(w.ui()["panel"]["body"]), "the sentence of the screen says what happened"
            assert all(b is None for b in failed["buttons"].values()), "no address and no box to use when it is over"
            # the picture to try again starts the sign-in again: the steps are back, the program asks again
            w.click(*centre(failed["retry_rect"]))
            again = wait_for(lambda: (s := screen(w)) and not s["retry"] and s["field"] and s["field"]["active"] and s)
            assert [s["progress"] for s in again["steps"]] == ["done", "done", "active"] and again["address"] == URL_AGY
            w.type_text(CODE)
            wait_for(lambda: screen(w)["field"]["stars"] == len(CODE))
            w.key("Return")
            wait_for(lambda: (u := w.ui())["panel"]["title"] == "Account: agy / Google" and "Signed in." in u["panel"]["body"])


def test_a_codex_sign_in_has_two_steps_and_the_second_way_and_no_box_until_the_program_asks_for_a_code():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        with ProductWindow(**rig(root, host_args=[*host_args(root, FAKE_CODEX, "hang")], env={"LICO_NO_AUTO_OPEN": "1"})) as w:
            conn = Conn(w.socket)
            made = account(conn, "Work", provider="codex")
            open_sign_in(w, conn, made, "codex / Work")
            shown = wait_for(lambda: (s := screen(w)) and s["pictured"] and s["address"] and s)
            assert shown["address"] == URL_CODEX
            assert [s["word"] for s in shown["steps"]] == ["Open", "Sign in"], "no Code picture while no code is asked for"
            assert progress(w) == ["done", "active"]
            assert shown["field"] is None and shown["device"] is True
            assert "D  use the device-code sign-in instead" in "\n".join(w.ui()["panel"]["body"]), "the key stays"
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None or w.ui()["panel"]["title"] != "Sign in: codex / Work")
