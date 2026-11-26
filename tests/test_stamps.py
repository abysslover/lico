"""M24-P11a: the host remembers when each record of a session was written, and says so on request (`stamps`).

The log of a session holds no time and the lines of the protocol are `[sequence, text]`, so the times live beside the log folder (`<log>.times/<id>.times`)
and are asked for on their own. These tests run the real host with the simulated codex provider.
"""
import os
import struct
import sys
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, start
from lico_measure.rig import private_dir
from tests.test_accounts import account
from tests.test_codex_adapter import FIXTURE, SIM, codex_host, texts, usual_umask, wait_for
from tests.test_recovery import hard_stop, registered, sessions


def say(conn, sid, generation, k, text):
    conn.call(op="input", session=sid, generation=generation, request_id=f"t-{k}", text=text)
    wait_for(lambda: f"echo: {text}" in texts(LOG[0], sid) and "busy_since" not in sessions(conn)[sid])


LOG = [None]


def times_dir(directory):
    return Path(directory) / "log.times"


def runs_of(conn, sid, start=0, until=100000):
    reply = conn.call(op="stamps", session=sid, **{"from": start}, until=until)
    assert reply["ok"] == 1, reply
    return reply


def test_every_second_of_activity_is_a_run_and_the_reply_says_which_lines_it_covers():
    with private_dir() as directory:
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            sid, generation = made["session"], made["generation"]
            first = int(time.time())
            say(conn, sid, generation, 0, "one")
            time.sleep(1.2)
            say(conn, sid, generation, 1, "two")
            last = int(time.time())
            end = sessions(conn)[sid]["seq"] + 1
            reply = runs_of(conn, sid, 0, end)
            runs = reply["stamps"]
            assert (reply["session"], reply["from"], reply["until"]) == (sid, 0, end)
            assert runs and runs[0][0] == 0, "the first record of the session begins the first run"
            assert [r[0] for r in runs] == sorted(set(r[0] for r in runs)), "the runs go forward by sequence"
            assert all(first - 2 <= secs <= last + 1 for _, secs in runs), (runs, first, last)
            assert len(runs) >= 2 and runs[-1][1] - runs[0][1] >= 1, "a second without records and then more makes another run"
            assert len(runs) < end, "lines of one second share a run: there are fewer runs than lines"
        finally:
            hard_stop(host)


def test_the_range_asked_for_decides_which_runs_come_back():
    with private_dir() as directory:
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            sid, generation = made["session"], made["generation"]
            say(conn, sid, generation, 0, "one")
            time.sleep(1.2)
            say(conn, sid, generation, 1, "two")
            end = sessions(conn)[sid]["seq"] + 1
            every = runs_of(conn, sid, 0, end)["stamps"]
            assert len(every) >= 2
            second = every[1][0]
            assert second >= 2, "the first run holds more than one line"
            assert runs_of(conn, sid, 1, end)["stamps"] == every, "from inside the first run: the run that covers it comes first"
            assert runs_of(conn, sid, second - 1, end)["stamps"] == every, "from the last line of the first run"
            assert runs_of(conn, sid, second, end)["stamps"] == every[1:], "from a line that begins a run: that run and the later ones"
            assert runs_of(conn, sid, 0, second)["stamps"] == every[:1], "until is exclusive: a run that begins at `until` is not in"
            assert runs_of(conn, sid, 5, 5)["stamps"] == [], "an empty range"
            assert runs_of(conn, sid, end + 1000, end + 2000)["stamps"][:1] == every[-1:], "past the end the last run still covers"
        finally:
            hard_stop(host)


def test_the_times_are_in_a_file_beside_the_log_folder_owner_only_and_survive_a_crash():
    with private_dir() as directory:
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            sid, generation = made["session"], made["generation"]
            say(conn, sid, generation, 0, "one")
            time.sleep(1.2)
            say(conn, sid, generation, 1, "two")
            end = sessions(conn)[sid]["seq"] + 1
            before = runs_of(conn, sid, 0, end)["stamps"]
            wait_for(lambda: (times_dir(directory) / f"{sid}.times").exists())
            file = times_dir(directory) / f"{sid}.times"
            assert file.stat().st_size == 16 * len(before)
            assert (file.stat().st_mode & 0o777, times_dir(directory).stat().st_mode & 0o777) == (0o600, 0o700)
            assert not [p for p in log.iterdir() if p.suffix == ".times" or p.is_dir()], "the log folder holds nothing new, so an older build still starts on it"
        finally:
            hard_stop(host)
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            assert runs_of(conn, sid, 0, end)["stamps"] == before, "the same runs after a restart"
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            time.sleep(1.2)                                                         # so that the new lines fall in a second of their own and begin a run
            say(conn, sid, resumed["generation"], 2, "three")
            after = runs_of(conn, sid, 0, 100000)["stamps"]
            assert after[: len(before)] == before
            assert after[-1][0] >= end, "the new lines are timed too"
        finally:
            hard_stop(host)


