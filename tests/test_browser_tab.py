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
    """The fake engine, in the shape of Moli, in a folder of the private home, named by the owner-only setting of the state folder (the host's state folder is `state/lico`)."""
    home, state = Path(w.directory) / "home", Path(w.directory) / "state" / "lico"
    engine = home / "engine"
    engine.mkdir(mode=0o700, parents=True, exist_ok=True)
    shutil.copy(HOSTD_DIR / "lico-fake-engine", engine / "moli")
    (engine / "moli").chmod(0o700)
    setting = state / "browser.json"
    setting.write_text(json.dumps({"engine_dir": str(engine)}))
    setting.chmod(0o600)


def centre(rect):
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def browser(w: ProductWindow) -> dict:
    return w.ui()["browser"]


def click_menu(w: ProductWindow, index: int, label: str) -> None:
    w.click(*centre(w.ui()["menu"]["titles"][index]["rect"]))
    items = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == index and m["items"] and m["items"])
    w.click(*centre(next(i for i in items if i["label"] == label)["rect"]))


def click_new_tab(w: ProductWindow, local: bool = False) -> None:
    """M22-P04: the tab row has no `+B` and `+BL`; the browser icon of the toolbar opens a tab, the Session menu opens one that keeps no history."""
    if local:
        click_menu(w, 1, "New local browser tab")
    else:
        w.click(*centre(next(b for b in w.ui()["toolbar"]["buttons"] if b["id"] == "new_browser")["rect"]))


def open_tab(w: ProductWindow, address: str, local: bool = False) -> str:
    click_new_tab(w, local)
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
        assert any(b["id"] == "new_browser" and b["rect"] for b in u["toolbar"]["buttons"]), "the browser icon is drawn"
        assert not any(k in u["tabs"] for k in ("new_browser_rect", "new_browser_local_rect")), "the tab row has no buttons"
        click_new_tab(w)
        b = wait_for(lambda: (b := browser(w)) and b["shown"] and b["prompt"] is False and b["bar"] and b)
        # M22-P05a: a new tab is an EMPTY page with the bar at the top focused and empty; the explanation is on F1
        assert "type an address" in b["header"] and "F1 help" in b["header"] and not b["help"] and b["page"] == "blank", b
        assert b["bar"]["focus"] and b["bar"]["text"] == "Type an address" and b["bar"]["enabled"] is False, b["bar"]
        assert b["row"] == b["header"] and "F1 help" in b["row"], "the row under the page says what the header says, not the address"
        assert b["bar"]["back"][1] < b["rect"][1] and b["bar"]["field"][2] > b["bar"]["field"][0] + 200, "the bar is above the page and the field has room"
        shot(w, "1_new_tab_empty_page_bar_focused.png")
        w.key("F1")                                            # the help comes with F1 and goes with the same key
        b = wait_for(lambda: (b := browser(w)) and b["help"] and b["page"] == "help" and b)
        assert any("lico browser open https://SITE" in line for line in b["help_lines"]), b["help_lines"]
        shot(w, "1b_new_tab_help_on_f1.png")
        w.key("F1")
        wait_for(lambda: not browser(w)["help"] and browser(w)["page"] == "blank")
        w.type_text("https://pages.test/start")
        w.key("Return")
        tab = wait_for(lambda: browser(w)["active"])
        t = painted(w, tab)
        b = browser(w)
        # the fake's frame is a JPEG (the engine is asked for JPEG, M20-P02): its blue (90) comes back to within the rounding of YCbCr
        assert abs((b["last_pixel"] & 0xFF) - 0x5A) <= 4 and (b["last_pixel"] >> 24) == 0xFF, f"the first pixel of the fake's frame is painted at the page area: {b['last_pixel']:#x}"
        assert b["rect"][2] > 400 and b["rect"][3] > 200 and b["rect"][0] > 0, b["rect"]
        wait_for(lambda: "A fake page" in (browser(w)["header"] or ""))
        assert "ready" in browser(w)["header"]
        # the bar of a page that has the keys shows the address without its https://, the buttons are on
        b = wait_for(lambda: (b := browser(w)) and b["bar"] and b["bar"]["text"] == "pages.test/start" and b)
        assert not b["bar"]["focus"] and b["bar"]["enabled"] and b["page"] == "frame", b["bar"]
        assert t["sent"] and t["sent"][0] == b["rect"][2], "the host was told the size of the area"
        row = next(r for r in conn.call(op="browser.list")["tabs"] if r["tab"] == tab)
        assert row["shown"] and row["viewport"][0] == b["rect"][2], row
        shot(w, "2_page_painted.png")
        w.key("F1")                                            # F1 over a page: the help lines, with the new sentences
        h = wait_for(lambda: (b := browser(w)) and b["help"] and b["help_lines"] and b)
        text = " ".join(h["help_lines"])
        for needle in ["F1 shows and hides", "80, 443, 8080 and 8443", "notice in the bottom row, not an error page", "Moli engine", "Windows platform", "next list", "localhost", "lico browser open https://SITE", "refuse an automated browser", "google.com/search?q=WORDS", "socket has no password"]:
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
        wait_for(lambda: "pages.test/start" in w.ui()["input"] and "pages.test/two" not in w.ui()["input"])   # M17-P11: the row that was not edited follows the page
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
        click_new_tab(w)
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


