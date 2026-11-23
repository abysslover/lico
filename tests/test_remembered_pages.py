"""M22-P05c: the browser remembers the pages it has shown, Chats lists them, a click reopens one, the Session menu reopens a closed tab and clears the list. The tabs run the FAKE engine of
the system tests (every page it loads is titled `A fake page`); no real site is touched. The host keeps the list in the owner-only `browser_pages.json` of the private state folder."""

from __future__ import annotations

import json
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from tests.test_browser_tab import browser, centre, fake_engine, go_to, open_tab, painted
from tests.test_codex_adapter import wait_for


def pages_file(w: ProductWindow) -> Path:
    return Path(w.directory) / "state" / "lico" / "browser_pages.json"


def remembered(w: ProductWindow) -> list[str]:
    """The addresses in the host's file, newest first (nothing when there is no file yet)."""
    try:
        return [p["address"] for p in json.loads(pages_file(w).read_text())["pages"]]
    except (OSError, ValueError):
        return []


def page_cards(w: ProductWindow) -> list[dict]:
    """The cards of remembered pages in the Chats tab, once its list has come: the Chats tab is opened (a switch to it fetches again) and the list waited for."""
    s = w.ui()["sidebar"]
    if s["tab"] != 1:
        w.click(*centre(s["chats_tab_rect"]))
        time.sleep(0.2)
    return wait_for(lambda: (s := w.ui()["sidebar"]) and s["chats_loading"] is False and s["chats_load_count"] > 0 and [e for e in s["chats_entries"] if e["provider"] == "browser"] or None)


def pages_menu(w: ProductWindow, label: str) -> None:
    """The group `Browser pages` of the menu More (M24-P04; the tree opens its groups closed): the heading is opened and then the entry clicked."""
    w.click(*centre(w.ui()["menu"]["titles"][4]["rect"]))
    m = wait_for(lambda: (x := w.ui()["menu"]) and x["open"] == 4 and x["items"] and all(b["rect"] for b in x["items"]) and x)
    head = next(i for i in m["items"] if i["kind"] == "group" and i["label"] == "Browser pages")
    if head["open"] is False:
        w.click(*centre(head["rect"]))
    entry = wait_for(lambda: next((i for i in w.ui()["menu"]["items"] if i["kind"] == "entry" and i["label"] == label and i["rect"]), None))
    w.click(*centre(entry["rect"]))


def close_tabs(w: ProductWindow, conn: Conn) -> None:
    left = len(conn.call(op="browser.list")["tabs"])
    for n in range(left - 1, -1, -1):
        w.key("ctrl+w")
        wait_for(lambda: len(conn.call(op="browser.list")["tabs"]) == n or n == 0 and not conn.call(op="browser.list")["tabs"])


