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

    def close(self) -> None:
        if self._display:
            _X11.XCloseDisplay(self._display)
            self._display = None
