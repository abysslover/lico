import pytest

from lico_measure.decision import DecisionError, recommend, validate_claim
from lico_measure.report import TARGETS, build_measurement

SCOPE = {"candidate": "x", "workload": "w", "process_scope": "gui tree"}


def _results(**overrides):
    """Per candidate, one record per targeted metric; overrides replace samples."""
    base = {"gui_idle_rss_mib": [30.0] * 5, "hidden_growth_rss_mib": [0.1] * 3,
            "activation_p95_ms": [10.0] * 100, "restart_p95_ms": [60.0] * 100,
            "idle_cpu_percent": [0.0]}
    out = {}
    for candidate, changes in overrides.items():
        samples = dict(base, **changes)
        out[candidate] = {
            metric: build_measurement(metric, dict(SCOPE, candidate=candidate,
                                                   **({"interval_s": 60} if metric == "idle_cpu_percent" else {})),
                                      values)
            for metric, values in samples.items()}
    return out


def test_architecture_decision_references_observed_budgets():
    assert set(_results(x={})["x"]) == set(TARGETS)
    results = _results(x={}, y={"activation_p95_ms": [1.0] * 99, "restart_p95_ms": [900.0] * 100})
    rec = recommend(results)
    assert rec["candidate"] == "x"
    basis = {b["metric"]: b for b in rec["basis"]}
    assert basis["gui_idle_rss_mib"]["value"] == 30.0 and basis["gui_idle_rss_mib"]["classification"] == "met"
    # an unmeasured or missed budget can never be called passed
    with pytest.raises(DecisionError):
        validate_claim("y", {"activation_p95_ms": "met"}, results)   # only 99 samples: unverified
    with pytest.raises(DecisionError):
        validate_claim("y", {"restart_p95_ms": "met"}, results)      # 900 ms: missed
    validate_claim("x", {"restart_p95_ms": "met"}, results)
    # a candidate whose only gap is an unverified budget is not accepted either
    with pytest.raises(DecisionError):
        recommend(_results(w={"activation_p95_ms": [1.0] * 99}))
    # when nothing is accepted a decision with three real alternatives is required
    none = _results(y={"gui_idle_rss_mib": [90.0] * 5}, z={"idle_cpu_percent": [2.0]})
    with pytest.raises(DecisionError):
        recommend(none)
    with pytest.raises(DecisionError):
        recommend(none, alternatives=[{"name": "a", "tradeoff": "t"}] * 2)
    options = [{"name": n, "tradeoff": "t"} for n in "abc"]
    decision = recommend(none, alternatives=options, recommended="a")
    assert decision["candidate"] is None and len(decision["alternatives"]) == 3
