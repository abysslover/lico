"""M17-P10: the PICTURES of the window while the browser engine installs itself (not a test of a rule; the file name has no `test_` so that no suite run collects it). It needs the host
binary built with the install seam (`LICO_SEAM_BIN`, what `cargo xtask system` builds), the fake engine (`LICO_FAKE_ENGINE_BIN`) and a folder for the pictures (`LICO_P10_SCREENS`):

    LICO_SEAM_BIN=... LICO_FAKE_ENGINE_BIN=... LICO_P10_SCREENS=... python -m pytest -q tests/probe_install_window.py -s

The release server is a fake on loopback (plain http, named in a seam file); the archive holds the test double of the engine. No real service."""

from __future__ import annotations

import hashlib
import http.server
import json
import os
import shutil
import tarfile
import tempfile
import threading
from pathlib import Path

from lico_measure.captures import ProductWindow
from tests.test_browser_tab import browser, open_tab, painted
from tests.test_codex_adapter import wait_for

SEAM_BIN = os.environ["LICO_SEAM_BIN"]
FAKE = os.environ["LICO_FAKE_ENGINE_BIN"]
SCREENS = Path(os.environ["LICO_P10_SCREENS"])


def release(work: Path, wrong_hash: bool):
    """A fake release server whose answer waits for `gate`; returns (seam file, gate)."""
    src = work / "src"
    src.mkdir()
    shutil.copy(FAKE, src / "obscura")
    (src / "obscura-worker").write_bytes(b"#!/bin/sh\n")
    for f in src.iterdir():
        f.chmod(0o755)
    archive = work / "a.tar.gz"
    with tarfile.open(archive, "w:gz") as t:
        t.add(src / "obscura", "obscura")
        t.add(src / "obscura-worker", "obscura-worker")
    digest = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
    gate = threading.Event()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            gate.wait()
            body = archive.read_bytes()
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *a):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    seam = work / "seam.json"
    seam.write_text(json.dumps({"url": f"http://127.0.0.1:{server.server_port}/a.tar.gz", "archive_sha256": "0" * 64 if wrong_hash else digest(archive), "engine_sha256": digest(src / "obscura"), "worker_sha256": digest(src / "obscura-worker"), "max_archive": 64 << 20}))
    return seam, gate


def run(name: str, wrong_hash: bool, finish):
    SCREENS.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix=f"lico_p10_{name}_"))
    seam, gate = release(work, wrong_hash)
    try:
        with ProductWindow(host_binary=SEAM_BIN, env_extra={"LICO_INSTALL_SEAM_FILE": str(seam)}) as w:
            tab = open_tab(w, "https://pages.test/start")
            finish(w, tab, gate)
    finally:
        gate.set()
        shutil.rmtree(work, ignore_errors=True)


def test_installing_then_ready_with_the_page_painted():
    def finish(w, tab, gate):
        b = wait_for(lambda: (b := browser(w)) and "installing the browser engine (about 72 MB from 127.0.0.1:" in (b["header"] or "") and b)
        assert "SHA-256 starts" in b["header"] and "?" not in b["header"].split("|")[-1], b["header"]
        w.png(SCREENS / "1_installing.png")
        gate.set()
        painted(w, tab)
        b = wait_for(lambda: (b := browser(w)) and "| ready" in (b["header"] or "") and b)
        assert "installing" not in b["header"], b["header"]
        w.png(SCREENS / "2_ready_painted.png")

    run("ok", False, finish)


def test_a_failed_install_says_why_as_a_security_notice_and_the_manual_way():
    def finish(w, tab, gate):
        gate.set()
        b = wait_for(lambda: (b := browser(w)) and "SECURITY: the engine could not be installed" in (b["header"] or "") and b)
        assert "by hand: download https://github.com/" in b["header"], b["header"]
        w.png(SCREENS / "3_failed_install_notice.png")

    run("bad", True, finish)
