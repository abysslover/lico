"""M17-P07: the MEASURED probe of the window with the REAL engine (not a test of a rule; the file name has no `test_` so that no suite run collects it). It needs the two files of the
pinned release extracted in `LICO_REAL_ENGINE_DIR` (the host checks their hash), a private output directory `LICO_PROBE_OUT`, and optionally `LICO_P07_SCREENS` for the pictures:

    LICO_REAL_ENGINE_DIR=... LICO_PROBE_OUT=... LICO_P07_SCREENS=... python -m pytest -q tests/probe_browser_window.py -s

The only origin is a fixture server on loopback, reached through the host's fetch guard by a tab opened for this machine's own services: no real site, no real name lookup."""

from __future__ import annotations

import http.server
import json
import os
import shutil
import subprocess
import threading
import time
from pathlib import Path

import lico_measure.captures as captures
from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO
from tests.test_browser_tab import browser, centre, open_tab, painted, shot
from tests.test_codex_adapter import wait_for

PAGE = "<html><head><meta charset='utf-8'><title>한글 시험 페이지</title></head><body style='font-size:28px'><h1>안녕하세요 Lico</h1><p>브라우저 탭의 한글 글꼴 시험입니다.</p><p>The page is a fixture of the test.</p></body></html>".encode()


INPUT_PAGE = "<html><head><meta charset='utf-8'><title>input</title></head><body style='margin:0;font-size:24px'><input id='f' style='position:absolute;left:10px;top:10px;width:420px;height:44px;font-size:28px;font-family:NanumGothic,sans-serif'><script>document.getElementById('f').addEventListener('input',function(e){fetch('/v?x='+encodeURIComponent(e.target.value))});</script></body></html>".encode()
SEEN: list[str] = []


class Fixture(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        SEEN.append(self.path)
        body = INPUT_PAGE if self.path.startswith("/input") else PAGE
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def rss_mib(pid: int) -> float:
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmRSS:"):
            return int(line.split()[1]) / 1024.0
    return 0.0


def hwm_mib(pid: int) -> float:
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmHWM:"):
            return int(line.split()[1]) / 1024.0
    return 0.0


def pinned_engine(w: ProductWindow) -> None:
    src = Path(os.environ["LICO_REAL_ENGINE_DIR"])
    dest = Path(w.directory) / "home" / ".local" / "share" / "lico" / "browser" / "v0.2.6"
    at = Path(w.directory) / "home"
    for part in dest.relative_to(at).parts:
        at = at / part
        at.mkdir(mode=0o700, exist_ok=True)
    for name in ("obscura", "obscura-worker"):
        shutil.copy(src / name, dest / name)
        (dest / name).chmod(0o700)


def measure(scale: str, geometry: str, resize_to: tuple[int, int] | None, tag: str, out: list[str]) -> None:
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_address[1]
    real_xvfb = captures.Xvfb
    captures.Xvfb = lambda: real_xvfb(geometry)
    try:
        with ProductWindow(host_binary=LICO, before_window=pinned_engine, env_extra={} if scale == "1" else {"WINIT_X11_SCALE_FACTOR": scale}) as w:
            if resize_to:
                subprocess.run(["xdotool", "windowsize", w.window_id(), str(resize_to[0]), str(resize_to[1])], env=w.env, check=True)
                time.sleep(1.0)
            time.sleep(1.0)
            pid = w.proc.pid
            idle = rss_mib(pid)
            try:
                tab = open_tab(w, f"http://localhost:{port}/", local=True)
            except AssertionError:
                u = w.ui()
                raise AssertionError((u['header'], u['input'], u['browser']['header'], u['tabs']['new_browser_local_rect']))
            end = time.time() + 25
            while time.time() < end and not any(x['decoded'] for x in browser(w)['tabs']):
                time.sleep(0.2)
            assert any(x['decoded'] for x in browser(w)['tabs']), json.dumps(browser(w))[:1500]
            time.sleep(2.0)
            b = browser(w)
            out.append(f"{tag}: scale {scale}, window area {b['rect'][2]}x{b['rect'][3]} px, frame {b['tabs'][0]['png_bytes']} bytes compressed; window RSS idle {idle:.1f} MiB, with one shown tab {rss_mib(pid):.1f} MiB (peak {hwm_mib(pid):.1f}); title {b['header']!r}")
            shot(w, f"7_real_korean_page_{tag}.png")
    finally:
        captures.Xvfb = real_xvfb
        server.shutdown()


def test_probe():
    out: list[str] = []
    note = Path(os.environ["LICO_PROBE_OUT"], "p07_window_probe.txt")
    for args in [("1", "1200x800x24", None, "default_scale1"), ("2", "2400x1600x24", None, "default_scale2"), ("1", "1920x1080x24", (1920, 1080), "1920x1080_scale1"), ("2", "3840x2160x24", (3840, 2160), "3840x2160_scale2_capped")]:
        measure(*args, out)
        note.write_text("\n".join(out) + "\n")
        print(out[-1])


def test_typing_korean_into_a_real_page():
    """The person clicks the page's input and composes Korean with the input method: the page receives the committed text (its own input events are seen by the fixture)."""
    import urllib.parse

    from lico_measure.ime import SENTENCE_KEYS, SENTENCE_TEXT

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_address[1]
    SEEN.clear()
    out = []
    try:
        with ProductWindow(host_binary=LICO, before_window=pinned_engine, ime=True) as w:
            tab = open_tab(w, f"http://localhost:{port}/input", local=True)
            end = time.time() + 25
            while time.time() < end and not any(x["decoded"] for x in browser(w)["tabs"]):
                time.sleep(0.2)
            time.sleep(1.0)
            x, y, _, _ = browser(w)["rect"]
            w.click(x + 60, y + 30)                                  # the page's input
            time.sleep(0.5)
            w.key("shift+space")
            time.sleep(0.3)
            w.key("--delay", "60", *SENTENCE_KEYS)
            time.sleep(2.0)
            values = [urllib.parse.unquote(p.split("x=", 1)[1]) for p in SEEN if p.startswith("/v?x=")]
            out.append(f"Korean typed into the page's input with the input method: the page's input events {len(values)}, the last value {values[-1] if values else None!r} (the sentence is {SENTENCE_TEXT!r})")
            shot(w, "9_real_page_korean_typed.png")
    finally:
        server.shutdown()
    Path(os.environ["LICO_PROBE_OUT"], "p08_window_typing.txt").write_text("\n".join(out) + "\n")
    print(out[0])
    assert values and values[-1].strip() == SENTENCE_TEXT.strip(), values[-3:]