def test_a_log_without_the_file_answers_no_time_for_its_old_lines_and_a_damaged_file_is_repaired():
    with private_dir() as directory:
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            sid, generation = made["session"], made["generation"]
            say(conn, sid, generation, 0, "one")
            end = sessions(conn)[sid]["seq"] + 1
            wait_for(lambda: (times_dir(directory) / f"{sid}.times").exists())
        finally:
            hard_stop(host)
        file = times_dir(directory) / f"{sid}.times"
        whole = file.read_bytes()
        file.write_bytes(whole + b"\x01\x02\x03")                                   # a torn tail
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            assert runs_of(conn, sid, 0, end)["stamps"][0][0] == 0
            assert file.read_bytes() == whole, "the torn tail was cut off"
            assert file.stat().st_mode & 0o777 == 0o600, "the repaired file is the owner's alone"
        finally:
            hard_stop(host)
        file.unlink()                                                               # what a log made by an older build looks like
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            assert runs_of(conn, sid, 0, end)["stamps"] == [], "no file: no time for the old lines, and no error"
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            say(conn, sid, resumed["generation"], 1, "two")
            runs = runs_of(conn, sid, 0, 100000)["stamps"]
            assert runs and runs[0][0] >= end, "only the lines written since have a time"
        finally:
            hard_stop(host)


def test_a_purge_removes_the_file_and_the_start_removes_the_files_of_sessions_nobody_knows():
    with private_dir() as directory:
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            kept = conn.call(op="session.create", kind="codex", account=a)
            say(conn, made["session"], made["generation"], 0, "one")
            say(conn, kept["session"], kept["generation"], 0, "one")
            wait_for(lambda: all((times_dir(directory) / f"{s['session']}.times").exists() for s in (made, kept)))
            assert conn.call(op="session.purge", session=made["session"], generation=made["generation"])["ok"] == 1
            assert not (times_dir(directory) / f"{made['session']}.times").exists(), "the file of a deleted session goes with it"
            assert (times_dir(directory) / f"{kept['session']}.times").exists()
            wait_for(lambda: kept["session"] in registered(directory))
        finally:
            hard_stop(host)
        stray = times_dir(directory) / "zzzzzzzz.times"
        stray.write_bytes(b"\0" * 16)
        foreign = times_dir(directory) / "notes.txt"
        foreign.write_text("mine")
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            wait_for(lambda: not stray.exists())
            assert foreign.read_text() == "mine", "what is not a times file is not touched"
            assert (times_dir(directory) / f"{kept['session']}.times").exists()
        finally:
            hard_stop(host)


def test_a_bad_request_is_refused_and_an_unknown_session_is_said():
    with private_dir() as directory:
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            sid = conn.call(op="session.create", kind="codex", account=a)["session"]
            assert "stamps" in conn.call(op="hello")["ops"]
            assert conn.call(op="stamps", session="nope", **{"from": 0}, until=10)["error"] == "unknown_session"
            for bad in ({"from": -1, "until": 5}, {"from": 0, "until": -1}, {"from": "0", "until": 5}, {"from": 0}, {"until": 5}, {"from": 0, "until": 5, "extra": 1}, {"from": 1.5, "until": 5}):
                assert conn.call(op="stamps", session=sid, **bad)["error"] == "bad_request", bad
            assert conn.call(op="stamps", **{"from": 0}, until=5)["error"] == "bad_request"
            reply = runs_of(conn, sid, 0, 10 ** 12)
            assert len(reply["stamps"]) <= 2048
        finally:
            hard_stop(host)


def small_retain_host(directory, retain):
    """The host of `codex_host` with a log that keeps only about `retain` records (segments of 16), so that its oldest segment goes soon."""
    received = str(Path(directory) / "received.jsonl")
    extra = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", str(SIM), "--codex-arg", "--received", "--codex-arg", received, "--codex-arg", "--fixture", "--codex-arg", str(FIXTURE)]
    with usual_umask():
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=retain)
    return host, log


def test_the_runs_of_what_the_log_dropped_are_dropped_with_it_and_the_run_that_covers_the_oldest_line_stays():
    with private_dir() as directory:
        host, log = small_retain_host(directory, 32)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            sid, generation = made["session"], made["generation"]
            for k in range(16):
                say(conn, sid, generation, k, f"turn {k}")
                time.sleep(0.35)
            oldest = conn.call(op="history", session=sid, before=1, limit=1)["oldest"]
            assert oldest > 0, "the log dropped its first segment"
            runs = runs_of(conn, sid, 0, 100000)["stamps"]
            assert len(runs) >= 3, runs
            assert runs[0][0] <= oldest < runs[1][0], (runs[:3], oldest)
        finally:
            hard_stop(host)


def cut_log_to(log, sid, keep):
    """Cuts the (only) segment of the session after its first `keep` records, at a record's end: what a log looks like that lost its last records."""
    (path,) = sorted(log.glob(f"{sid}.*.seg"))
    data = path.read_bytes()
    at = 0
    for _ in range(keep):
        length, _seq = struct.unpack_from("<IQ", data, at)
        at += 12 + length
    path.write_bytes(data[:at])


