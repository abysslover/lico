"""M17-P07: the browser tab in the window. The host's browser tabs (`browser.*`) run the FAKE engine of the system tests (`lico-fake-engine`, named in the owner's setting file of
a private state folder); no real site and no real name lookup is touched. A screenshot of each state is kept for the owner under `private/scratch/.../m17_p07_screens`."""

from __future__ import annotations

import json
import os
import shutil
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import HOSTD_DIR, LICO, ProtoConn as Conn
from tests.test_codex_adapter import wait_for

SCREENS = Path(os.environ.get("LICO_P07_SCREENS", "")) if os.environ.get("LICO_P07_SCREENS") else None


def shot(w: ProductWindow, name: str) -> None:
    if SCREENS:
        SCREENS.mkdir(parents=True, exist_ok=True)
        w.png(SCREENS / name)


def fake_engine(w: ProductWindow) -> None:
    """The fake engine in a folder of the private home, named by the owner-only setting of the state folder (the host's state folder is `state/lico`)."""
    home, state = Path(w.directory) / "home", Path(w.directory) / "state" / "lico"
    engine = home / "engine"
    engine.mkdir(mode=0o700, parents=True, exist_ok=True)
    shutil.copy(HOSTD_DIR / "lico-fake-engine", engine / "obscura")
    (engine / "obscura").chmod(0o700)
    setting = state / "browser.json"
    setting.write_text(json.dumps({"engine_dir": str(engine)}))
    setting.chmod(0o600)


def centre(rect):
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def browser(w: ProductWindow) -> dict:
    return w.ui()["browser"]


def open_tab(w: ProductWindow, address: str, local: bool = False) -> str:
    u = w.ui()
    w.click(*centre(u["tabs"]["new_browser_local_rect" if local else "new_browser_rect"]))
    wait_for(lambda: browser(w)["prompt"] is not None and browser(w)["shown"])
    w.type_text(address)
    w.key("Return")
    return wait_for(lambda: browser(w)["active"])


def painted(w: ProductWindow, tab: str, minimum: int = 1) -> dict:
    return wait_for(lambda: (b := browser(w)) and next((t for t in b["tabs"] if t["id"] == tab and t["decoded"] and t["painted"] >= minimum), None))


def test_a_new_tab_asks_for_an_address_then_the_page_is_painted_and_the_address_navigates():
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        conn = Conn(w.socket)
        u = w.ui()
        assert u["tabs"]["new_browser_rect"] and u["tabs"]["new_browser_local_rect"], "the two + buttons are drawn"
        w.click(*centre(u["tabs"]["new_browser_rect"]))
        b = wait_for(lambda: (b := browser(w)) and b["shown"] and b["prompt"] is False and b)
        assert "type an address" in b["header"] and b["help"], b
        assert any("lico browser open https://SITE" in line for line in b["help_lines"]), b["help_lines"]
        shot(w, "1_new_tab_prompt_and_help.png")
        w.type_text("https://pages.test/start")
        w.key("Return")
        tab = wait_for(lambda: browser(w)["active"])
        t = painted(w, tab)
        b = browser(w)
        assert b["last_pixel"] & 0xFF == 0x5A and (b["last_pixel"] >> 24) == 0xFF, f"the first pixel of the fake's frame is painted at the page area: {b['last_pixel']:#x}"
        assert b["rect"][2] > 400 and b["rect"][3] > 200 and b["rect"][0] > 0, b["rect"]
        wait_for(lambda: "A fake page" in (browser(w)["header"] or ""))
        assert "ready" in browser(w)["header"]
        assert t["sent"] and t["sent"][0] == b["rect"][2], "the host was told the size of the area"
        row = next(r for r in conn.call(op="browser.list")["tabs"] if r["tab"] == tab)
        assert row["shown"] and row["viewport"][0] == b["rect"][2], row
        shot(w, "2_page_painted.png")
        # the address row follows the page, and Enter on a new address navigates
        wait_for(lambda: "pages.test/start" in w.ui()["input"])
        w.key("ctrl+u")  # clears the input line
        w.type_text("https://pages.test/two")
        w.key("Return")
        wait_for(lambda: any(r["url"] == "https://pages.test/two" for r in conn.call(op="browser.list")["tabs"]))
        # back and reload are Lico's own keys: the page never sees them
        frames = painted(w, tab)["frame_seq"]
        w.key("alt+Left")
        wait_for(lambda: any(r["url"] == "https://pages.test/start" for r in conn.call(op="browser.list")["tabs"]))
        w.key("F5")
        wait_for(lambda: painted(w, tab)["frame_seq"] > frames)
        # a refused address is said at once in the window's own line, and nothing reaches the host's tab
        w.key("ctrl+u")  # clears the input line
        w.type_text("ftp://x.test/")
        w.key("Return")
        time.sleep(1.0)
        u = w.ui()
        assert "only http and https" in (u["header"] or ""), (u["header"], u["input"], u["browser"]["header"])
        # a prompt for another tab hides this one: the host stops its screencast
        w.click(*centre(w.ui()["tabs"]["new_browser_rect"]))
        wait_for(lambda: not next(r for r in conn.call(op="browser.list")["tabs"] if r["tab"] == tab)["shown"])
        assert browser(w)["tabs"][0]["decoded"] is False, "a hidden tab keeps only its compressed frame in the window"
        # closing the tab (Ctrl+W on the tab) ends it and, being the last, the engine
        assert len(browser(w)["tabs"]) == 1
        w.type_text("https://pages.test/three")
        w.key("Return")
        tab2 = wait_for(lambda: (a := browser(w)["active"]) and a != tab and a)
        painted(w, tab2)
        w.key("ctrl+w")
        wait_for(lambda: [r["tab"] for r in conn.call(op="browser.list")["tabs"]] == [tab])
        wait_for(lambda: browser(w)["active"] == tab)
        w.key("ctrl+w")
        wait_for(lambda: conn.call(op="browser.list")["tabs"] == [] and not conn.call(op="browser.list")["engines"][0]["running"])


