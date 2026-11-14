"""A small window manager for tests (M14-P02): it manages the top-level windows of ONE private display, follows the part of the EWMH standard that decides whether a window starts
maximized, and can be told to behave the way a real window manager was seen to behave. It is a MODEL of such a window manager, not mutter: what it shows is what the standard (or
the stated behaviour) gives, never what the owner's GNOME does.

    python -m lico_measure.fakewm DISPLAY [--manage-delay SECONDS] [--ignore-messages] [--ignore-initial-state]

- It takes the window at its MapRequest, reads `_NET_WM_STATE` then (the way to ask for an initial state) and, after `--manage-delay`, maps it; an initial state with both
  `_NET_WM_STATE_MAXIMIZED_VERT` and `_HORZ` makes the window screen sized and keeps both atoms on the property (`--ignore-initial-state`: the property is not looked at).
- A `_NET_WM_STATE` client message (add, remove, toggle of the two maximized atoms) is honoured for a window it already manages and dropped for one it does not manage yet
  (`--ignore-messages`: dropped always, which is what the owner's mutter did with the message of `lico gui`: no maximized bit, size as requested).
- A configure request of an unmaximized window is granted as asked; one of a maximized window is not.
It prints `ready` when it holds the display and ends on SIGTERM."""

from __future__ import annotations

import argparse
import ctypes
import ctypes.util
import signal
import sys
import time

c_int, c_long, c_ulong, c_void_p = ctypes.c_int, ctypes.c_long, ctypes.c_ulong, ctypes.c_void_p

MAP_REQUEST, CONFIGURE_REQUEST, CLIENT_MESSAGE = 20, 23, 33
SUBSTRUCTURE_NOTIFY, SUBSTRUCTURE_REDIRECT = 1 << 19, 1 << 20
PROP_MODE_REPLACE, XA_ATOM = 0, 4


class MapRequest(ctypes.Structure):
    _fields_ = [("type", c_int), ("serial", c_ulong), ("send_event", c_int), ("display", c_void_p), ("parent", c_ulong), ("window", c_ulong)]


class ClientMessage(ctypes.Structure):
    _fields_ = [("type", c_int), ("serial", c_ulong), ("send_event", c_int), ("display", c_void_p), ("window", c_ulong), ("message_type", c_ulong), ("format", c_int), ("data", c_long * 5)]


class ConfigureRequest(ctypes.Structure):
    _fields_ = [("type", c_int), ("serial", c_ulong), ("send_event", c_int), ("display", c_void_p), ("parent", c_ulong), ("window", c_ulong), ("x", c_int), ("y", c_int),
                ("width", c_int), ("height", c_int), ("border_width", c_int), ("above", c_ulong), ("detail", c_int), ("value_mask", c_ulong)]


class Changes(ctypes.Structure):
    _fields_ = [("x", c_int), ("y", c_int), ("width", c_int), ("height", c_int), ("border_width", c_int), ("sibling", c_ulong), ("stack_mode", c_int)]


class Event(ctypes.Union):
    _fields_ = [("type", c_int), ("map", MapRequest), ("client", ClientMessage), ("configure", ConfigureRequest), ("pad", c_long * 24)]