def test_a_page_that_loaded_is_remembered_listed_in_chats_reopened_by_a_click_and_by_the_menu_and_cleared():
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        conn = Conn(w.socket)
        tab = open_tab(w, "pages.test/one")
        painted(w, tab)
        wait_for(lambda: remembered(w) == ["https://pages.test/one"])
        assert pages_file(w).stat().st_mode & 0o777 == 0o600, "the list is owner-only"
        time.sleep(1.2)                                       # the time is in seconds: Chats orders by it, so the two pages must not share one
        go_to(w, "pages.test/two")
        wait_for(lambda: remembered(w) == ["https://pages.test/two", "https://pages.test/one"])
        # a tab for this machine's own services leaves nothing
        local = open_tab(w, "http://localhost:8123/", local=True)
        painted(w, local)
        time.sleep(0.3)                                       # the host remembers a page as soon as it has loaded, which is before it is painted
        assert remembered(w) == ["https://pages.test/two", "https://pages.test/one"], "a local tab is never remembered"
        # Chats lists the pages among the chats: the browser's picture, the page's title, no state, newest first
        cards = page_cards(w)
        assert [c["provider_session"] for c in cards] == ["https://pages.test/two", "https://pages.test/one"], cards
        assert all(c["icon"] == "browser" and c["title"] == "A fake page" and c["state"] == "none" for c in cards), cards
        assert all(c["open_session"] is None and c["folder"] is None for c in cards)
        # a click on a card opens that page in a new tab
        before = {r["tab"] for r in conn.call(op="browser.list")["tabs"]}
        w.click(*centre(cards[1]["rect"]))
        wait_for(lambda: {r["tab"] for r in conn.call(op="browser.list")["tabs"]} - before)
        opened = next(r for r in conn.call(op="browser.list")["tabs"] if r["tab"] not in before)
        assert opened["url"] == "https://pages.test/one" and opened["local"] is False
        wait_for(lambda: browser(w)["active"] == opened["tab"])
        assert [t["local"] for t in browser(w)["tabs"] if t["id"] == opened["tab"]] == [False], "the window shows it as an ordinary tab"
        # opening a page shows it again, so it is the newest now; close every tab: Reopen closed tab brings back the newest page no tab shows, then the next one
        wait_for(lambda: remembered(w) == ["https://pages.test/one", "https://pages.test/two"])
        close_tabs(w, conn)
        pages_menu(w, "Reopen closed tab")
        wait_for(lambda: [r["url"] for r in conn.call(op="browser.list")["tabs"]] == ["https://pages.test/one"])
        wait_for(lambda: [t["local"] for t in browser(w)["tabs"]] == [False])    # the window shows the reopened tab as an ordinary one
        pages_menu(w, "Reopen closed tab")
        wait_for(lambda: sorted(r["url"] for r in conn.call(op="browser.list")["tabs"]) == ["https://pages.test/one", "https://pages.test/two"])
        pages_menu(w, "Reopen closed tab")
        wait_for(lambda: "no closed tab to reopen" in (w.ui()["header"] or ""))
        assert len(conn.call(op="browser.list")["tabs"]) == 2, "nothing is left to reopen: no tab was added"
        # clearing empties the file and the list in Chats
        loads = w.ui()["sidebar"]["chats_load_count"]
        pages_menu(w, "Clear remembered pages")
        wait_for(lambda: not pages_file(w).exists())
        wait_for(lambda: "Remembered pages cleared (2)" in (w.ui()["header"] or ""))
        wait_for(lambda: (s := w.ui()["sidebar"]) and s["chats_load_count"] > loads and s["chats_loading"] is False and not [e for e in s["chats_entries"] if e["provider"] == "browser"])
        pages_menu(w, "Clear remembered pages")
        wait_for(lambda: "No remembered pages." in (w.ui()["header"] or ""))


def test_the_host_operations_take_no_address_and_the_list_is_the_only_source_of_one():
    with ProductWindow(host_binary=LICO, before_window=fake_engine) as w:
        conn = Conn(w.socket)
        refused = conn.call(op="browser.reopen")
        assert refused["ok"] != 1 and refused["error"] == "no_page", refused
        assert conn.call(op="browser.forget")["forgotten"] == 0
        for body in ({"url": "https://a.test/"}, {"tab": "b1"}, {"address": "https://a.test/"}):
            assert conn.call(op="browser.reopen", **body)["ok"] != 1, body
            assert conn.call(op="browser.forget", **body)["ok"] != 1, body
        assert not pages_file(w).exists() and conn.call(op="browser.list")["tabs"] == []