def test_a_local_tab_says_so_and_refusals_say_why():
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        conn = Conn(w.socket)
        open_tab(w, "http://192.168.1.5/", local=True)
        b = wait_for(lambda: (b := browser(w)) and "only this machine's own services" in (b["header"] or "") and b)
        assert "this tab can reach services on this machine and no other site" in b["header"], b["header"]
        shot(w, "3_local_tab_refusal.png")
        open_tab(w, "http://93.184.216.34:25/")
        b = wait_for(lambda: (b := browser(w)) and "not one of 80, 443, 8080 and 8443" in (b["header"] or "") and b)
        assert "this tab can reach services" not in b["header"], "an ordinary tab does not carry the local sentence"
        shot(w, "4_port_refusal.png")
        assert len(conn.call(op="browser.list")["tabs"]) == 2


def test_an_engine_that_is_not_installed_shows_where_to_get_it_and_where_to_put_it():
    with ProductWindow(host_binary=LICO) as w:
        open_tab(w, "https://pages.test/start")
        b = wait_for(lambda: (b := browser(w)) and ".local/share/lico/browser/v0.2.6" in (b["header"] or "") and b)
        h = b["header"]
        assert "the engine is not installed" in h and "https://github.com/h4ckf0r0day/obscura/releases/download/v0.2.6/" in h and "c1400eec0b1572ea9b583b1544feafb0b25cea4e155b353dbba9c144d94ae6e3" in h, h
        shot(w, "5_absent_engine.png")


def test_korean_composes_in_the_address_row_and_nothing_reaches_a_page():
    from lico_measure.ime import SENTENCE_KEYS, SENTENCE_TEXT

    with ProductWindow(host_binary=LICO, before_window=fake_engine, ime=True) as w:
        conn = Conn(w.socket)
        u = w.ui()
        w.click(*centre(u["tabs"]["new_browser_rect"]))
        wait_for(lambda: browser(w)["prompt"] is not None and browser(w)["shown"])
        time.sleep(0.5)
        w.key("shift+space")                                   # the engine starts in Latin mode: its trigger key switches to Hangul
        time.sleep(0.3)
        w.key("--delay", "60", *SENTENCE_KEYS)
        u = wait_for(lambda: (u := w.ui()) and "안녕하세요" in u["input"] and u)
        assert SENTENCE_TEXT.strip() in u["input"], u["input"]
        assert u["browser"]["prompt"] is False and u["browser"]["tabs"] == [], "a composition in the address row opens nothing"
        assert conn.call(op="browser.list")["tabs"] == [], "and the host has no tab: the text stayed in the row"
        shot(w, "6_address_row_korean_composition.png")
