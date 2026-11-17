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


class Fixture(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(PAGE)))
        self.end_headers()
        self.wfile.write(PAGE)

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
        with ProductWindow(host_binary=LICO, before_window=pinned_engine, env_extra={"WINIT_X11_SCALE_FACTOR": scale}) as w:
            if resize_to:
                subprocess.run(["xdotool", "windowsize", w.window_id(), str(resize_to[0]), str(resize_to[1])], env=w.env, check=True)
                time.sleep(1.0)
            time.sleep(1.0)
            pid = w.proc.pid
            idle = rss_mib(pid)
            tab = open_tab(w, f"http://localhost:{port}/", local=True)
            t = painted(w, tab)
            time.sleep(2.0)
            b = browser(w)
            out.append(f"{tag}: scale {scale}, window area {b['rect'][2]}x{b['rect'][3]} px, frame {b['tabs'][0]['png_bytes']} bytes compressed; window RSS idle {idle:.1f} MiB, with one shown tab {rss_mib(pid):.1f} MiB (peak {hwm_mib(pid):.1f}); title {b['header']!r}")
            shot(w, f"7_real_korean_page_{tag}.png")
    finally:
        captures.Xvfb = real_xvfb
        server.shutdown()


def test_probe():
    out: list[str] = []
    measure("1", "1200x800x24", None, "default_scale1", out)
    measure("2", "1200x800x24", None, "default_scale2", out)
    measure("1", "1920x1080x24", (1920, 1080), "1920x1080_scale1", out)
    measure("2", "1920x1080x24", (1920, 1080), "1920x1080_scale2", out)
    Path(os.environ["LICO_PROBE_OUT"], "p07_window_probe.txt").write_text("\n".join(out) + "\n")
    print("\n".join(out))