def test_session_chats_with_pages_lists_the_remembered_pages_among_the_chats_and_without_pages_is_as_before():
    """The host's side of Chats: `pages` joins the remembered pages to the list of this machine (one list, one order, one paging), the query matches them, an account leaves them out, and
    every caller that does not say `pages` sees what it saw."""
    from lico_measure.p02 import start
    from lico_measure.rig import private_dir
    from lico_protocol import v1
    from tests.account_helpers import account
    from tests.test_chats_pagination_and_resume import _seed_claude_jsonl
    from tests.test_claude_adapter import argv_of  # noqa: F401  (the rig's helper module)
    from tests.test_codex_adapter import usual_umask
    from tests.test_new_session_screen import claude_args

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700, parents=True)
        c_args, _rec = claude_args(root)
        host, _log_dir = start(directory, extra=["--text", "--state-dir", str(state_dir), *c_args], sessions=0, producing=0)
        try:
            conn = Conn(host.socket)
            acc = account(conn, "LocalClaude", "claude")
            home = state_dir / "accounts" / acc / "claude"
            _seed_claude_jsonl(home, "proj-alpha", "cl-a1", str(proj), "alpha question", "answer", 1_760_000_010)
            _seed_claude_jsonl(home, "proj-alpha", "cl-a2", str(proj), "beta question", "answer", 1_760_000_030)
            pages = [
                {"address": "https://pages.test/old", "title": "Old page", "time": 1_760_000_005},
                {"address": "https://pages.test/new?q=1", "title": "", "time": 1_760_000_040},
                {"address": "https://docs.test/beta", "title": "Beta docs", "time": 1_760_000_020},
            ]
            file = state_dir / "browser_pages.json"
            file.write_text(json.dumps({"v": 1, "pages": pages}))
            file.chmod(0o600)

            def ask(**extra):
                reply = conn.call(op="session.chats", all=True, **extra)
                assert reply["ok"] == 1, reply
                return reply

            plain = ask()
            assert [it["provider_session"] for it in plain["items"]] == ["cl-a2", "cl-a1"] and plain["total"] == 2, "no `pages`: what it was"
            assert v1.validate_response("session.chats", plain) == []
            both = ask(pages=True)
            assert v1.validate_request("session.chats", {"op": "session.chats", "all": True, "pages": True}) == []
            assert v1.validate_response("session.chats", both) == [], both
            assert [it["provider_session"] for it in both["items"]] == ["https://pages.test/new?q=1", "cl-a2", "https://docs.test/beta", "cl-a1", "https://pages.test/old"], "one list, newest first"
            assert both["total"] == 5 and both["has_more"] is False
            page = {it["provider_session"]: it for it in both["items"]}
            new = page["https://pages.test/new?q=1"]
            assert (new["provider"], new["title"], new["updated_at"], new["cwd"], new["account"], new["global"], new["open_session"]) == ("browser", "https://pages.test/new?q=1", 1_760_000_040, None, None, False, None), "no title: the address"
            assert page["https://docs.test/beta"]["title"] == "Beta docs"
            # the page boundary: a walk of two pages of three loses nothing
            walked = [it["provider_session"] for off in (0, 3) for it in ask(pages=True, offset=off, limit=3)["items"]]
            assert walked == [it["provider_session"] for it in both["items"]]
            # the query matches a page's title and address; an account has no pages; a false flag is no flag
            assert [it["provider_session"] for it in ask(pages=True, query="beta")["items"]] == ["cl-a2", "https://docs.test/beta"]
            assert [it["provider_session"] for it in ask(pages=True, query="pages.test")["items"]] == ["https://pages.test/new?q=1", "https://pages.test/old"]
            assert [it["provider_session"] for it in ask(pages=True, account=acc)["items"]] == ["cl-a2", "cl-a1"]
            assert ask(pages=False)["total"] == 2
            # `pages` is for this machine's list only: the other scopes ignore it
            assert conn.call(op="session.chats", cwd=str(proj), pages=True)["total"] == 2
            # a damaged or group-readable file is an empty list of pages
            file.chmod(0o640)
            assert ask(pages=True)["total"] == 2
            file.chmod(0o600)
            file.write_text("{damaged")
            assert ask(pages=True)["total"] == 2
            assert conn.call(op="session.chats", all=True, pages="yes")["ok"] == 0
        finally:
            host.stop()
