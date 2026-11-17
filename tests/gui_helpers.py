"""Shared non-test GUI helpers for menu and panel tests (kept separate from leaf test files)."""

from __future__ import annotations

import time

from lico_measure.ime import _focus
from lico_measure.rig import Client
from tests.test_codex_adapter import wait_for
from tests.test_first_view import click


def centre(rect):
    return (rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2


def item(m, text):
    return next(i for i in m["items"] if text in i["label"])


def drawn(u, which):
    m = u["menu"]
    return m["open"] == which and bool(m["items"]) and all(b["rect"] for b in m["titles"] + m["items"])


def click_at(display, x, y):
    click(display, x, y)
    time.sleep(0.35)


def menu_client(host, display, extra_args=None):
    args = ["--ui", "--menu"] + (list(extra_args) if extra_args else [])
    client = Client("a2", host.socket, display, extra_args=args, extra_env={"LICO_NO_AUTO_OPEN": "1"})
    time.sleep(0.6)
    _focus(display)
    time.sleep(0.2)
    return client


def menu_entry(w, index, prefix, keep_open=False):
    """M22-P04: the toolbar is three icons; what its text buttons did is in the menus. Opens menu `index` (File 0, Session 1, View 2, Help 3) of a `ProductWindow` and returns the entry whose
    label starts with `prefix`; the menu is closed again unless `keep_open`."""
    w.click(*centre(w.ui()["menu"]["titles"][index]["rect"]))
    items = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == index and m["items"] and m["items"])
    entry = next(i for i in items if i["label"].startswith(prefix))
    if not keep_open:
        w.key("Escape")
        wait_for(lambda: w.ui()["menu"]["open"] is None)
    return entry


def menu_click(w, index, prefix):
    """Opens menu `index` and clicks the entry that starts with `prefix`."""
    w.click(*centre(menu_entry(w, index, prefix, keep_open=True)["rect"]))


def menu_click_when_enabled(w, index, prefix, timeout=10.0):
    """Clicks the menu entry that starts with `prefix` as soon as it can act (a greyed entry only says why); raises when it never can."""
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        entry = menu_entry(w, index, prefix, keep_open=True)
        if entry["enabled"]:
            w.click(*centre(entry["rect"]))
            return entry
        w.key("Escape")
        wait_for(lambda: w.ui()["menu"]["open"] is None)
        time.sleep(0.1)
    raise AssertionError(f"the menu entry {prefix!r} never became enabled")
