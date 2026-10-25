import json
import os
import re
import signal
import subprocess
import threading
import time
from pathlib import Path

from lico_measure.p02 import HOSTD, ProtoConn as Conn, read_log

from lico_measure.rig import private_dir
from tests.test_accounts import account, status_of as _unused  # noqa: F401  (account helper)
from tests.test_codex_adapter import codex_host, texts, wait_for


def sessions(conn):
    return {s["session"]: s for s in conn.call(op="status")["sessions"]}


def registry_file(directory):
    return Path(directory) / "registry" / "sessions.json"


def registered(directory):
    try:
        return {e["session"]: e for e in json.loads(registry_file(directory).read_text())["sessions"]}
    except (OSError, ValueError, KeyError):
        return {}


def hard_stop(host):
    host.proc.send_signal(signal.SIGKILL)       # no goodbye: what a crash or a power cut leaves behind
    host.proc.wait()


def test_host_restart_recovers_sessions_as_disconnected_with_bindings_and_history():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            kept = conn.call(op="session.create", kind="codex", account=a)
            gone = conn.call(op="session.create", kind="codex", account=a)
            sid, generation = kept["session"], kept["generation"]
            for k in range(3):
                conn.call(op="input", session=sid, generation=generation, request_id=f"h-{k}", text=f"hello 한글 {k}")
            wait_for(lambda: "echo: hello 한글 2" in texts(log, sid))
            assert conn.call(op="session.purge", session=gone["session"], generation=gone["generation"])["ok"] == 1
            wait_for(lambda: sid in registered(directory) and registered(directory)[sid].get("provider_session") and gone["session"] not in registered(directory))
            # the turn is closed and the last line the host holds is on disk: the log is read, and the host killed, only after that
            wait_for(lambda: "busy_since" not in sessions(conn)[sid] and sessions(conn)[sid]["seq"] == read_log(log, sid)[-1][0])
            before = read_log(log, sid)
            last_seq = before[-1][0]
            assert not any(gone["session"] in p.name for p in (Path(directory) / "log").glob("*.seg"))
        finally:
            hard_stop(host)
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            found = sessions(conn)
            assert set(found) == {sid}                                          # the purged session did not come back, nor did anything else
            entry = found[sid]
            assert entry["state"] == "disconnected" and entry["generation"] == generation and entry["account"] == a
            assert entry["state_at"] > 0 and entry["seq"] == last_seq and "approvals" not in entry
            # its history is there and its sequence continues without a gap or a duplicate
            snap = conn.call(op="snapshot", session=sid)
            assert [line[1] for line in snap["lines"] if "hello 한글" in line[1] and not line[1].startswith(">> ")] == [f"echo: hello 한글 {k}" for k in range(3)]
            assert [line[1] for line in snap["lines"] if line[1].startswith(">> hello 한글")] == [f">> hello 한글 {k}" for k in range(3)]       # each input is echoed once, and the echo survives the restart
            assert conn.call(op="input", session=sid, generation=generation, request_id="x", text="no")["error"] == "session_disconnected"
            # it is shown as disconnected until a person resumes it, and then it works under a new generation
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            assert resumed["ok"] == 1 and resumed["generation"] == generation + 1
            conn.call(op="input", session=sid, generation=resumed["generation"], request_id="after", text="after the restart")
            wait_for(lambda: "echo: after the restart" in texts(log, sid))
            seqs = [s for s, _ in read_log(log, sid)]
            assert seqs == list(range(len(seqs)))
            assert sessions(conn)[sid]["state"] == "running" and sessions(conn)[sid]["account"] == a
        finally:
            host.stop()


