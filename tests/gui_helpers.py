"""Shared non-test GUI helpers for menu and panel tests (kept separate from leaf test files)."""

from __future__ import annotations

import time

from lico_measure.ime import _focus
from lico_measure.rig import Client
from tests.test_codex_adapter import wait_for
from tests.test_first_view import click, ui


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


def reveal(read, press, matches):
    """M22-P04a: the menus are trees. `read()` returns the open, drawn dropdown (its `menu` state, every row with a box) or None; `press(rect)` clicks a box. Returns the visible entry whose label
    `matches` (a predicate); an entry in a closed group is shown first by a click on the group's heading."""
    def found():
        m = read()
        return m and next((i for i in m["items"] if i["kind"] == "entry" and matches(i["label"])), None)

    wait_for(read)  # the dropdown is open and drawn
    row = found()
    if row is not None:
        return row
    m = read()
    group = next(g["label"] for g in m["groups"] if any(matches(label) for label in g["entries"]))
    press(next(i for i in m["items"] if i["kind"] == "group" and i["label"] == group)["rect"])
    return wait_for(found)


def menu_reveal(w, index, matches):
    """`reveal` for a `ProductWindow` whose menu `index` is open."""
    def read():
        m = w.ui()["menu"]
        return m if m["open"] == index and m["items"] and all(b["rect"] for b in m["items"]) else None

    return reveal(read, lambda rect: w.click(*centre(rect)), matches)


def client_reveal(client, display, index, text):
    """`reveal` for a window that a `Client` drives (its `ui` states, xdotool clicks): the visible entry of the open menu `index` whose label contains `text`."""
    def read():
        m = ui(client)["menu"]
        return m if m["open"] == index and m["items"] and all(b["rect"] for b in m["items"]) else None

    return reveal(read, lambda rect: click_at(display, *centre(rect)), lambda label: text in label)


def menu_entry(w, index, prefix, keep_open=False):
    """M22-P04: the toolbar is three icons; what its text buttons did is in the menus. Opens menu `index` (File 0, Session 1, View 2, Help 3) of a `ProductWindow` and returns the entry whose
    label starts with `prefix` (in a group that is closed, after opening the group); the menu is closed again unless `keep_open`."""
    w.click(*centre(w.ui()["menu"]["titles"][index]["rect"]))
    entry = menu_reveal(w, index, lambda label: label.startswith(prefix))
    if not keep_open:
        w.key("Escape")
        wait_for(lambda: w.ui()["menu"]["open"] is None)
    return entry


def menu_click(w, index, prefix):
    """Opens menu `index` and clicks the entry that starts with `prefix`."""
    w.click(*centre(menu_entry(w, index, prefix, keep_open=True)["rect"]))


def open_form(w):
    """M22-P04b: Ctrl+N and the plus of the toolbar open the new-session wizard (three icons first); the one-page form, every row on one screen, is File > New session options...
    Returns its panel once it is open."""
    menu_click(w, 0, "New session options")
    return wait_for(lambda: (p := w.ui()["panel"]) and p["form"] is not None and p["form"]["wizard"] is None and p)


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