def test_the_header_says_nothing_of_a_stealth_setting_even_when_an_old_setting_file_names_one():
    """The engine of the tab has no stealth switch (M20, Moli); a 0.6.2 setting file that still says it is read and ignored, and the header never says it."""
    def old_setting(w: ProductWindow) -> None:
        fake_engine(w)
        setting = Path(w.directory) / "state" / "lico" / "browser.json"
        setting.write_text(json.dumps({"engine_dir": str(Path(w.directory) / "home" / "engine"), "stealth": True}))
        setting.chmod(0o600)

    with ProductWindow(host_binary=LICO, before_window=old_setting) as w:
        tab = open_tab(w, "https://pages.test/start")
        painted(w, tab)
        assert "stealth" not in (browser(w)["header"] or ""), "the header says nothing of it"
        row = next(t for t in Conn(w.socket).call(op="browser.list")["tabs"] if t["tab"] == tab)
        assert "stealth" not in row, row


def no_auto_install(w: ProductWindow) -> None:
    """The owner's switch: with `{"auto_install": false}` a tab opened with no engine does not install it (this test must never download anything)."""
    setting = Path(w.directory) / "state" / "lico" / "browser.json"
    setting.write_text(json.dumps({"auto_install": False}))
    setting.chmod(0o600)


def test_an_engine_that_is_not_installed_shows_where_to_get_it_and_where_to_put_it():
    with ProductWindow(host_binary=LICO, before_window=no_auto_install) as w:
        open_tab(w, "https://pages.test/start")
        b = wait_for(lambda: (b := browser(w)) and ".local/share/lico/browser/moli-v1.1.25" in (b["header"] or "") and b)
        h = b["header"]
        assert "the engine is not installed; by hand: download" in h and "https://github.com/lexmount/moli/releases/download/v1.1.25/moli-x86_64-unknown-linux-gnu.tar.gz" in h and "04c73204626054c9ec0064f7fc00abb0dcfa7f8b9c3ec9df5de8e2afc301087f" in h, h
        shot(w, "5_absent_engine.png")


def test_korean_composes_in_the_address_row_and_nothing_reaches_a_page():
    from lico_measure.ime import SENTENCE_KEYS, SENTENCE_TEXT

    with ProductWindow(host_binary=LICO, before_window=fake_engine, ime=True) as w:
        conn = Conn(w.socket)
        click_new_tab(w)
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


