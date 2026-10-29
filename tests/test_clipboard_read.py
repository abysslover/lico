"""`lico clipboard read` and the hidden job `clipboard_read` through the real executable (class L3, a small private display, no host). Every scenario runs
through the command and through the job and the two must answer alike. The other client is `xclip`, a real owner of a selection (text, a PNG, a PNG
over the maximum request size, which it serves by the incremental protocol; `clipboard::tests_x` shows the chunk count of that transfer), and a
stalled owner is made by stopping an `xclip` with SIGSTOP: it holds the selection and never answers. The protocol cases that need an owner of our own
(both text and picture offered, an owner that refuses TARGETS, a transfer that is slow as a whole) are in the Rust tests."""
import json
import os
import signal
import stat
import struct
import subprocess
import time
import zlib
from pathlib import Path

import pytest

from lico_measure.p02 import HOSTD_DIR
from lico_measure.rig import REPO, Xvfb, private_dir
from tests.test_capture_cli import im, read_png

EXE = HOSTD_DIR / "lico"
ROUTES = {"cmd": ["clipboard", "read"], "job": ["xjob", "clipboard_read"]}


@pytest.fixture(scope="module")
def display():
    server = Xvfb("400x240x24")
    yield server.display
    server.stop()


@pytest.fixture()
def owners(display):
    """Starts owners of a selection with `xclip` and ends every one of them (by its exact pid) when the test is over."""
    made = []

    def own(data, selection="clipboard", target=None):
        for old in [o for o in made if o[1] == selection]:       # the previous owner of this selection goes first, so nobody else answers
            old[0].kill()
            old[0].wait()
            made.remove(old)
        with private_dir() as d:
            file = Path(d) / "data"
            file.write_bytes(data)
            cmd = ["xclip", "-quiet", "-selection", selection] + (["-t", target] if target else []) + ["-i", str(file)]
            proc = subprocess.Popen(cmd, env={"DISPLAY": display, "PATH": "/usr/bin:/bin"}, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            made.append((proc, selection))
            # ready when it answers a request for its targets (xclip has read its file by then)
            for _ in range(200):
                probe = subprocess.run(["xclip", "-o", "-selection", selection, "-t", "TARGETS"], env={"DISPLAY": display, "PATH": "/usr/bin:/bin"}, capture_output=True)
                if probe.returncode == 0:
                    return proc
                time.sleep(0.02)
        raise AssertionError("xclip did not take the selection")

    yield own
    for proc, _ in made:
        proc.kill()
        proc.wait()


def lico(route, *args, display=None, cwd=None, umask=None, timeout=60):
    # no host socket anywhere in the environment and nothing on standard input: the command needs neither
    env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": "/CANARY-no-home"}
    if display:
        env["DISPLAY"] = display
    done = subprocess.run([str(EXE), *ROUTES[route], *args], env=env, capture_output=True, text=True, timeout=timeout, cwd=cwd, stdin=subprocess.DEVNULL,
                          preexec_fn=(lambda: os.umask(umask)) if umask is not None else None)
    lines = done.stdout.splitlines()
    assert len(lines) == 1, (done.stdout, done.stderr)          # one JSON line, always
    return done.returncode, json.loads(lines[0])


def both(*args, display=None, **kw):
    """The same arguments through the command and through the job: the answers must be equal; returns the answer (the command's)."""
    a, b = lico("cmd", *args, display=display, **kw), lico("job", *args, display=display, **kw)
    assert a == b, (a, b)
    return a


def make_png(w, h, noise=False):
    """A real PNG (zlib level 1, filter None) and its pixels."""
    rows = [bytes((x * 7 + y * 3 + c * 50) & 255 for x in range(w) for c in range(3)) if not noise else os.urandom(w * 3) for y in range(h)]
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"".join(b"\0" + r for r in rows), 1)) + chunk(b"IEND", b""))
    return data, b"".join(rows)


def forged_head(w, h):
    """33 bytes: the signature and an IHDR chunk with any size (nothing is decoded by the reader)."""
    body = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + body + struct.pack(">I", zlib.crc32(b"IHDR" + body))


