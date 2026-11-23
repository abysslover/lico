"""M24-P03: the account screen as pictures in the real window. The list is a card for each account (the provider's picture, its label, a star on the default one) and a card with a plus for a
new one; an account's page is five pictures (a key, a pencil, a star, a bin and a bin with a cross), each doing what its key did (`S`, `R`, `F`, `D`, `E`). The lines of text stay under the
pictures (the keys and the dump read them). With `LICO_M24_SCREENS` set the test also saves the screens as pictures there, for the owner's look."""

from __future__ import annotations

import os
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import centre
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_icon_toolbar_wizard import _button, _panel, _ready, _rig

SCREENS = Path(os.environ["LICO_M24_SCREENS"]) if os.environ.get("LICO_M24_SCREENS") else None


def _shot(w: ProductWindow, name: str) -> None:
    if SCREENS:
        SCREENS.mkdir(parents=True, exist_ok=True)
        w.png(SCREENS / name)


def _cards(w: ProductWindow, count: int) -> dict:
    """The panel once the list has painted `count` cards, each with its rectangle."""
    return _panel(w, lambda p: len(p["acct"]["cards"]) == count and all(c["rect"] for c in p["acct"]["cards"]))


def _page(w: ProductWindow) -> dict:
    """The panel once an account's page has painted its five pictures."""
    return _panel(w, lambda p: len(p["acct"]["buttons"]) == 5 and all(b["rect"] for b in p["acct"]["buttons"]))


def _accounts(conn: Conn) -> dict:
    return {a["label"]: a for a in conn.call(op="account.list")["accounts"]}


def test_the_list_is_a_card_for_each_account_and_a_plus_and_a_click_on_a_card_opens_its_page():
    with usual_umask(), private_dir() as directory:
        kwargs, made, _received = _rig(Path(directory), [("Work", "claude"), ("Home", "codex")])
        with ProductWindow(**kwargs) as w:
            conn = Conn(w.socket)
            _ready(w)
            w.click(*centre(_button(w, "accounts")["rect"]))
            panel = _cards(w, 3)
            cards = panel["acct"]["cards"]
            assert [c["icon"] for c in cards] == ["claude", "codex", "plus"], cards
            assert [c["plus"] for c in cards] == [False, False, True]
            assert "Work" in cards[0]["text"] and "Home" in cards[1]["text"], cards
            assert any("Work" in line for line in panel["body"]), "the text stays under the pictures: the keys and the dump read it"
            _shot(w, "1_account_list.png")
            w.click(*centre(cards[1]["rect"]))                                   # the card of Home
            page = _page(w)
            assert page["title"].startswith("Account:") and "Home" in page["title"], page["title"]
            assert [b["icon"] for b in page["acct"]["buttons"]] == ["key", "pencil", "star", "trash", "trash_x"]
            assert [b["key"] for b in page["acct"]["buttons"]] == ["s", "r", "f", "d", "e"]
            _shot(w, "2_account_page.png")
            assert page["acct"]["cards"] == [], "the page has no cards"
            assert _accounts(conn)["Home"]["default"] is True, "a provider's only account is its default"


def test_the_star_makes_the_account_the_default_and_the_bin_asks_twice_before_it_removes():
    with usual_umask(), private_dir() as directory:
        kwargs, made, _received = _rig(Path(directory), [("Work", "claude"), ("Other", "claude")])
        with ProductWindow(**kwargs) as w:
            conn = Conn(w.socket)
            _ready(w)
            w.click(*centre(_button(w, "accounts")["rect"]))
            cards = _cards(w, 3)["acct"]["cards"]
            assert [c["default"] for c in cards[:2]] == [True, False], "the first account of a provider is its default"
            w.click(*centre(cards[1]["rect"]))                                   # Other
            page = _page(w)
            w.click(*centre(page["acct"]["buttons"][2]["rect"]))                 # the star
            wait_for(lambda: _accounts(conn)["Other"]["default"] is True)
            assert _accounts(conn)["Work"]["default"] is False
            w.click(*centre(_page(w)["acct"]["buttons"][3]["rect"]))             # the bin: asks
            asked = _panel(w, lambda p: any(b["pending"] for b in p["acct"]["buttons"]))
            assert [b["pending"] for b in asked["acct"]["buttons"]] == [False, False, False, True, False]
            _shot(w, "3_remove_asks.png")
            assert "Other" in _accounts(conn), "one click removes nothing"
            w.click(*centre(asked["acct"]["buttons"][3]["rect"]))                # the bin again: removes
            wait_for(lambda: "Other" not in _accounts(conn))
            assert "Work" in _accounts(conn)


def test_the_plus_card_adds_an_account_and_a_screen_with_no_account_has_only_the_plus():
    with usual_umask(), private_dir() as directory:
        kwargs, _made, _received = _rig(Path(directory), [])
        with ProductWindow(**kwargs) as w:
            _ready(w)
            w.click(*centre(_button(w, "accounts")["rect"]))
            cards = _cards(w, 1)["acct"]["cards"]
            assert cards[0]["plus"] is True and cards[0]["icon"] == "plus", cards
            _shot(w, "4_no_account.png")
            w.click(*centre(cards[0]["rect"]))
            panel = _panel(w, lambda p: p["acct"]["cards"] == [] and p["title"] != "Accounts")
            assert panel["text_entry"] is True or panel["title"] != "Accounts", panel["title"]