def load():
    x = ctypes.CDLL(ctypes.util.find_library("X11"))
    x.XOpenDisplay.restype = c_void_p
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XDefaultRootWindow.restype = c_ulong
    x.XDefaultRootWindow.argtypes = [c_void_p]
    x.XInternAtom.restype = c_ulong
    x.XInternAtom.argtypes = [c_void_p, ctypes.c_char_p, c_int]
    x.XSelectInput.argtypes = [c_void_p, c_ulong, c_long]
    x.XPending.argtypes = [c_void_p]
    x.XNextEvent.argtypes = [c_void_p, ctypes.POINTER(Event)]
    x.XMapWindow.argtypes = [c_void_p, c_ulong]
    x.XMoveResizeWindow.argtypes = [c_void_p, c_ulong, c_int, c_int, ctypes.c_uint, ctypes.c_uint]
    x.XConfigureWindow.argtypes = [c_void_p, c_ulong, ctypes.c_uint, ctypes.POINTER(Changes)]
    x.XChangeProperty.argtypes = [c_void_p, c_ulong, c_ulong, c_ulong, c_int, c_int, ctypes.POINTER(c_ulong), c_int]
    x.XDeleteProperty.argtypes = [c_void_p, c_ulong, c_ulong]
    x.XGetWindowProperty.argtypes = [c_void_p, c_ulong, c_ulong, c_long, c_long, c_int, c_ulong, ctypes.POINTER(c_ulong), ctypes.POINTER(c_int), ctypes.POINTER(c_ulong),
                                     ctypes.POINTER(c_ulong), ctypes.POINTER(ctypes.POINTER(c_ulong))]
    x.XDisplayWidth.argtypes = x.XDisplayHeight.argtypes = [c_void_p, c_int]
    x.XFlush.argtypes = [c_void_p]
    x.XFree.argtypes = [c_void_p]
    handler = ctypes.CFUNCTYPE(c_int, c_void_p, c_void_p)(lambda _d, _e: 0)  # a window that went away is no error of ours
    x.XSetErrorHandler(handler)
    return x, handler


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("display")
    parser.add_argument("--manage-delay", type=float, default=0.0)
    parser.add_argument("--ignore-messages", action="store_true")
    parser.add_argument("--ignore-initial-state", action="store_true")
    args = parser.parse_args()
    x, _keep = load()
    d = x.XOpenDisplay(args.display.encode())
    if not d:
        print("no display", file=sys.stderr)
        return 2
    root = x.XDefaultRootWindow(d)
    screen_w, screen_h = x.XDisplayWidth(d, 0), x.XDisplayHeight(d, 0)
    atom = lambda name: x.XInternAtom(d, name.encode(), 0)  # noqa: E731
    state, vert, horz = atom("_NET_WM_STATE"), atom("_NET_WM_STATE_MAXIMIZED_VERT"), atom("_NET_WM_STATE_MAXIMIZED_HORZ")
    x.XSelectInput(d, root, SUBSTRUCTURE_REDIRECT | SUBSTRUCTURE_NOTIFY)
    x.XFlush(d)
    stop = False

    def on_term(*_):
        nonlocal stop
        stop = True

    signal.signal(signal.SIGTERM, on_term)

    def initial_maximized(window: int) -> bool:
        actual, fmt, n, after, data = c_ulong(), c_int(), c_ulong(), c_ulong(), ctypes.POINTER(c_ulong)()
        x.XGetWindowProperty(d, window, state, 0, 32, 0, XA_ATOM, ctypes.byref(actual), ctypes.byref(fmt), ctypes.byref(n), ctypes.byref(after), ctypes.byref(data))
        atoms = {data[i] for i in range(n.value)} if data and fmt.value == 32 else set()
        if data:
            x.XFree(data)
        return vert in atoms and horz in atoms

    maximized: set[int] = set()
    saved: dict[int, tuple[int, int]] = {}
    managed: set[int] = set()
    waiting: list[tuple[float, int, bool]] = []  # (due, window, asked for maximized at the map)

    def maximize(window: int, on: bool) -> None:
        if on and window not in maximized:
            maximized.add(window)
            x.XMoveResizeWindow(d, window, 0, 0, screen_w, screen_h)
            values = (c_ulong * 2)(vert, horz)
            x.XChangeProperty(d, window, state, XA_ATOM, 32, PROP_MODE_REPLACE, values, 2)
        elif not on and window in maximized:
            maximized.discard(window)
            w, h = saved.get(window, (1200, 800))
            x.XMoveResizeWindow(d, window, 0, 0, w, h)  # back where the tests expect it: the display origin
            x.XDeleteProperty(d, window, state)

    print("ready", flush=True)
    ev = Event()
    while not stop:
        now = time.monotonic()
        for item in [i for i in waiting if i[0] <= now]:
            waiting.remove(item)
            _due, window, wants = item
            managed.add(window)
            if wants and not args.ignore_initial_state:
                maximize(window, True)
            x.XMapWindow(d, window)
            x.XFlush(d)
        if not x.XPending(d):
            time.sleep(0.005)
            continue
        x.XNextEvent(d, ctypes.byref(ev))
        if ev.type == MAP_REQUEST:
            window = ev.map.window
            waiting.append((time.monotonic() + args.manage_delay, window, initial_maximized(window)))
        elif ev.type == CONFIGURE_REQUEST:
            r = ev.configure
            if r.window in maximized:
                continue
            saved[r.window] = (r.width, r.height)
            changes = Changes(r.x, r.y, r.width, r.height, r.border_width, r.above, r.detail)
            x.XConfigureWindow(d, r.window, r.value_mask & 0x7F, ctypes.byref(changes))
        elif ev.type == CLIENT_MESSAGE and ev.client.message_type == state:
            if args.ignore_messages or ev.client.window not in managed:
                continue
            op, first, second = ev.client.data[0], ev.client.data[1], ev.client.data[2]
            if vert in (first, second) or horz in (first, second):
                maximize(ev.client.window, op == 1 or (op == 2 and ev.client.window not in maximized))
        x.XFlush(d)
    return 0


if __name__ == "__main__":
    sys.exit(main())
