"""M24-P11g in the window: the menu bar at the right and the tabs of the left column are one row high, and the row of icons is as high as the two rows under
the tabs (the nav row and the host row in Directories, the host row and the filter row in Chats), so that the two lines across the top of the window are level
(the owner's words of 2026-11-26: "채팅창 상단 아이콘의 크기를 정확히 왼쪽 2줄과 맞춰서 줄이 딱 수평으로 나눠지도록 배치"). A private Xvfb and the simulated agent."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO
from lico_measure.rig import private_dir
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _centre(rect):
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _check_level(w: ProductWindow, tab: str) -> None:
    u = w.ui()
    sb = u["sidebar"]
    assert sb["tab"] == tab, sb["tab"]
    menu_bottom = u["menu"]["titles"][0]["rect"][3]
    icon = u["toolbar"]["buttons"][0]["rect"]
    first, second = (sb["back_rect"], sb["host_rect"]) if tab == "directories" else (sb["host_rect"], sb["filter_rect"])
    # the first row: the menu bar and the tabs end on one line (the tab's rectangle leaves the pixel of the line)
    assert abs(sb["dir_tab_rect"][3] + 1 - menu_bottom) < 0.5, (sb["dir_tab_rect"], menu_bottom)
    # the second block: two rows of one height with one gap above, between and below, and the icon as high as the two rows with their gap between
    gap = first[1] - menu_bottom
    row = first[3] - first[1]
    assert gap > 0 and row > 0, (gap, row)
    assert abs((second[3] - second[1]) - row) < 0.5 and abs((second[1] - first[3]) - gap) < 0.5, (first, second, gap, row)
    assert abs((icon[3] - icon[1]) - (2 * row)) < 0.5, ("the icon is the height of two rows", icon, row)
    pad = icon[1] - menu_bottom
    left_bottom = second[3] + gap
    right_bottom = icon[3] + pad
    assert abs(left_bottom - right_bottom) < 0.5, ("the second block ends on one line", left_bottom, right_bottom)
    # the lines are drawn: the same pixel at the left and at the right of the window, a different one a few rows above
    data, width, height = w.gray()
    x_left, x_right = 5, int(u["layout"]["list_w"]) + 300
    for y in (int(menu_bottom) - 1, int(right_bottom) - 1):
        assert data[y * width + x_left] == data[y * width + x_right], ("the line at row", y, data[y * width + x_left], data[y * width + x_right])
        assert data[y * width + x_left] != data[(y - 4) * width + x_left], ("a line, not the ground", y)


def test_the_lines_across_the_top_of_the_window_are_level_in_both_tabs():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        log_dir = root / "logs"
        args, _received = claude_args(root)
        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log_dir)]) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            wait_for(lambda: (u := w.ui()) and u["sidebar"]["dir_tab_rect"] and u["menu"]["titles"] and u["toolbar"]["buttons"] and u)
            _check_level(w, "directories")
            w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
            wait_for(lambda: (x := w.ui()["sidebar"]) and x["tab"] == "chats" and x["filter_rect"] and x)
            _check_level(w, "chats")
            w.click(*_centre(w.ui()["sidebar"]["dir_tab_rect"]))
            wait_for(lambda: (x := w.ui()["sidebar"]) and x["tab"] == "directories" and x["back_rect"] and x)
            _check_level(w, "directories")