def test_a_text_is_read_whole_through_both_routes(display, owners):
    for text in ["clipboard canary text", "파일을 읽습니다 안녕하세요", "emoji \U0001F642 and é", "quotes \" back\\slash\nnew line\ttab end", "x",
                 "separators " + chr(0x2028) + " and " + chr(0x2029) + " and \x85 and \x0b and \x0c end"]:       # what a line splitter would cut at, as escapes
        owners(text.encode())
        code, answer = both("--kind", "text", display=display)
        assert code == 0 and answer == {"ok": 1, "kind": "text", "selection": "clipboard", "text": text, "bytes": len(text.encode())}, answer
    code, answer = both(display=display)                     # the defaults: the clipboard, as text
    assert (code, answer["kind"], answer["selection"]) == (0, "text", "clipboard")


def test_the_two_selections_hold_different_texts_and_each_read_gets_its_own(display, owners):
    owners(b"the clipboard text", "clipboard")
    owners(b"the primary text", "primary")
    assert both("--selection", "clipboard", display=display)[1]["text"] == "the clipboard text"
    code, answer = both("--selection", "primary", display=display)
    assert (code, answer["text"], answer["selection"]) == (0, "the primary text", "primary")
    assert both(display=display)[1]["text"] == "the clipboard text"


def test_the_text_limit_is_exact_and_nothing_is_cut(display, owners):
    owners(b"a" * 65536)
    code, answer = both(display=display)
    assert (code, answer["bytes"]) == (0, 65536), "64 KiB exactly is accepted by default"
    owners(b"a" * 65537)
    code, answer = both(display=display)
    assert (code, answer["error"]) == (2, "too_large") and "text" not in answer, "64 KiB + 1 is refused"
    owners("가나다".encode())                                 # 9 bytes
    assert both("--max-bytes", "9", display=display)[0] == 0
    code, answer = both("--max-bytes", "8", display=display)
    assert (code, answer["error"]) == (2, "too_large")


def test_an_empty_text_and_no_owner_are_empty(display, owners):
    owners(b"")
    code, answer = both(display=display)
    assert (code, answer["error"]) == (2, "empty"), answer
    done = subprocess.run(["xclip", "-selection", "primary", "-o"], env={"DISPLAY": display, "PATH": "/usr/bin:/bin"}, capture_output=True)
    assert done.returncode != 0, "nobody owns PRIMARY in this test"
    code, answer = both("--selection", "primary", display=display)
    assert (code, answer["error"]) == (2, "empty") and answer["detail"], answer


def test_a_small_picture_is_saved_and_read_back_by_two_readers(display, owners):
    png, pixels = make_png(40, 30)
    owners(png, target="image/png")
    with private_dir() as d:
        for route in ROUTES:
            out = Path(d) / f"small-{route}.png"
            code, answer = lico(route, "--kind", "image", "--out", str(out), display=display)
            assert code == 0 and answer == {"ok": 1, "kind": "image", "selection": "clipboard", "path": str(out), "bytes": len(png), "width": 40, "height": 30}, answer
            assert out.read_bytes() == png
            w, h, got = read_png(out)                                  # reader 1: the test's decoder
            assert (w, h, got) == (40, 30, pixels)
            assert im("convert", str(out), "rgb:-") == pixels          # reader 2: ImageMagick
            assert subprocess.run(["identify", "-format", "%wx%h", str(out)], capture_output=True, check=True).stdout == b"40x30"
        # auto takes the picture when the owner offers one
        out = Path(d) / "auto.png"
        code, answer = both("--kind", "auto", "--out", str(out), display=display)
        assert (code, answer["kind"], answer["bytes"]) == (0, "image", len(png)) and out.read_bytes() == png