def test_the_runs_of_records_the_log_lost_are_forgotten_at_a_restart_and_the_next_records_are_timed_again():
    with private_dir() as directory:
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            sid, generation = made["session"], made["generation"]
            say(conn, sid, generation, 0, "one")
            time.sleep(1.2)
            say(conn, sid, generation, 1, "two")
            runs_before = runs_of(conn, sid, 0, 100000)["stamps"]
            assert len(runs_before) >= 2, "the second input began a run after the first"
            kept = runs_before[-1][0] + 1                                           # the log is cut right after the first record of that run: the run stays, the next one goes
            time.sleep(1.2)
            say(conn, sid, generation, 2, "three")
            runs_all = runs_of(conn, sid, 0, 100000)["stamps"]
            assert runs_all[: len(runs_before)] == runs_before and runs_all[-1][0] > kept, "the third input began a run past the cut"
            wait_for(lambda: (times_dir(directory) / f"{sid}.times").stat().st_size >= 16 * len(runs_all))
        finally:
            hard_stop(host)
        cut_log_to(log, sid, kept)
        host, log, _ = codex_host(directory)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            assert sessions(conn)[sid]["seq"] + 1 == kept
            runs = runs_of(conn, sid, 0, 100000)["stamps"]
            assert runs == runs_before, "the runs up to the cut stay (the one that begins at the last record too) and the later ones are forgotten"
            assert (times_dir(directory) / f"{sid}.times").stat().st_size == 16 * len(runs), "the file does not keep them either"
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            time.sleep(1.2)
            say(conn, sid, resumed["generation"], 3, "four")
            again = runs_of(conn, sid, 0, 100000)["stamps"]
            assert again[: len(runs)] == runs and again[-1][0] >= kept, "the records that take the numbers again have their own time"
        finally:
            hard_stop(host)


def test_a_log_with_no_recorded_times_answers_an_estimate_from_the_times_of_its_own_files_and_marks_where_the_recorded_ones_begin():
    # M25-P10, the owner's "여전히 ... 년월일시분초가 보이지 않음": the lines of a session written before the times were kept are placed between the writes of the log's files
    with private_dir() as directory:
        host, log = small_retain_host(directory, 64)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            a = account(conn, "Work")
            made = conn.call(op="session.create", kind="codex", account=a)
            sid, generation = made["session"], made["generation"]
            for k in range(12):
                say(conn, sid, generation, k, f"turn {k}")
                if len(sorted(log.glob(f"{sid}.*.seg"))) >= 3:
                    break
            end = sessions(conn)[sid]["seq"] + 1
            wait_for(lambda: (times_dir(directory) / f"{sid}.times").exists())
            fresh = runs_of(conn, sid, 0, end)
            assert (fresh["exact_from"], fresh["approx"]) == (0, []), "a session of this build has the exact time of every line: nothing is estimated"
        finally:
            hard_stop(host)
        segments = sorted(log.glob(f"{sid}.*.seg"))
        assert len(segments) >= 3, segments
        (times_dir(directory) / f"{sid}.times").unlink()                              # what a log made before the times were kept looks like
        base = int(time.time()) - 10 * 86400
        for index, segment in enumerate(segments):
            os.utime(segment, (base + 1000 * index, base + 1000 * index))              # each file was last written 1000 seconds after the one before
        host, log = small_retain_host(directory, 64)
        LOG[0] = log
        try:
            conn = Conn(host.socket)
            old = runs_of(conn, sid, 0, end)
            assert old["stamps"] == [] and old["exact_from"] is None, "nothing was recorded: no first recorded line"
            approx = old["approx"]
            assert approx[0] == [0, base], (approx, base)
            assert [r[0] for r in approx] == sorted(set(r[0] for r in approx)) and [r[1] for r in approx] == sorted(r[1] for r in approx), "forward in both"
            assert approx[-1][1] == base + 1000 * (len(segments) - 1), "the last lines are at the last write of the last file"
            second = int(segments[1].name.split(".")[1])
            assert any(r[0] >= second and base < r[1] <= base + 1000 for r in approx), "the lines of the second file lie between the writes of the first and of the second"
            assert len(approx) <= 32 * len(segments) and len(approx) + len(old["stamps"]) <= 2048
            middle = runs_of(conn, sid, second + 3, second + 6)
            assert middle["approx"] and middle["approx"][0][0] <= second + 3, "the run that covers the first line asked for comes first"
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            say(conn, sid, resumed["generation"], 99, "after")
            both = runs_of(conn, sid, 0, 100000)
            assert both["stamps"] and both["exact_from"] == both["stamps"][0][0] >= end, "the lines written since are recorded and say where"
            assert both["approx"] and all(r[0] < both["exact_from"] and r[1] <= both["stamps"][0][1] for r in both["approx"]), "the estimate stays below the first recorded line"
        finally:
            hard_stop(host)