def test_the_bottom_row_says_where_typing_goes_a_click_leaves_no_ring_and_the_pointer_changes_over_the_page():
    """M17-P11 (the owner could not tell that the page took the keys: the engine draws no caret): the row under the page says where typing goes and the pointer is a crosshair over the
    page. M22-P05a: the ring at the last click is gone (the owner's word); the bar at the top shows which of the two has the keys by its frame."""
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        conn = Conn(w.socket)
        tab = open_tab(w, "https://pages.test/start")
        painted(w, tab)
        wait_for(lambda: browser(w)["page_focus"])
        assert "typing goes to the page (Ctrl+L: address)" in browser(w)["header"], browser(w)["header"]
        w.click(*page_point(w, 120, 90))
        time.sleep(0.4)
        b = browser(w)
        assert "mark" not in b and "mark_pixel" not in b, "the click ring is gone"
        shot(w, "11_click_no_ring_and_typing_hint.png")
        w.move(*page_point(w, 200, 150))
        wait_for(lambda: browser(w)["pointer"] == "crosshair")
        w.move(30, 300)                                          # over the side list
        wait_for(lambda: browser(w)["pointer"] == "default")
        # the keys leave the page: the row says so and the bar is the one with the frame lit
        w.key("ctrl+l")
        b = wait_for(lambda: (b := browser(w)) and not b["page_focus"] and b)
        assert "typing goes to the address row (click the page to type in it)" in b["header"], b["header"]
        assert "typing goes to the address row" in b["row"], b["row"]
        assert b["bar"]["focus"], b["bar"]
        shot(w, "13_typing_goes_to_the_address_bar.png")
        # an address typed there goes; a click on the page gives the keys back
        w.type_text("https://pages.test/two")
        w.key("Return")
        wait_for(lambda: any(r["url"] == "https://pages.test/two" for r in conn.call(op="browser.list")["tabs"]))
        w.click(*page_point(w, 60, 70))
        wait_for(lambda: browser(w)["page_focus"] and not browser(w)["bar"]["focus"])


def centre_of(rect):
    return centre(rect)


def test_the_buttons_of_the_bar_go_back_forward_and_reload_and_the_field_takes_the_keys_with_the_address_selected():
    """M22-P05a: `‹ › ⟳` run the host's `browser.nav`; a click on the field gives it the keys and selects the address (typing replaces it); the buttons do nothing with no tab."""
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        conn = Conn(w.socket)
        click_new_tab(w)
        b = wait_for(lambda: (b := browser(w)) and b["bar"] and b)
        w.click(*centre_of(b["bar"]["back"]))                    # no tab yet: nothing happens, nothing breaks
        w.click(*centre_of(b["bar"]["reload"]))
        time.sleep(0.3)
        assert conn.call(op="browser.list")["tabs"] == [] and browser(w)["prompt"] is False
        w.type_text("https://pages.test/start")
        w.key("Return")
        tab = wait_for(lambda: browser(w)["active"])
        painted(w, tab)
        w.key("ctrl+l")
        w.type_text("https://pages.test/two")
        w.key("Return")

        def urls():
            return [r["url"] for r in conn.call(op="browser.list")["tabs"]]

        wait_for(lambda: urls() == ["https://pages.test/two"])
        bar = wait_for(lambda: (bar := browser(w)["bar"]) and bar["enabled"] and bar)
        w.click(*centre_of(bar["back"]))
        wait_for(lambda: urls() == ["https://pages.test/start"])
        w.click(*centre_of(bar["forward"]))
        wait_for(lambda: urls() == ["https://pages.test/two"])
        frames = painted(w, tab)["frame_seq"]
        w.click(*centre_of(bar["reload"]))
        wait_for(lambda: painted(w, tab)["frame_seq"] > frames)
        assert browser(w)["page_focus"], "a button does not take the keys from the page"
        # the field: a click gives it the keys and selects the whole address; typing replaces it and Enter goes
        wait_for(lambda: w.ui()["input"] == "https://pages.test/two")
        def field_gray():
            data, width, _ = w.gray()                            # one pixel of the field's top row, above the letters: the selection's colour is lighter than the field's
            return data[int(bar["field"][1] + 3) * width + int(bar["field"][0] + 12)]

        before = field_gray()
        w.click(*centre_of(bar["field"]))
        b = wait_for(lambda: (b := browser(w)) and not b["page_focus"] and b["bar"]["focus"] and b)
        assert field_gray() > before + 25, "the selected address is painted with the selection's colour"
        assert b["bar"]["selection"] == "https://pages.test/two", b["bar"]
        assert b["bar"]["text"] == "https://pages.test/two", "with the keys the field shows the whole address, https:// included"
        shot(w, "14_field_clicked_address_selected.png")
        w.type_text("https://pages.test/three")
        w.key("Return")
        wait_for(lambda: urls() == ["https://pages.test/three"])
        wait_for(lambda: browser(w)["page_focus"])


