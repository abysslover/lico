"""`lico xjob <job>`, the hidden job side of the helper process (plan M02-P04, S03; XF-07), through the real executable (class L3, a small private
display, no host). The host side (`helper::run_job`) and the property that a host is unchanged after jobs are proved in the Rust tests
(`helper::tests`); here the program is what a host would start: the same answer line and exit codes as `lico capture`, the error line of an unknown
job, and the name that no usage text or README gives away."""
import json
import os
import subprocess
from pathlib import Path

import pytest

from lico_measure.p02 import HOSTD_DIR
from lico_measure.rig import REPO, Xvfb, private_dir
from tests.test_capture_cli import im, read_png

EXE = HOSTD_DIR / "lico"
W, H = 400, 240


@pytest.fixture(scope="module")
def display():
    server = Xvfb(f"{W}x{H}x24")
    yield server.display
    server.stop()


def run(*args, display=None, cwd=None):
    # no host socket anywhere in the environment and nothing on standard input: a job needs neither
    env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": "/CANARY-no-home"}
    if display:
        env["DISPLAY"] = display
    done = subprocess.run([str(EXE), *args], env=env, capture_output=True, text=True, timeout=60, cwd=cwd, stdin=subprocess.DEVNULL)
    return done


def one_line(done):
    lines = done.stdout.splitlines()
    assert len(lines) == 1, (done.stdout, done.stderr)
    return json.loads(lines[0])


def test_the_capture_job_is_lico_capture_with_a_hidden_name(display):
    with private_dir() as d:
        a_png, b_png = Path(d) / "a.png", Path(d) / "b.png"
        region = ["--region", "13,9,321,199"]            # odd sizes, inside the 400 x 240 screen; 321 x 199 is not shrunk, no second file
        via_job = run("xjob", "capture", *region, "--out", str(a_png), display=display)
        via_cmd = run("capture", *region, "--out", str(b_png), display=display)
        assert via_job.returncode == via_cmd.returncode == 0, (via_job.stderr, via_cmd.stderr)
        a, b = one_line(via_job), one_line(via_cmd)
        assert set(a) == set(b) == {"ok", "path", "width", "height", "bytes", "offset", "server", "agent"}
        assert (a["ok"], a["width"], a["height"], a["offset"]) == (1, 321, 199, {"x": 13, "y": 9})
        for key in ("ok", "width", "height", "bytes", "offset", "server"):
            assert a[key] == b[key], key
        assert (a["path"], a["agent"]["path"]) == (str(a_png), str(a_png)) and a["agent"]["scale"] == 1.0
        # a valid PNG with the pixels of the screen: read by the test's decoder and by ImageMagick, and compared with the screen from `import`
        w, h, pix = read_png(a_png)
        assert (w, h) == (321, 199)
        whole = im("import", "-display", display, "-window", "root", "-depth", "8", "rgb:-")
        assert pix == b"".join(whole[((9 + y) * W + 13) * 3:((9 + y) * W + 13 + 321) * 3] for y in range(199))
        assert pix == im("convert", str(a_png), "rgb:-")
        assert a_png.read_bytes() == b_png.read_bytes()
        assert sorted(p.name for p in Path(d).iterdir()) == ["a.png", "b.png"]


@pytest.mark.parametrize("args,reason,code", [
    ([], "usage", 1),                                                   # no job
    (["nosuchjob"], "usage", 1),
    (["Capture", "--out", "x.png"], "usage", 1),                        # names are exact
    (["--screen", "--out", "x.png"], "usage", 1),                       # a capture word is not a job name
    (["capture"], "usage", 1),                                          # a bad capture argument: --out is required
    (["capture", "--region", "1,2,3", "--out", "x.png"], "bad_region", 2),
    (["capture", "--bogus", "--out", "x.png"], "usage", 1),
])
def test_the_job_errors_are_one_json_line_with_the_exit_code_of_the_command(display, args, reason, code):
    with private_dir() as d:
        done = run("xjob", *args, display=display, cwd=d)
        answer = one_line(done)
        assert (done.returncode, answer["ok"], answer["error"]) == (code, 0, reason), (answer, done.stderr)
        assert isinstance(answer["detail"], str) and answer["detail"]
        assert list(Path(d).iterdir()) == []


def test_a_job_without_a_display_says_so_and_the_unknown_job_names_itself():
    done = run("xjob", "capture", "--out", "/CANARY-x/x.png")
    assert (done.returncode, one_line(done)["error"]) == (2, "no_display")
    assert "nosuchjob" in one_line(run("xjob", "nosuchjob"))["detail"]


def test_the_hidden_name_is_in_no_usage_text_and_no_readme():
    for args in ([], ["--help"], ["help"], ["capture"], ["bogus"]):
        done = run(*args)
        assert "xjob" not in done.stdout + done.stderr, args
    assert "usage: lico" in run().stderr + run().stdout           # the usage text was really looked at
    tracked = subprocess.run(["git", "-C", str(REPO), "ls-files", "*README*"], capture_output=True, text=True, check=True).stdout.split()
    assert "README.md" in tracked
    for name in tracked:
        assert "xjob" not in (REPO / name).read_text(errors="replace"), name