def test_a_picture_over_the_request_size_arrives_whole_by_incr_and_the_limit_is_exact(display, owners):
    png, pixels = make_png(800, 900, noise=True)                       # about 2.2 MB of noise: xclip serves it in chunks (see clipboard::tests_x)
    assert 2_000_000 < len(png) < 3_145_728
    owners(png, target="image/png")
    with private_dir() as d:
        for route in ROUTES:
            out = Path(d) / f"big-{route}.png"
            code, answer = lico(route, "--kind", "image", "--out", str(out), display=display)
            assert (code, answer["bytes"], answer["width"], answer["height"]) == (0, len(png), 800, 900), answer
            assert out.read_bytes() == png, "every byte, in order"
            assert read_png(out)[2] == pixels and im("convert", str(out), "rgb:-") == pixels      # two independent readers
        out = Path(d) / "exact.png"
        assert both("--kind", "image", "--out", str(out), "--max-bytes", str(len(png)), display=display)[0] == 0
        out2 = Path(d) / "short.png"
        code, answer = both("--kind", "image", "--out", str(out2), "--max-bytes", str(len(png) - 1), display=display)
        assert (code, answer["error"]) == (2, "too_large") and not out2.exists(), "one byte under the size is refused and leaves no file"


def test_a_picture_that_is_too_wide_or_not_a_png_is_refused_and_leaves_nothing(display, owners):
    with private_dir() as d:
        folder = Path(d)
        out = folder / "x.png"
        owners(forged_head(9000, 20), target="image/png")
        code, answer = both("--kind", "image", "--out", str(out), display=display)
        assert (code, answer["error"]) == (2, "too_large"), answer
        owners(forged_head(8000, 8000), target="image/png")             # exactly the edge is accepted (the header only; nothing is decoded)
        code, answer = both("--kind", "image", "--out", str(out), display=display)
        assert (code, answer["width"], answer["height"]) == (0, 8000, 8000)
        out.unlink()
        for blob in [b"this is not a PNG at all" * 20, b"\xff\xd8\xff\xe0" + b"jpeg" * 100, forged_head(0, 5), forged_head(5, 0)]:
            owners(blob, target="image/png")
            code, answer = both("--kind", "image", "--out", str(out), display=display)
            assert (code, answer["error"]) == (2, "bad_image"), answer
        assert list(folder.iterdir()) == [], list(folder.iterdir())
        assert not out.exists()


def test_a_format_the_kind_cannot_use_is_unsupported_not_empty(display, owners):
    owners(b"\xff\xd8\xff\xe0 jpeg bytes", target="image/jpeg")
    with private_dir() as d:
        for kind in ["image", "auto", "text"]:
            args = ["--kind", kind] + (["--out", str(Path(d) / "j.png")] if kind != "text" else [])
            code, answer = both(*args, display=display)
            assert (code, answer["error"]) == (2, "unsupported_format"), (kind, answer)
        assert list(Path(d).iterdir()) == []
    owners(b"<b>html</b>", target="text/html")
    code, answer = both("--kind", "text", display=display)
    assert (code, answer["error"]) == (2, "unsupported_format"), answer
    owners(b"just words")                                              # a text owner and an image wanted
    with private_dir() as d:
        code, answer = both("--kind", "image", "--out", str(Path(d) / "t.png"), display=display)
        assert (code, answer["error"]) == (2, "unsupported_format") and list(Path(d).iterdir()) == []
        code, answer = both("--kind", "auto", "--out", str(Path(d) / "t.png"), display=display)
        assert (code, answer["kind"], answer["text"]) == (0, "text", "just words") and list(Path(d).iterdir()) == [], "auto gives the text when only text is offered"


def test_an_owner_that_never_answers_gives_timeout_within_the_limit_and_a_second(display, owners):
    proc = owners(b"never delivered")
    os.kill(proc.pid, signal.SIGSTOP)                                  # it holds the selection and answers nothing
    try:
        for route in ROUTES:
            started = time.monotonic()
            code, answer = lico(route, "--timeout-ms", "400", display=display)
            took = time.monotonic() - started
            assert (code, answer["error"]) == (2, "timeout") and 0.4 <= took < 1.4, (answer, took)
    finally:
        os.kill(proc.pid, signal.SIGCONT)


