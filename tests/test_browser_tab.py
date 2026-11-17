"""M17-P07: the browser tab in the window. The host's browser tabs (`browser.*`) run the FAKE engine of the system tests (`lico-fake-engine`, named in the owner's setting file of
a private state folder); no real site and no real name lookup is touched. A screenshot of each state is kept for the owner under `private/scratch/.../m17_p07_screens`."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
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
        assert "type an address" in b["header"] and "F1 help" in b["header"] and b["help"], b
        assert any("lico browser open https://SITE" in line for line in b["help_lines"]), b["help_lines"]
        shot(w, "1_new_tab_prompt_and_help.png")
        w.key("F1")                                            # the help goes away and comes back with the same key
        wait_for(lambda: not browser(w)["help"])
        w.key("F1")
        wait_for(lambda: browser(w)["help"])
        w.key("F1")
        wait_for(lambda: not browser(w)["help"])
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
        w.key("F1")                                            # F1 over a page: the help lines, with the new sentences
        h = wait_for(lambda: (b := browser(w)) and b["help"] and b["help_lines"] and b)
        text = " ".join(h["help_lines"])
        for needle in ["F1 shows and hides", "80, 443, 8080 and 8443", "notice in the header, not an error page", "may show as boxes", "Windows platform", "next list", "localhost", "lico browser open https://SITE"]:
            assert needle in text, (needle, text)
        shot(w, "10_f1_help_over_a_page.png")
        w.key("F1")
        wait_for(lambda: not browser(w)["help"])
        # the address row follows the page, and Enter on a new address navigates
        wait_for(lambda: "pages.test/start" in w.ui()["input"])
        w.key("ctrl+l")  # the page has the keys after an address: Ctrl+L gives them to the address row
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
        w.key("ctrl+l")
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


def no_auto_install(w: ProductWindow) -> None:
    """The owner's switch: with `{"auto_install": false}` a tab opened with no engine does not install it (this test must never download anything)."""
    setting = Path(w.directory) / "state" / "lico" / "browser.json"
    setting.write_text(json.dumps({"auto_install": False}))
    setting.chmod(0o600)


def test_an_engine_that_is_not_installed_shows_where_to_get_it_and_where_to_put_it():
    with ProductWindow(host_binary=LICO, before_window=no_auto_install) as w:
        open_tab(w, "https://pages.test/start")
        b = wait_for(lambda: (b := browser(w)) and ".local/share/lico/browser/v0.2.6" in (b["header"] or "") and b)
        h = b["header"]
        assert "the engine is not installed; by hand: download" in h and "https://github.com/h4ckf0r0day/obscura/releases/download/v0.2.6/" in h and "eed986e478c5ec5439f531f2d7fb6b0e36a356f1df7a6c44a8e8fd4f2486bd76" in h, h
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


def input_log(w: ProductWindow) -> list[tuple[str, dict]]:
    """What reached the fake engine, in order (its log lives in the scratch folder of the engine's run)."""
    out = []
    for log in sorted((Path(w.directory) / "state" / "lico" / "browser").glob("scratch-*/input.log")):
        for line in log.read_text().splitlines():
            _, method, params = line.split(" ", 2)
            out.append((method, json.loads(params)))
    return out


def page_point(w: ProductWindow, dx: int, dy: int) -> tuple[int, int]:
    x, y, _, _ = browser(w)["rect"]
    return x + dx, y + dy


