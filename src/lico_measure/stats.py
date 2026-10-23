"""Small, explicit statistics: the method is part of every reported number."""

from __future__ import annotations

import math


def percentile_nearest_rank(samples: list[float], q: float) -> float:
    """Nearest-rank percentile: the value at rank ceil(q*n) of the sorted samples."""
    if not samples:
        raise ValueError("no samples")
    ordered = sorted(samples)
    return ordered[max(math.ceil(q * len(ordered)), 1) - 1]


def median(samples: list[float]) -> float:
    ordered = sorted(samples)
    mid = len(ordered) // 2
    return ordered[mid] if len(ordered) % 2 else (ordered[mid - 1] + ordered[mid]) / 2
