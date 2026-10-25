"""Scenarios for the toolkit experiments that the P01 rig does not cover: editing keys, clipboard, menu keyboard use,
Korean composition in a menu. They reuse the rig (private X server, host, client, private ibus) unchanged.

Every scenario runs on its own private X server; nothing here touches another display or another host.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import toolkits  # noqa: E402

toolkits.register()

from lico_measure import ime, rig  # noqa: E402
from lico_measure.xpixel import PixelReader  # noqa: E402

# Pixel positions (physical, scale 1) of the clickable things, calibrated once from screenshots of each candidate
# (see data/calibration/): menu titles, the item or field inside the opened menu, the entry line and the output area.
LAYOUT = {
    "egui": {"File": (57, 11), "Session": (97, 11), "Help": (160, 11), "entry": (700, 790), "output": (700, 300)},
    "iced": {"File": (55, 16), "Session": (116, 16), "Help": (181, 16), "entry": (700, 785), "output": (700, 300)},
    "slint": {"File": (20, 18), "Session": (75, 18), "Help": (130, 18), "entry": (700, 785), "output": (700, 300)},
}
LAYOUT_FILE = HERE / "data" / "layout.json"


def layout(candidate: str) -> dict:
    import json

    if LAYOUT_FILE.exists():
        return json.loads(LAYOUT_FILE.read_text())[candidate]
    return LAYOUT[candidate]


class Scene:
    """Private display + host + (optionally) a private ibus session + one candidate client."""

    def __init__(self, candidate: str, with_ime: bool = False, sessions: int = 3, producing: int = 1) -> None:
        self.candidate = candidate
        self.with_ime = with_ime
        self.sessions, self.producing = sessions, producing
        self.events: list[dict] = []

    def __enter__(self) -> "Scene":
        self.tmp = rig.private_dir()
        self.dir = self.tmp.name
        self.xvfb = rig.Xvfb()
        self.host = self.method = self.client = self.reader = None
        try:
            self.host = rig.Host(self.dir, sessions=self.sessions, producing=self.producing, history=500)
            if self.with_ime:
                self.method = ime.InputMethod(self.xvfb.display)
            extra = self.method.client_env if self.method else None
            self.client = rig.Client(self.candidate, self.host.socket, self.xvfb.display, extra_env=extra)
            self.reader = PixelReader(self.xvfb.display)
            sid = self.host.sessions[0]
            self.client.send(f"show {sid}")
            self.client.expect("state", "subscribed")
            time.sleep(1.0)
            self.env = dict(os.environ, DISPLAY=self.xvfb.display)
            ime._focus(self.xvfb.display)
            time.sleep(0.5)
        except BaseException:
            self.__exit__()
            raise
        return self

    def __exit__(self, *exc) -> None:
        if self.reader:
            self.reader.close()
        if self.client:
            self.client.kill()
        if self.method:
            self.method.stop()
        if self.host:
            self.host.stop()
        self.xvfb.stop()
        self.tmp.cleanup()

    def xdo(self, *args: str) -> None:
        subprocess.run(["xdotool", *args], env=self.env, check=True)

    def key(self, *keys: str, delay: int = 60) -> None:
        self.xdo("key", "--delay", str(delay), *keys)
        time.sleep(0.25)

    def click(self, x: int, y: int) -> None:
        self.xdo("mousemove", str(x), str(y), "click", "1")
        time.sleep(0.4)

    def drain(self) -> list[dict]:
        out = []
        while not self.client.lines.empty():
            out.append(self.client.lines.get_nowait())
        self.events += out
        return out

    def text(self) -> str:
        self.drain()
        self.client.send("text")
        time.sleep(0.4)
        texts = [e["text"] for e in self.drain() if e.get("ev") == "ime" and e.get("kind") == "text"]
        return texts[-1] if texts else ""

    def notes(self) -> list[tuple[str, str]]:
        return [(e["kind"], e["text"]) for e in self.drain() if e.get("ev") == "note"]

    def clipboard(self) -> str | None:
        return self.reader.selection_text("CLIPBOARD")

    def shot(self, path: str) -> None:
        subprocess.run(["import", "-display", self.xvfb.display, "-window", "root", path], check=False)


def free_controls(candidate: str) -> dict:
    """Editing keys, selection and clipboard in the entry line, and selection in the output area (no input method)."""
    lay = layout(candidate)
    result: dict = {}
    with Scene(candidate) as scene:
        scene.click(*lay["entry"])
        scene.xdo("type", "--delay", "40", "hello world")
        time.sleep(0.3)
        result["type"] = scene.text()
        scene.key("Home", "Delete")
        result["home_delete"] = scene.text()                 # expect "ello world"
        scene.key("End", "BackSpace")
        result["end_backspace"] = scene.text()               # expect "ello worl"
        scene.key("shift+Home", "ctrl+c")
        result["clipboard_after_copy"] = scene.clipboard()   # expect "ello worl"
        scene.key("End", "ctrl+v")
        result["after_paste"] = scene.text()                 # expect "ello worlello worl"
        scene.key("Left", "Left", "shift+Right", "shift+Right", "BackSpace")
        result["shift_arrows_delete"] = scene.text()         # two characters selected and removed
        scene.key("ctrl+a", "BackSpace")
        result["select_all_delete"] = scene.text()           # expect ""
        scene.xdo("type", "--delay", "40", "abc")
        scene.key("ctrl+z")
        result["undo"] = scene.text()                        # informational: undo is not required of a toolkit
        # output area: drag over some text, copy, read the clipboard
        scene.xdo("mousemove", "300", "200", "mousedown", "1", "mousemove", "700", "200", "mouseup", "1")
        time.sleep(0.4)
        scene.key("ctrl+c")
        result["output_selection_copy"] = scene.clipboard()
        result["primary_selection"] = scene.reader.selection_text("PRIMARY")
    return result


def menu_keyboard(candidate: str) -> dict:
    """Which keyboard routes open a menu and activate an item (observed as menu notes)."""
    result: dict = {}
    for label, keys in (("alt+f, Down, Return", ["alt+f", "Down", "Return"]), ("F10, Down, Return", ["F10", "Down", "Return"]),
                        ("alt+f, Return", ["alt+f", "Return"]), ("Tab x3, Return", ["Tab", "Tab", "Tab", "Return"])):
        with Scene(candidate) as scene:
            scene.notes()
            scene.key(*keys, delay=200)
            time.sleep(0.4)
            result[label] = scene.notes()
    with Scene(candidate) as scene:
        lay = layout(candidate)
        scene.notes()
        scene.click(*lay["File"])
        scene.shot(str(HERE / "data" / "calibration" / f"{candidate}_file_menu.png"))
        result["mouse File opened (screenshot saved)"] = True
    return result


def korean_in_menu(candidate: str, delay_ms: int = 80) -> dict:
    """Type the Korean sentence into the text field of a menu popup, through the private ibus."""
    lay = layout(candidate)
    with Scene(candidate, with_ime=True) as scene:
        scene.notes()
        for step in lay["menu_find_clicks"]:
            scene.click(*step)
        scene.shot(str(HERE / "data" / "calibration" / f"{candidate}_find_open.png"))
        scene.xdo("key", "shift+space")
        time.sleep(0.5)
        scene.drain()
        scene.xdo("key", "--delay", str(delay_ms), *ime.SENTENCE_KEYS)
        time.sleep(1.0)
        notes = scene.notes()
        finds = [t for k, t in notes if k == "find"]
        events = [e for e in scene.events if e.get("ev") == "ime"]
        scene.shot(str(HERE / "data" / "calibration" / f"{candidate}_find_typed.png"))
        return {"final_find_text": finds[-1] if finds else None, "expected": ime.SENTENCE_TEXT,
                "exact": bool(finds) and finds[-1] == ime.SENTENCE_TEXT,
                "find_note_count": len(finds), "ime_event_kinds": sorted({e["kind"] for e in events})}
