import pytest

from lico_measure.report import ReportError, build_measurement

SCOPE = {"candidate": "a", "workload": "switch-100", "process_scope": "gui tree"}


def test_measurement_report_requires_scope_and_samples():
    # a missing scope or a missing/zero-filled value is refused, never defaulted
    with pytest.raises(ReportError):
        build_measurement("activation_p95_ms", {"candidate": "a"}, [50.0] * 100)
    with pytest.raises(ReportError):
        build_measurement("activation_p95_ms", SCOPE, [])
    with pytest.raises(ReportError):
        build_measurement("activation_p95_ms", SCOPE, [10.0, None, 12.0])
    # too few samples: the target is unverified even though every value is small
    few = build_measurement("activation_p95_ms", SCOPE, [1.0] * 99)
    assert few["classification"] == "unverified" and few["n"] == 99
    # enough samples: classified against the target with a declared method
    met = build_measurement("activation_p95_ms", SCOPE, [10.0] * 100)
    assert met["classification"] == "met" and met["statistic"] == "p95" and met["n"] == 100
    slow = build_measurement("activation_p95_ms", SCOPE, [10.0] * 90 + [400.0] * 10)
    assert slow["classification"] == "missed" and slow["value"] == 400.0
    # a CPU figure from an interval shorter than the declared 60 s stays unverified
    cpu = build_measurement("idle_cpu_percent", dict(SCOPE, interval_s=10), [0.1])
    assert cpu["classification"] == "unverified"
