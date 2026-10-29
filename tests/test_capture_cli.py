"""`lico capture` through the real executable (class L3, a small private display, no host). The pixels of the pictures are proved in the Rust tests
(`capture::tests_x`, 1080p, two readers, the reference shrink); here the command line is: the answer line, the exit codes, every failure reason, the
files left behind. The screen is read twice, by the command and by ImageMagick's `import`, and the two must agree pixel for pixel."""
import json
import os
import stat
import struct
import subprocess
import zlib
from pathlib import Path

import pytest

from lico_measure.p02 import HOSTD_DIR
from lico_measure.rig import Xvfb, private_dir

EXE = HOSTD_DIR / "lico"
W, H = 1700, 120      # wider than 1568, so the agent copy is a shrunk one


@pytest.fixture(scope="module")
def display():
    server = Xvfb(f"{W}x{H}x24")
    yield server.display
    server.stop()


def lico(*args, display=None, cwd=None, timeout=60):
    # no host socket anywhere in the environment: the command must not need one
    env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": "/CANARY-no-home"}
    if display:
        env["DISPLAY"] = display
    done = subprocess.run([str(EXE), "capture", *args], env=env, capture_output=True, text=True, timeout=timeout, cwd=cwd)
    lines = done.stdout.splitlines()
    assert len(lines) == 1, (done.stdout, done.stderr)      # one JSON line, always
    return done.returncode, json.loads(lines[0])


def read_png(path):
    """The test's own decoder (zlib for the inflate, the five unfilters written here)."""
    data = Path(path).read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    pos, idat, head = 8, b"", None
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        assert struct.unpack(">I", data[pos + 8 + n:pos + 12 + n])[0] == zlib.crc32(kind + body)
        if kind == b"IHDR":
            head = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    w, h, depth, colour = head[:4]
    assert (depth, colour) == (8, 2)
    raw, rl = zlib.decompress(idat), w * 3
    assert len(raw) == h * (rl + 1)
    out = bytearray(h * rl)
    for y in range(h):
        ft = raw[y * (rl + 1)]
        for i in range(rl):
            a = out[y * rl + i - 3] if i >= 3 else 0
            b = out[(y - 1) * rl + i] if y else 0
            c = out[(y - 1) * rl + i - 3] if y and i >= 3 else 0
            if ft == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
            else:
                pred = (0, a, b, (a + b) // 2)[ft]
            out[y * rl + i] = (raw[y * (rl + 1) + 1 + i] + pred) & 255
    return w, h, bytes(out)


def im(*args):
    return subprocess.run(["convert" if args[0] != "import" else "import", *args[1:]], capture_output=True, check=True).stdout


def test_a_capture_answers_with_one_line_and_both_files_show_the_screen(display):
    with private_dir() as d:
        out = Path(d) / "shot.png"
        code, a = lico("--out", str(out), display=display)
        assert code == 0 and a["ok"] == 1
        assert (a["path"], a["width"], a["height"], a["offset"]) == (str(out), W, H, {"x": 0, "y": 0})
        assert a["bytes"] == out.stat().st_size and a["server"]["vendor"] and isinstance(a["server"]["release"], int)
        ag = a["agent"]
        assert (ag["path"], ag["width"], ag["height"]) == (str(Path(d) / "shot.agent.png"), 1568, 111)
        assert abs(ag["scale"] - 1568 / 1700) < 1e-12 and ag["bytes"] == Path(ag["path"]).stat().st_size
        assert stat.S_IMODE(out.stat().st_mode) == 0o600 and stat.S_IMODE(Path(ag["path"]).stat().st_mode) == 0o600
        # two readers of the screen agree: the command and ImageMagick's import
        w, h, pix = read_png(out)
        assert (w, h) == (W, H)
        assert pix == im("import", "-display", display, "-window", "root", "-depth", "8", "rgb:-")
        assert pix == im("convert", str(out), "rgb:-")
        aw, ah, _ = read_png(ag["path"])
        assert (aw, ah) == (1568, 111)
        assert sorted(p.name for p in Path(d).iterdir()) == ["shot.agent.png", "shot.png"]


def test_a_region_and_a_window_carry_their_offset_and_a_small_capture_has_no_second_file(display):
    with private_dir() as d:
        out = Path(d) / "r.png"
        code, a = lico("--region", "30,20,101,33", "--out", str(out), display=display)
        assert code == 0 and (a["width"], a["height"], a["offset"]) == (101, 33, {"x": 30, "y": 20})
        assert a["agent"] == {"path": str(out), "width": 101, "height": 33, "bytes": a["bytes"], "scale": 1.0}
        assert [p.name for p in Path(d).iterdir()] == ["r.png"]
        w, h, pix = read_png(out)
        whole = im("import", "-display", display, "-window", "root", "-depth", "8", "rgb:-")
        assert pix == b"".join(whole[((20 + y) * W + 30) * 3:((20 + y) * W + 131) * 3] for y in range(33))
        # a window: the root is a window, and a decimal and a hex id are the same
        root = subprocess.run(["xwininfo", "-display", display, "-root"], capture_output=True, text=True).stdout
        wid = int(root.split("Window id:")[1].split()[0], 16)
        for form in (str(wid), hex(wid)):
            code, b = lico("--window", form, "--out", str(Path(d) / "w.png"), display=display)
            assert code == 0 and (b["width"], b["height"], b["offset"]) == (W, H, {"x": 0, "y": 0}), b


@pytest.mark.parametrize("args,reason,code", [
    ([], "usage", 1),
    (["--screen"], "usage", 1),
    (["--out"], "usage", 1),
    (["--screen", "--region", "0,0,1,1", "--out", "x.png"], "usage", 1),
    (["--window", "12x", "--out", "x.png"], "usage", 1),
    (["--bogus", "--out", "x.png"], "usage", 1),
    (["--region", "1,2,3", "--out", "x.png"], "bad_region", 2),
    (["--region", "0,0,0,5", "--out", "x.png"], "bad_region", 2),
    (["--region", f"1,0,{W},5", "--out", "x.png"], "bad_region", 2),
    (["--region", f"0,{H - 1},5,2", "--out", "x.png"], "bad_region", 2),
    (["--window", "0x7ffffff0", "--out", "x.png"], "no_such_window", 2),
    (["--out", "/CANARY-no-such-folder/x.png"], "write_failed", 2),
])
def test_every_failure_has_one_reason_and_leaves_no_file(display, args, reason, code):
    with private_dir() as d:
        got_code, answer = lico(*args, display=display, cwd=d)
        assert (got_code, answer["ok"], answer["error"]) == (code, 0, reason), answer
        assert list(Path(d).iterdir()) == []


def test_no_display_and_an_unsupported_screen_are_reported():
    code, answer = lico("--out", "/CANARY-x/x.png", display=":4999")        # no such server, and none of ours is touched
    assert (code, answer["error"]) == (2, "no_display")
    code, answer = lico("--out", "/CANARY-x/x.png")                          # no DISPLAY at all
    assert (code, answer["error"]) == (2, "no_display")
    server = Xvfb("320x200x16")
    try:
        with private_dir() as d:
            code, answer = lico("--out", str(Path(d) / "x.png"), display=server.display)
            assert (code, answer["error"]) == (2, "unsupported_screen"), answer
            assert list(Path(d).iterdir()) == []
    finally:
        server.stop()