@pytest.mark.parametrize("umask", [0o022, 0o077])
def test_the_file_is_private_never_follows_a_link_and_the_path_is_absolute(display, owners, umask):
    png, _ = make_png(9, 7)
    owners(png, target="image/png")
    with private_dir() as d:
        folder = Path(d)
        for route in ROUTES:
            out = folder / f"mode-{route}.png"
            assert lico(route, "--kind", "image", "--out", str(out), display=display, umask=umask)[0] == 0
            assert stat.S_IMODE(out.stat().st_mode) == 0o600
            code, answer = lico(route, "--kind", "image", "--out", f"rel-{route}.png", display=display, cwd=folder, umask=umask)     # a relative name
            assert code == 0 and answer["path"] == str(folder / f"rel-{route}.png") and (folder / f"rel-{route}.png").read_bytes() == png
        secret = folder / "target.txt"
        secret.write_text("CANARY-untouched")
        link = folder / "link.png"
        link.symlink_to(secret)
        assert lico("cmd", "--kind", "image", "--out", str(link), display=display, umask=umask)[0] == 0
        assert secret.read_text() == "CANARY-untouched" and not link.is_symlink() and link.read_bytes() == png
        # the folder must exist: no folder is made, nothing is left
        code, answer = lico("cmd", "--kind", "image", "--out", str(folder / "no" / "such" / "x.png"), display=display, umask=umask)
        assert (code, answer["error"]) == (2, "write_failed") and not (folder / "no").exists()
        assert not [p for p in folder.iterdir() if p.name.endswith(".tmp")], "no temporary file is left"


def test_no_display_says_so():
    for route in ROUTES:
        code, answer = lico(route, "--kind", "text")
        assert (code, answer["error"]) == (2, "no_display") and answer["detail"]


USAGE_CASES = [
    ["--selection"], ["--kind"], ["--out"], ["--max-bytes"], ["--timeout-ms"],                      # a missing value
    ["--selection", "secondary"], ["--kind", "png"], ["--max-bytes", "0"], ["--max-bytes", "-5"], ["--max-bytes", "x"],
    ["--timeout-ms", "0"], ["--timeout-ms", "1.5"],                                                  # a bad value
    ["--selection", "primary", "--selection", "primary"], ["--kind", "text", "--kind", "text"],
    ["--max-bytes", "9", "--max-bytes", "9"], ["--timeout-ms", "9", "--timeout-ms", "9"],
    ["--kind", "image", "--out", "a.png", "--out", "b.png"],                                         # doubled
    ["--kind", "image"], ["--kind", "auto"], ["--out", "x.png"], ["--kind", "text", "--out", "x.png"],   # --out: required for a picture, not for a text
    ["--bogus"], ["stray"],
]


@pytest.mark.parametrize("args", USAGE_CASES, ids=lambda a: " ".join(a))
def test_usage_errors_are_one_json_line_with_exit_1_and_make_no_file(args):
    with private_dir() as d:
        for route in ROUTES:
            code, answer = lico(route, *args, cwd=d)                       # no display is needed to find a usage error
            assert (code, answer["ok"], answer["error"]) == (1, 0, "usage") and answer["detail"], (route, answer)
        assert list(Path(d).iterdir()) == []


def test_write_is_not_there_yet_and_the_usage_text_names_the_read_but_not_the_hidden_job():
    for args in (["clipboard", "write", "x"], ["clipboard"], ["clipboard", "paste"]):
        done = subprocess.run([str(EXE), *args], capture_output=True, text=True, stdin=subprocess.DEVNULL, env={"PATH": "/usr/bin:/bin"})
        line = json.loads(done.stdout)
        assert (done.returncode, line["error"]) == (1, "usage"), (args, done.stdout)
    text = subprocess.run([str(EXE)], capture_output=True, text=True, stdin=subprocess.DEVNULL, env={"PATH": "/usr/bin:/bin"})
    assert "clipboard read" in text.stdout + text.stderr and "xjob" not in text.stdout + text.stderr
    tracked = subprocess.run(["git", "-C", str(REPO), "ls-files", "*README*"], capture_output=True, text=True, check=True).stdout.split()
    for name in tracked:
        assert "xjob" not in (REPO / name).read_text(errors="replace"), name
