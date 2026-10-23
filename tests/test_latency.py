import pytest

from lico_measure.latency import measure_switch


class Fake:
    """Scripted screen: the API 'acks' at once, the correct frame comes later."""

    def __init__(self, frames):
        self.frames = list(frames)   # (session_rgb, seq_rgb), one per 2 ms of fake time
        self.now = 0

    def clock(self):
        self.now += 100_000          # 0.1 ms per call
        return self.now

    def pixel(self, x, y):
        frame = self.frames[min(self.now // 2_000_000, len(self.frames) - 1)]
        return frame[0] if x == 8 else frame[1]


def test_latency_uses_correct_frame_event():
    old, new = 0x111111, 0x222222
    # previous session, then the new session with stale content, then correct
    fake = Fake([(old, 5000), (old, 5000), (new, 4000), (new, 4000), (new, 6200)])
    issued = []
    ms = measure_switch(lambda: issued.append(True), fake.pixel, new, 6100, clock_ns=fake.clock, timeout_s=1.0)
    assert issued == [True]
    # a blank page whose sequence block shows a large unrelated colour is not content either
    blank = Fake([(new, 1_053_720), (new, 1_053_720), (new, 6200)])
    ms2 = measure_switch(lambda: None, blank.pixel, new, 6100, clock_ns=blank.clock, timeout_s=1.0)
    assert ms2 >= 4.0
    # the first four 2 ms slots show a wrong or stale frame; only the fifth is correct
    assert 8.0 <= ms < 10.0
    never = Fake([(old, 5000)])
    with pytest.raises(TimeoutError):
        measure_switch(lambda: None, never.pixel, new, 6100, clock_ns=never.clock, timeout_s=0.05)