def test_a_torn_log_tail_or_a_damaged_registry_is_reported_and_never_replayed_as_data():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            made = conn.call(op="session.create", kind="codex")
            sid, generation = made["session"], made["generation"]
            for k in range(4):
                conn.call(op="input", session=sid, generation=generation, request_id=f"t-{k}", text=f"line number {k}")
            wait_for(lambda: "echo: line number 3" in texts(log, sid))
            wait_for(lambda: registered(directory).get(sid, {}).get("provider_session"))
        finally:
            hard_stop(host)
        segments = sorted((Path(directory) / "log").glob(f"{sid}.*.seg"))
        last = segments[-1]
        whole = last.read_bytes()
        last.write_bytes(whole[:-5])                                            # the last record is cut in the middle, as a crash during a write leaves it
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            entry = sessions(conn)[sid]
            assert entry["state"] == "disconnected"
            shown = [line[1] for line in conn.call(op="snapshot", session=sid)["lines"]]
            # the cut record (the last "[turn completed]") is not replayed; the record before it is
            assert shown.count("[turn completed]") == 3 and "echo: line number 3" in shown
            recovered = [line for line in shown if line.startswith("[log recovered")]
            assert len(recovered) == 1 and re.search(r"\d+ byte", recovered[0]), shown[-6:]
            damaged = list((Path(directory) / "log").glob(f"{sid}.*.damaged"))   # the bytes are set aside, not deleted and not read
            assert damaged and oct(damaged[0].stat().st_mode & 0o777) == "0o600"
            seqs = [s for s, _ in read_log(log, sid)]
            assert seqs == list(range(len(seqs)))                               # what remains is contiguous
            resumed = conn.call(op="session.resume", session=sid, generation=entry["generation"])
            conn.call(op="input", session=sid, generation=resumed["generation"], request_id="n", text="continues")
            wait_for(lambda: "echo: continues" in texts(log, sid))
            seqs = [s for s, _ in read_log(log, sid)]
            assert seqs == list(range(len(seqs)))                               # and the next record continues it
        finally:
            hard_stop(host)
        newest = sorted((Path(directory) / "log").glob(f"{sid}.*.seg"))[-1]
        with newest.open("ab") as handle:                                       # garbage after the last good record (a zeroed or foreign tail)
            handle.write(b"\xff\xff\xff\x7f" + b"garbage that claims to be a record")
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            shown = [line[1] for line in conn.call(op="snapshot", session=sid)["lines"]]
            assert not any("garbage" in line for line in shown)                 # nothing of it was replayed as a record
            assert len([line for line in shown if line.startswith("[log recovered")]) == 2
            assert b"garbage that claims to be a record" in b"".join(p.read_bytes() for p in (Path(directory) / "log").glob(f"{sid}.*.damaged"))
            seqs = [s for s, _ in read_log(log, sid)]
            assert seqs == list(range(len(seqs)))
        finally:
            hard_stop(host)
        newest = sorted((Path(directory) / "log").glob(f"{sid}.*.seg"))[-1]
        with newest.open("ab") as handle:                                       # a zero-filled tail, as some file systems leave after a power cut
            handle.write(b"\x00" * 64)
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            shown = [line[1] for line in conn.call(op="snapshot", session=sid)["lines"]]
            assert len([line for line in shown if line.startswith("[log recovered")]) == 3 and "" not in shown
            seqs = [s for s, _ in read_log(log, sid)]
            assert seqs == list(range(len(seqs)))                               # empty frames with a wrong sequence number were not accepted as records
        finally:
            hard_stop(host)
        # a damaged or forged registry stops the host instead of guessing
        for content, word in (("{not json", "sessions.json"),
                              (json.dumps({"sessions": [{"session": "../../etc/x", "provider": "codex", "generation": 1}]}), "sessions.json"),
                              (json.dumps({"sessions": [{"session": "s" + "0" * 16, "provider": "codex", "generation": 0}]}), "sessions.json"),   # a missing account is a removed one, not a forgery
                              (json.dumps({"sessions": [{"session": "s" + "0" * 16, "provider": "nothing", "generation": 1}]}), "sessions.json")):
            registry_file(directory).write_text(content)
            done = subprocess.run([str(HOSTD), "--socket", str(Path(directory) / "second.sock"), "--sessions", "0", "--producing", "0",
                                   "--log-dir", str(Path(directory) / "log"), "--accounts-dir", str(Path(directory) / "accounts"),
                                   "--registry-dir", str(Path(directory) / "registry")], capture_output=True, text=True, timeout=15)
            assert done.returncode != 0 and word in done.stderr and not (Path(directory) / "second.sock").exists(), (content, done.stderr)