def test_the_persons_input_reaches_the_page_and_lico_s_own_keys_do_not():
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        tab = open_tab(w, "https://pages.test/start")
        painted(w, tab)
        wait_for(lambda: browser(w)["page_focus"])
        # a click on the page: a press and a release at the point, in the page's own coordinates
        w.click(*page_point(w, 40, 30))
        wait_for(lambda: [p["type"] for m, p in input_log(w) if m == "Input.dispatchMouseEvent" and p["type"] != "mouseMoved"][-2:] == ["mousePressed", "mouseReleased"])
        press = next(p for m, p in input_log(w) if m == "Input.dispatchMouseEvent" and p["type"] == "mousePressed")
        assert (press["x"], press["y"], press["button"]) == (40.0, 30.0, "left"), press
        # a wheel step over the page
        w.click(*page_point(w, 50, 50), button=5)
        wait_for(lambda: any(p.get("type") == "mouseWheel" and p["deltaY"] > 0 for m, p in input_log(w)))
        # typed text and an editing key
        w.type_text("ab")
        w.key("Return")
        wait_for(lambda: [p["text"] for m, p in input_log(w) if m == "Input.insertText"] == ["a", "b"] and any(m == "Input.dispatchKeyEvent" and p["key"] == "Enter" for m, p in input_log(w)))
        before = len(input_log(w))
        # Lico's own keys are never sent: back, reload, the address row's key
        w.key("alt+Left")
        w.key("F5")
        time.sleep(0.5)
        w.key("ctrl+l")
        wait_for(lambda: not browser(w)["page_focus"])
        w.type_text("zz")                                       # the address row has the keys now
        wait_for(lambda: "zz" in w.ui()["input"])
        time.sleep(0.4)
        later = [(m, p) for m, p in input_log(w)[before:] if m == "Input.dispatchKeyEvent" or m == "Input.insertText"]
        assert later == [], f"nothing of Lico's own keys or of the address row reached the page: {later}"
        # a click on the page gives it the keys again
        w.click(*page_point(w, 10, 10))
        wait_for(lambda: browser(w)["page_focus"])
        shot(w, "8_page_has_the_focus.png")


def test_korean_committed_in_a_page_reaches_it_as_text_and_the_address_row_keeps_its_own():
    from lico_measure.ime import SENTENCE_KEYS, SENTENCE_TEXT

    with ProductWindow(host_binary=LICO, before_window=fake_engine, ime=True) as w:
        tab = open_tab(w, "https://pages.test/start")
        painted(w, tab)
        wait_for(lambda: "pages.test/start" in w.ui()["input"])
        w.click(*page_point(w, 60, 60))
        wait_for(lambda: browser(w)["page_focus"])
        time.sleep(0.5)
        w.key("shift+space")                                   # the engine starts in Latin mode: its trigger key switches to Hangul
        time.sleep(0.3)
        w.key("--delay", "60", *SENTENCE_KEYS)
        wait_for(lambda: "".join(p["text"] for m, p in input_log(w) if m == "Input.insertText").endswith("한글 "))
        sent = "".join(p["text"] for m, p in input_log(w) if m == "Input.insertText")
        assert sent == SENTENCE_TEXT, sent
        assert "pages.test/start" in w.ui()["input"] and "안녕" not in w.ui()["input"].split("[")[0], "the address row did not take the committed text"


def test_a_pasted_text_goes_to_the_page_only_on_the_persons_own_paste_and_is_capped():
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        tab = open_tab(w, "https://pages.test/start")
        painted(w, tab)
        w.click(*page_point(w, 20, 20))
        wait_for(lambda: browser(w)["page_focus"])
        subprocess.run(["xclip", "-selection", "clipboard"], input=("가나다 pasted " + "x" * 6000).encode(), env=w.env, check=True)
        w.key("ctrl+v")
        got = wait_for(lambda: [p["text"] for m, p in input_log(w) if m == "Input.insertText"] or None)
        assert got[0].startswith("가나다 pasted ") and len(got[0]) <= 4096, len(got[0])
        assert len(got) == 1, "one paste is one insert"


def test_at_scale_two_the_page_is_asked_for_at_most_1400_by_900_and_the_tab_says_so():
    import lico_measure.captures as captures

    real = captures.Xvfb
    captures.Xvfb = lambda: real("2400x1600x24")                # the window is 2400 x 1600 pixels at scale 2
    try:
        with ProductWindow(host_binary=LICO, before_window=fake_engine, env_extra={"WINIT_X11_SCALE_FACTOR": "2"}) as w:
            conn = Conn(w.socket)
            tab = open_tab(w, "https://pages.test/start")
            painted(w, tab)
            b = browser(w)
            assert b["rect"][2] > 1400 and b["rect"][3] > 900, f"the area is larger than the cap: {b['rect']}"
            row = wait_for(lambda: next((r for r in conn.call(op="browser.list")["tabs"] if r["tab"] == tab and r["viewport"] == [1400, 900]), None))
            assert row["shown"]
            assert "the page is drawn at 1400 x 900" in wait_for(lambda: (h := browser(w)["header"]) and "drawn at" in h and h), browser(w)["header"]
            shot(w, "11_scale2_cap_said_in_the_tab.png")
    finally:
        captures.Xvfb = real