def test_a_long_address_is_cut_in_the_middle_in_the_bar_and_whole_with_the_keys_and_the_caret_stays_in_the_field():
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        long = "https://pages.test/" + "/".join(f"segment{i}" for i in range(40)) + "/the-end.html"
        tab = open_tab(w, long)
        painted(w, tab)
        b = wait_for(lambda: (b := browser(w)) and b["bar"] and "\u2026" in b["bar"]["text"] and b)
        text, field = b["bar"]["text"], b["bar"]["field"]
        assert text.startswith("pages.test/") and text.endswith("the-end.html") and text.count("\u2026") == 1, text
        shot(w, "15_long_address_cut_in_the_middle.png")
        w.key("ctrl+l")                                          # with the keys: the whole address, the caret at its end and inside the field
        b = wait_for(lambda: (b := browser(w)) and b["bar"]["focus"] and b)
        assert b["bar"]["text"] == long and b["bar"]["selection"] == long, b["bar"]["text"][:60]
        w.key("End")
        caret = wait_for(lambda: (c := browser(w)["bar"]["caret"]) and c)
        assert field[0] < caret[0] < field[2], (caret, field)
        w.key("Home")
        caret = wait_for(lambda: (c := browser(w)["bar"]["caret"]) and c)
        assert field[0] < caret[0] < field[0] + 40, (caret, field)
        data, width, _ = w.gray()                                # the caret is drawn: light pixels on the field's top row at its x (the field around it is dark)
        row = int(field[1] + 3) * width
        assert max(data[row + int(caret[0])], data[row + int(caret[0]) + 1]) > 150 and data[row + int(caret[0]) + 8] < 100, "a light caret on a dark field"
        shot(w, "16_long_address_whole_with_the_keys.png")


def drag(w, x0, x1, y):
    """Press at x0, move to x1, release (the harness has click and move only): a selection by the mouse."""
    since = w.frames()
    wid = w.window_id()
    subprocess.run(["xdotool", "mousemove", "--window", wid, str(int(x0)), str(int(y)), "mousedown", "1", "mousemove", "--window", wid, str(int(x1)), str(int(y)), "mouseup", "1"], env=w.env, check=True)
    w.settle(since, 0.2)


def test_the_mouse_places_the_caret_and_selects_in_the_bar_and_typing_edits_at_the_caret():
    """M22-P05a: the editor's own mouse code works on the bar because the bar gives it the geometry of what it drew (caret, glyph boxes): a click puts the caret between two letters, a drag selects."""
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        address = "https://pages.test/start"
        tab = open_tab(w, address)
        painted(w, tab)
        w.key("ctrl+l")
        w.key("Home")
        bar = wait_for(lambda: (bar := browser(w)["bar"]) and bar["caret"] and bar)
        x_start = bar["caret"][0]
        w.key("End")
        cw = (wait_for(lambda: browser(w)["bar"]["caret"])[0] - x_start) / len(address)
        y = (bar["field"][1] + bar["field"][3]) / 2
        w.click(x_start + 8 * cw + 1, y)                         # between "https://" and "pages"
        wait_for(lambda: browser(w)["bar"]["selection"] in (None, ""))
        w.type_text("X")
        wait_for(lambda: w.ui()["input"] == "https://Xpages.test/start")
        drag(w, x_start + 9 * cw + 1, x_start + 14 * cw + 1, y)  # "pages"
        b = wait_for(lambda: (b := browser(w)) and b["bar"]["selection"] and b)
        assert b["bar"]["selection"] == "pages", b["bar"]
        shot(w, "17_selection_by_the_mouse_in_the_bar.png")
        w.type_text("ok")                                        # typing replaces the selection
        wait_for(lambda: w.ui()["input"] == "https://Xok.test/start")


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
        w.key("--delay", "60", *SENTENCE_KEYS[:6])             # a half composed word: M17-P11: the header shows it (the address row's brackets stay empty while the page has the keys: the picture 12)
        h = wait_for(lambda: (h := browser(w)["header"]) and "composing: " in h and h)
        assert h.split("composing: ")[1].split(" |")[0].strip() != "", h
        shot(w, "12_composing_in_the_header.png")
        w.key("--delay", "60", *SENTENCE_KEYS[6:])
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