def test_purge_during_a_resume_leaves_no_process_log_or_workspace():
    with private_dir() as directory:
        host, log, received = codex_host(directory, sim_args=["--resume-delay", "1.5"])
        try:
            conn = Conn(host.socket)
            made = conn.call(op="session.create", kind="codex")
            sid, generation = made["session"], made["generation"]
            conn.call(op="input", session=sid, generation=generation, request_id="boom", text="crash")
            wait_for(lambda: sessions(conn)[sid]["state"] == "disconnected")
            outcome = {}
            resumer = Conn(host.socket)
            thread = threading.Thread(target=lambda: outcome.update(resumer.call(op="session.resume", session=sid, generation=generation)))
            thread.start()
            time.sleep(0.6)                                                      # the new provider is starting: its answer is held back
            pids = lambda: [a["pid"] for a in conn.call(op="resources")["agents"] if a["session"] == sid]
            purged = conn.call(op="session.purge", session=sid, generation=generation + 1)
            thread.join(30)
            assert purged["ok"] == 1 and outcome.get("ok") == 0, (purged, outcome)    # the resume does not succeed on a deleted session
            time.sleep(0.5)
            assert sid not in sessions(conn) and not list((Path(directory) / "log").glob(f"{sid}.*"))
            assert not (Path(directory) / "workspaces" / sid).exists()
            assert any('"thread/resume"' in l for l in (Path(directory) / "received.jsonl").read_text().splitlines()), "the resume did start a provider"
            time.sleep(0.5)
            alive = []
            for entry in os.listdir("/proc"):
                if entry.isdigit() and int(entry) != host.pid:
                    try:
                        command = Path(f"/proc/{entry}/cmdline").read_bytes()
                        state = Path(f"/proc/{entry}/stat").read_text().rsplit(")", 1)[1].split()[0]
                    except OSError:
                        continue
                    if b"simulator.py" in command and str(directory).encode() in command and state != "Z":
                        alive.append(int(entry))
            assert alive == [], alive                                            # the provider the resume started was stopped, not left running
        finally:
            host.stop()


def test_a_session_whose_account_was_removed_with_detach_still_lets_the_host_restart_and_says_so():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            sid = made["session"]
            wait_for(lambda: sid in registered(directory))
            assert conn.call(op="account.remove", account=a, sessions="detach")["ok"] == 1
            time.sleep(1.5)                                                   # the registry is written about once a second
        finally:
            hard_stop(host)
        host, log, received = codex_host(directory)                          # before the repair the host refused to start here
        try:
            entry = sessions(Conn(host.socket))[sid]
            assert entry["state"] == "disconnected" and not entry.get("account") and entry["account_issue"] == "account_removed"
        finally:
            host.stop()


def test_purge_removes_the_set_aside_damaged_files_of_the_session_too():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            made = conn.call(op="session.create", kind="codex")
            sid = made["session"]
            conn.call(op="input", session=sid, generation=made["generation"], request_id="x", text="hello")
            wait_for(lambda: "echo: hello" in texts(log, sid))
            aside = Path(log) / f"{sid}.{0:020}.seg.damaged"
            aside.write_bytes(b"conversation bytes set aside earlier")
            assert conn.call(op="session.purge", session=sid, generation=made["generation"])["ok"] == 1
            assert not list(Path(log).glob(f"{sid}.*"))
        finally:
            host.stop()
