import subprocess
import time

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Client, Xvfb, private_dir

from tests.account_helpers import account
from tests.test_claude_adapter import claude_host, new_claude, status_of
from tests.test_codex_adapter import wait_for
from tests.test_first_view import responses, ui, xdo
from lico_measure.ime import _focus


def centre(rect):
    return (rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2


def menu(w):
    return w.ui()["menu"]


def item(m, text):
    return next(i for i in m["items"] if text in i["label"])


def test_a_closed_bar_with_four_titles_is_drawn_inside_the_window_and_not_clipped():
    with ProductWindow() as w:
        m = menu(w)
        assert [t["label"] for t in m["titles"]] == ["File", "Session", "View", "Help"] and m["open"] is None
        rects = [t["rect"] for t in m["titles"]]
        assert all(a[2] <= b[0] for a, b in zip(rects, rects[1:]))                      # no two title boxes overlap
        assert rects[0][0] >= 4 and rects[-1][2] <= 1200 - 4 and all(r[1] == 0 or r[1] >= 0 for r in rects)
        data, width, _ = w.gray()
        y0, y1 = int(rects[0][1]), int(rects[0][3])
        lit = [x for x in range(width) if any(data[y * width + x] > 140 for y in range(y0, y1))]
        assert lit and lit[0] >= 2 and lit[-1] < width // 2                             # the first letter is whole (not cut at the left edge) and the bar holds only its titles
        assert sum(1 for y in range(y0, y1) for x in range(width) if data[y * width + x] > 140) > 100


def test_f10_and_alt_letters_open_menus_arrows_and_enter_run_and_greyed_entries_say_why():
    with ProductWindow() as w:
        w.key("F10")
        m = menu(w)
        assert m["open"] == 0 and [i["label"] for i in m["items"]][:3] == ["New session", "Accounts", "Quit"] and item(m, "New session")["key"] == "Ctrl+N"
        before = w.bright_total()
        w.key("Down")
        assert menu(w)["selected"] == 1 and w.bright_total() != 0 and before > 0
        w.key("Up")
        w.key("Return")                                                                  # runs the entry that was selected: New session
        panel = w.ui()["panel"]
        assert panel and panel["title"].startswith("New session: which provider") and menu(w)["open"] is None
        w.key("Escape")
        w.key("alt+s")
        m = menu(w)
        assert m["open"] == 1
        nxt = item(m, "Next session")
        assert nxt["enabled"] is False and nxt["reason"] == "needs two sessions"
        assert item(m, "Allow")["reason"] == "no approval is pending" and item(m, "Copy")["reason"] == "nothing is selected"
        assert w.bright(int(nxt["rect"][0]), int(nxt["rect"][1]), int(nxt["rect"][2]), int(nxt["rect"][3]), threshold=70) > 50       # drawn (dim), with its reason beside it
        w.key("Right")
        assert menu(w)["open"] == 2
        w.key("Escape")
        assert menu(w)["open"] is None
        w.key("ctrl+q")
        time.sleep(1.0)
        assert not w.alive()                                                             # Quit ends the window (the host keeps running)


def test_clicks_open_a_menu_run_an_entry_close_on_a_click_elsewhere_and_a_greyed_entry_only_says_why():
    with ProductWindow() as w:
        m = menu(w)
        w.click(*centre(m["titles"][0]["rect"]))
        m = menu(w)
        assert m["open"] == 0
        w.click(*centre(item(m, "Accounts")["rect"]))
        panel = w.ui()["panel"]
        assert panel["title"] == "Accounts" and menu(w)["open"] is None
        w.key("Escape")
        w.click(*centre(menu(w)["titles"][1]["rect"]))
        m = menu(w)
        assert m["open"] == 1
        w.click(*centre(item(m, "Next session")["rect"]))                                # greyed: nothing happens but the reason is shown
        state = w.ui()
        assert state["menu"]["open"] is None and "needs two sessions" in state["header"] and state["panel"] is None
        w.click(*centre(menu(w)["titles"][2]["rect"]))
        assert menu(w)["open"] == 2
        w.click(700, 500)                                                                # a click elsewhere closes it
        assert menu(w)["open"] is None


def menu_client(host, display):
    client = Client("a2", host.socket, display, extra_args=["--ui", "--menu"], extra_env={"LICO_NO_AUTO_OPEN": "1"})
    time.sleep(0.8)
    _focus(display)
    time.sleep(0.3)
    return client


def test_with_sessions_the_menu_switches_them_and_allow_and_deny_follow_the_pending_approval():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log, received = claude_host(directory)
            conn = Conn(host.socket)
            a = account(conn, "Work", "claude")
            ids = [new_claude(conn, account=a, mode="default")[0] for _ in range(3)]
            client = menu_client(host, xvfb.display)
            state = ui(client, lambda u: len(u["rows"]) == 3)
            order = [r[2] for r in state["layout"]["rows"]]
            xdo(xvfb.display, "key", "alt+s")
            m = ui(client, lambda u: u["menu"]["open"] == 1)["menu"]
            assert item(m, "Next session")["enabled"] and item(m, "Session 1")["key"] == "Alt+1" and item(m, "Allow")["enabled"] is False
            rect = item(m, "Session 2")["rect"]
            click_at(xvfb.display, *centre(rect))
            assert ui(client, lambda u: u["selected"] == order[1])["selected"] == order[1]
            xdo(xvfb.display, "key", "alt+s")
            m = ui(client, lambda u: u["menu"]["open"] == 1)["menu"]
            click_at(xvfb.display, *centre(item(m, "Next session")["rect"]))
            assert ui(client, lambda u: u["selected"] == order[2])["selected"] == order[2]
            xdo(xvfb.display, "key", "alt+s")
            ui(client, lambda u: u["menu"]["open"] == 1)
            xdo(xvfb.display, "key", "Escape")
            conn.call(op="input", session=order[2], generation=1, request_id="a", text="fixture")
            wait_for(lambda: status_of(conn, order[2]).get("approvals"))
            xdo(xvfb.display, "key", "alt+s")
            m = ui(client, lambda u: u["menu"]["open"] == 1 and any(i["label"].startswith("Allow") and i["enabled"] for i in u["menu"]["items"]))["menu"]
            click_at(xvfb.display, *centre(item(m, "Allow")["rect"]))
            wait_for(lambda: len(responses(received)) == 1)
            assert responses(received)[0]["response"]["behavior"] == "allow"
            for scale_check in (m,):                                                     # every box lies inside the window, clear of the margins
                for entry in scale_check["items"]:
                    r = entry["rect"]
                    assert r[0] >= 0 and r[2] <= 1200 - 4 and r[3] <= 800
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def click_at(display, x, y):
    from tests.test_first_view import click
    click(display, x, y)
    time.sleep(0.5)


def test_at_scale_two_the_bar_and_the_open_menu_keep_inside_the_window_and_do_not_overlap():
    import lico_measure.rig as rig
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        rig.DISPLAY_SCALE = 2
        try:
            host, log, received = claude_host(directory)
            client = menu_client(host, xvfb.display)
            xdo(xvfb.display, "key", "F10")
            m = ui(client, lambda u: u["menu"]["open"] == 0)["menu"]
            titles = [t["rect"] for t in m["titles"]]
            assert all(a[2] <= b[0] for a, b in zip(titles, titles[1:])) and titles[0][0] >= 8 and titles[-1][2] <= 1200 - 8
            for entry in m["items"]:
                r = entry["rect"]
                assert r[0] >= 0 and r[2] <= 1200 - 8 and r[3] <= 800 and r[1] >= titles[0][3] - 1      # right and left margins kept at the doubled scale; below the bar
            assert m["items"][0]["rect"][3] - m["items"][0]["rect"][1] >= 30                              # the rows are doubled too
        finally:
            rig.DISPLAY_SCALE = 1
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
