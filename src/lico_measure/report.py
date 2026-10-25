"""Versioned measurement records.

A target copied from the product specification is never a result. Each record
carries its scope, sample count and statistic, and is classified as met, missed
or unverified; a missing value is an error, never zero.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

from .stats import median, percentile_nearest_rank

SCHEMA = "lico.measure.report.v1"
REQUIRED_SCOPE = ("candidate", "workload", "process_scope")


class ReportError(ValueError):
    pass


@dataclass(frozen=True)
class Target:
    limit: float
    unit: str
    statistic: str          # how the samples reduce to one value
    min_samples: int
    spec: str               # product specification identifier
    min_interval_s: float = 0.0


TARGETS = {
    "gui_idle_rss_mib": Target(50.0, "MiB", "max", 3, "PERF-01"),
    "hidden_growth_rss_mib": Target(10.0, "MiB", "max", 3, "PERF-02"),
    "activation_p95_ms": Target(100.0, "ms", "p95", 100, "PERF-03"),
    "restart_p95_ms": Target(500.0, "ms", "p95", 100, "PERF-04"),
    "idle_cpu_percent": Target(0.5, "% of one core", "max", 1, "PERF-05", min_interval_s=60.0),
}

# limits that hold the product to a cost without being part of the candidate decision (which weighs every entry of TARGETS)
PRODUCT_LIMITS = {
    "noisy_cpu_percent": Target(5.0, "% of one core", "max", 1, "PERF-05", min_interval_s=60.0),
}

_REDUCERS = {
    "p95": lambda s: percentile_nearest_rank(s, 0.95),
    "max": max,
    "median": median,
}


def build_measurement(metric: str, scope: dict, samples: list, unit: str | None = None) -> dict:
    missing = [key for key in REQUIRED_SCOPE if not scope.get(key)]
    if missing:
        raise ReportError(f"scope is missing {missing}")
    if not samples:
        raise ReportError("no samples recorded")
    for value in samples:
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
            raise ReportError("every sample must be a finite number; a missing value is not zero")
    target = TARGETS.get(metric) or PRODUCT_LIMITS.get(metric)
    statistic = target.statistic if target else "median"
    value = _REDUCERS[statistic](list(samples))
    record = {
        "metric": metric, "unit": unit or (target.unit if target else ""), "scope": dict(scope),
        "n": len(samples), "statistic": statistic, "value": value,
        "samples": list(samples),
    }
    if target is None:
        record["classification"] = "reported"
        return record
    record["target"] = {"limit": target.limit, "spec": target.spec, "comparison": "<="}
    short = len(samples) < target.min_samples
    short = short or float(scope.get("interval_s", 0)) < target.min_interval_s
    if short:
        record["classification"] = "unverified"
    else:
        record["classification"] = "met" if value <= target.limit else "missed"
    return record


def build_report(measurements: list[dict], environment: dict) -> dict:
    return {"schema": SCHEMA, "environment": environment, "measurements": measurements}
