"""Turn observed, classified measurements into a recommendation or a decision.

A candidate is accepted only when every targeted metric is classified met.
Unmet or unmeasured budgets can never be called passed, and when nothing is
accepted a decision with at least three real alternatives is required.
"""

from __future__ import annotations

from .report import TARGETS


class DecisionError(ValueError):
    pass


def _problems(records: dict[str, dict]) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {"missed": [], "unverified": []}
    for metric in TARGETS:
        record = records.get(metric)
        state = record["classification"] if record else "unverified"
        if state != "met":
            out["missed" if state == "missed" else "unverified"].append(metric)
    return out


def validate_claim(candidate: str, claims: dict[str, str], results: dict[str, dict]) -> None:
    """Refuse any claim that a budget is met when the observation says otherwise."""
    records = results[candidate]
    for metric, claimed in claims.items():
        record = records.get(metric)
        observed = record["classification"] if record else "unverified"
        if claimed == "met" and observed != "met":
            raise DecisionError(f"{candidate}: {metric} is {observed}, not met")


def recommend(results: dict[str, dict], alternatives: list[dict] | None = None,
              recommended: str | None = None) -> dict:
    accepted = {c: r for c, r in results.items() if not any(_problems(r).values())}
    if accepted:
        # prefer the smallest idle footprint among accepted candidates
        best = min(accepted, key=lambda c: accepted[c]["gui_idle_rss_mib"]["value"])
        basis = [{"metric": m, "value": accepted[best][m]["value"], "unit": accepted[best][m]["unit"],
                  "classification": accepted[best][m]["classification"], "n": accepted[best][m]["n"]}
                 for m in TARGETS]
        return {"candidate": best, "basis": basis, "accepted": sorted(accepted)}
    if not alternatives or len(alternatives) < 3:
        raise DecisionError("no candidate meets the budgets: at least three alternatives are required")
    if recommended not in {a["name"] for a in alternatives}:
        raise DecisionError("a recommendation among the alternatives is required")
    return {"candidate": None, "alternatives": alternatives, "recommended": recommended,
            "problems": {c: _problems(r) for c, r in results.items()}}
