import os
import stat

from lico_measure.hostile import CANARY, run_hostile_cases
from lico_measure.rig import Host, private_dir


def test_hostile_requests_rejected_without_effect():
    with private_dir() as directory:
        os.environ["LICO_CANARY_CREDENTIAL"] = CANARY
        host = Host(directory, sessions=2, producing=1)
        try:
            results = run_hostile_cases(host)
            names = {r["case"] for r in results}
            assert {"unknown_session", "guessed_id", "stale_generation", "oversize_request",
                    "malformed_json", "unknown_op", "canary_in_sinks"} <= names
            assert all(r["rejected"] and r["no_effect"] for r in results), results
            assert stat.S_IMODE(os.stat(host.socket).st_mode) == 0o600
            assert stat.S_IMODE(os.stat(os.path.dirname(host.socket)).st_mode) == 0o700
            planted = [r for r in results if r["case"] == "canary_in_sinks"][0]
            assert "request" in planted["detail"]       # the canary was sent, not just present in the environment
        finally:
            host.stop()
            os.environ.pop("LICO_CANARY_CREDENTIAL", None)
