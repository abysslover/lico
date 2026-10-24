"""Read pixels from an X server's root window through libX11 (ctypes).

This observes what the server shows, independently of any client's own report.
"""

from __future__ import annotations

import ctypes
import ctypes.util

_X11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
_X11.XOpenDisplay.restype = ctypes.c_void_p
_X11.XOpenDisplay.argtypes = [ctypes.c_char_p]
_X11.XDefaultRootWindow.restype = ctypes.c_ulong
_X11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
_X11.XGetImage.restype = ctypes.c_void_p
_X11.XGetImage.argtypes = [
    ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
    ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_int,
]
_X11.XGetPixel.restype = ctypes.c_ulong
_X11.XGetPixel.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
_X11.XDestroyImage.argtypes = [ctypes.c_void_p]
_X11.XCloseDisplay.argtypes = [ctypes.c_void_p]
_X11.XInternAtom.restype = ctypes.c_ulong
_X11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
_X11.XGetSelectionOwner.restype = ctypes.c_ulong
_X11.XGetSelectionOwner.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
_X11.XCreateSimpleWindow.restype = ctypes.c_ulong
_X11.XCreateSimpleWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint,
                                     ctypes.c_uint, ctypes.c_ulong, ctypes.c_ulong]
_X11.XDestroyWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
_X11.XConvertSelection.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
_X11.XFlush.argtypes = [ctypes.c_void_p]
_X11.XPending.argtypes = [ctypes.c_void_p]
_X11.XNextEvent.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
_X11.XGetWindowProperty.argtypes = [
    ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long, ctypes.c_long, ctypes.c_int, ctypes.c_ulong,
    ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong),
    ctypes.POINTER(ctypes.POINTER(ctypes.c_ubyte)),
]
_X11.XFree.argtypes = [ctypes.c_void_p]
_SELECTION_NOTIFY = 31
_ALL_PLANES = 0xFFFFFFFFFFFFFFFF
_ZPIXMAP = 2


class PixelReader:
    def __init__(self, display: str) -> None:
        self._display = _X11.XOpenDisplay(display.encode())
        if not self._display:
            raise RuntimeError(f"cannot open display {display}")
        self._root = _X11.XDefaultRootWindow(self._display)

    def pixel(self, x: int, y: int) -> int:
        """The 0xRRGGBB value currently shown at (x, y) on the root window."""
        image = _X11.XGetImage(self._display, self._root, x, y, 1, 1, _ALL_PLANES, _ZPIXMAP)
        if not image:
            raise RuntimeError("XGetImage failed")
        try:
            return int(_X11.XGetPixel(image, 0, 0)) & 0xFFFFFF
        finally:
            _X11.XDestroyImage(image)

    def selection_owner(self, name: str) -> int:
        """The window that owns a selection such as CLIPBOARD or PRIMARY (0: nobody)."""
        return int(_X11.XGetSelectionOwner(self._display, _X11.XInternAtom(self._display, name.encode(), 0)))

    def selection_text(self, name: str, timeout: float = 3.0) -> str | None:
        """The text a selection owner hands out as UTF8_STRING, asked for the way any client asks (None: nobody answers)."""
        import time

        display = self._display
        window = _X11.XCreateSimpleWindow(display, self._root, 0, 0, 1, 1, 0, 0, 0)
        selection = _X11.XInternAtom(display, name.encode(), 0)
        target = _X11.XInternAtom(display, b"UTF8_STRING", 0)
        prop = _X11.XInternAtom(display, b"LICO_SELECTION_PROBE", 0)
        try:
            _X11.XConvertSelection(display, selection, target, prop, window, 0)
            _X11.XFlush(display)
            event = ctypes.create_string_buffer(256)
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if not _X11.XPending(display):
                    time.sleep(0.02)
                    continue
                _X11.XNextEvent(display, event)
                if ctypes.c_int.from_buffer(event).value != _SELECTION_NOTIFY:
                    continue
                # XSelectionEvent: type, serial, send_event, display, requestor, selection, target, property, time
                answered = ctypes.c_ulong.from_buffer(event, 56).value
                if not answered:
                    return None
                actual, fmt, count, rest = ctypes.c_ulong(), ctypes.c_int(), ctypes.c_ulong(), ctypes.c_ulong()
                data = ctypes.POINTER(ctypes.c_ubyte)()
                _X11.XGetWindowProperty(display, window, prop, 0, 1 << 20, 1, 0, ctypes.byref(actual), ctypes.byref(fmt), ctypes.byref(count),
                                        ctypes.byref(rest), ctypes.byref(data))
                if not data:
                    return None
                try:
                    return bytes(data[: count.value * (fmt.value // 8)]).decode("utf-8", errors="replace")
                finally:
                    _X11.XFree(data)
            return None
        finally:
            _X11.XDestroyWindow(display, window)

    def close(self) -> None:
        if self._display:
            _X11.XCloseDisplay(self._display)
            self._display = None
