"""Which build a person is looking at: the title of the window, Help > About Lico and `lico --version` say the same thing (M02-P03-H1)."""

import re
import subprocess

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO

from tests.test_stale_host import status


def menu(w):
    return w.ui()["menu"]


def test_the_title_the_about_panel_and_the_command_line_say_which_build_this_is():
    with ProductWindow() as w:
        found = re.fullmatch(r"Lico (\S+) \(commit ([0-9a-f]{7,}(?:-dirty)?|unknown)\)", w.title())
        assert found, w.title()                                                      # the version and the commit, short enough for a title bar
        version, commit = found.groups()
        build = status(w)["build"]                                                   # the host this window started runs the window's own build
        assert re.fullmatch(r"[0-9a-f]{16}", build)
        w.key("alt+h")
        assert menu(w)["open"] == 3
        w.key("Down")
        assert "About Lico" in menu(w)["items"][menu(w)["selected"]]["label"]
        w.key("Return")
        panel = w.ui()["panel"]
        assert panel["title"] == "About Lico" and f"Lico {version} (commit {commit}, build {build})" in panel["body"], panel
        for form in ("version", "--version"):
            said = subprocess.run([str(LICO), form], env=w.env, capture_output=True, text=True, timeout=30).stdout.strip()
            assert said == f"lico {version} (commit {commit}, build {build})", said     # the command-line form of the same line
