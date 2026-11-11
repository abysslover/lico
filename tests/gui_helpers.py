"""Shared non-test GUI helpers for menu and panel tests (kept separate from leaf test files)."""

from __future__ import annotations

import time

from lico_measure.ime import _focus
from lico_measure.rig import Client
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
