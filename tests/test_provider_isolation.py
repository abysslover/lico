import os
import stat
import sys
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, read_log, start
from lico_measure.rig import REPO, private_dir

from tests.test_codex_adapter import FIXTURE, SIM, codex_host, new_session, texts, wait_for

CANARY = "LICO_SYNTH_CANARY_isolation_0000000000000000"


def test_provider_environment_is_an_allowlist_and_the_workspace_is_private():
    os.environ["LICO_HOST_SECRET"] = CANARY          # something the host has and a provider must never get
    os.environ["AWS_SECRET_ACCESS_KEY"] = CANARY
    try:
        with private_dir() as directory:
            host, log, received = codex_host(directory)
            try:
                _c1, first, g1 = new_session(host)
                _c2, second, g2 = new_session(host)
                for sid, gen in ((first, g1), (second, g2)):
                    Conn(host.socket).call(op="input", session=sid, generation=gen, request_id="env", text="env")
                    wait_for(lambda: any(t.startswith("CWD ") for t in texts(log, sid)))
                names = {sid: [t[4:] for t in texts(log, sid) if t.startswith("ENV ")] for sid in (first, second)}
                allowed = {"PATH", "HOME", "LANG", "CODEX_HOME", "LC_CTYPE"}
                for sid in (first, second):
                    assert names[sid] and set(names[sid]) <= allowed, names[sid]
                    assert "LICO_HOST_SECRET" not in names[sid] and "AWS_SECRET_ACCESS_KEY" not in names[sid]
                    assert not any(CANARY in t for t in texts(log, sid))
                cwd = {sid: [t[4:] for t in texts(log, sid) if t.startswith("CWD ")][0] for sid in (first, second)}
                mode = {sid: [t[8:] for t in texts(log, sid) if t.startswith("CWDMODE ")][0] for sid in (first, second)}
                workspaces = Path(directory) / "workspaces"
                assert cwd[first] != cwd[second]                       # one workspace per session
                for sid in (first, second):
                    assert Path(cwd[sid]) == workspaces / sid          # under the host's workspace root, named by the session
                    assert mode[sid] == "700" and stat.S_IMODE(os.stat(workspaces).st_mode) == 0o700
            finally:
                host.stop()
    finally:
        os.environ.pop("LICO_HOST_SECRET", None)
        os.environ.pop("AWS_SECRET_ACCESS_KEY", None)


def test_purge_removes_the_session_log_and_nothing_else():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn, doomed, gd = new_session(host)
            _c2, kept, gk = new_session(host)
            for sid, gen in ((doomed, gd), (kept, gk)):
                conn.call(op="input", session=sid, generation=gen, request_id="c", text=f"keep nothing {CANARY}")
                wait_for(lambda: any(CANARY in t for t in texts(log, sid)))
                wait_for(lambda: "[turn completed]" in texts(log, sid))        # the echo of the input comes first; the size below is taken when the provider has said all it will
            before = {p.name: p.stat().st_size for p in log.iterdir() if p.name.startswith(kept)}
            pids = {a["session"]: a["pid"] for a in conn.call(op="resources")["agents"]}
            workspace = Path(directory) / "workspaces" / doomed
            assert workspace.is_dir()
            # a stale generation purges nothing
            assert conn.call(op="session.purge", session=doomed, generation=gd + 7)["error"] == "stale_generation"
            assert any(p.name.startswith(doomed) for p in log.iterdir())
            reply = conn.call(op="session.purge", session=doomed, generation=gd)
            assert reply["ok"] == 1 and reply["session"] == doomed
            time.sleep(0.5)
            # nothing of the purged session is left on disk, its provider is gone, and the other session is untouched
            assert not any(p.name.startswith(doomed) for p in log.iterdir()) and not workspace.exists()
            leftovers = [p for p in Path(directory).rglob("*") if p.is_file() and doomed in p.name]
            assert leftovers == []
            assert not os.path.exists(f"/proc/{pids[doomed]}") or open(f"/proc/{pids[doomed]}/stat").read().split()[2] == "Z"
            assert conn.call(op="input", session=doomed, generation=gd, request_id="again", text="x")["error"] == "unknown_session"
            after = {p.name: p.stat().st_size for p in log.iterdir() if p.name.startswith(kept)}
            assert after == before and [s["session"] for s in conn.call(op="status")["sessions"]] == [kept]
            assert conn.call(op="input", session=kept, generation=gk, request_id="more", text="still here")["ok"] == 1
            wait_for(lambda: "echo: still here" in texts(log, kept))
            # the purged session's provider process ending must not bring its log back
            time.sleep(0.5)
            assert not any(p.name.startswith(doomed) for p in log.iterdir())
            # nor may a still-producing agent: its late output is dropped, not written to a new log
            busy = conn.call(op="session.create", kind="synthetic", rate=10000, history=0, payload=4096)
            time.sleep(0.5)
            assert conn.call(op="session.purge", session=busy["session"], generation=busy["generation"])["ok"] == 1
            time.sleep(1.0)
            assert not any(p.name.startswith(busy["session"]) for p in log.iterdir())
        finally:
            host.stop()
