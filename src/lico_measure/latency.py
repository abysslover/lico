"""First-correct-frame latency read from the screen, not from an API reply."""

from __future__ import annotations

import time
from typing import Callable

SESSION_MARK = (8, 8)   # inside the 16x16 session-colour block
SEQ_MARK = (24, 8)      # inside the 16x16 block coloured by the last drawn sequence


def measure_switch(
    issue: Callable[[], None],
    pixel: Callable[[int, int], int],
    expect_session_rgb: int,
    min_seq: int,
    clock_ns: Callable[[], int] = time.monotonic_ns,
    timeout_s: float = 2.0,
) -> float:
    """Milliseconds from issuing a switch to the first screen state that shows the
    target session with content at least as new as ``min_seq``.

    A frame of another session, or of the right session with stale content, does
    not count. Reply events from the client are never consulted.
    """
    start = clock_ns()
    issue()
    deadline = start + int(timeout_s * 1e9)
    while True:
        if pixel(*SESSION_MARK) == expect_session_rgb and pixel(*SEQ_MARK) >= min_seq:
            return (clock_ns() - start) / 1e6
        if clock_ns() > deadline:
            raise TimeoutError("no correct frame was shown")
