import os
import socket
import stat
import time

from lico_measure.rig import private_dir

from lico_measure.p02 import AGENT, HOSTD, ProtoConn as Conn, read_log, start  # noqa: F401


def test_output_persists_and_stays_sequenced_without_gui():
    with private_dir() as directory:
        host, log = start(directory, sessions=1, producing=1, rate=200, history=0, retain=100000)
        try:
            sid = host.sessions[0]
            gui = Conn(host.socket)                       # a GUI attaches, then vanishes
            gui.call(op="subscribe", session=sid, after=-1, snapshot=True)
            time.sleep(0.5)
            gui.close()
            time.sleep(2.5)                               # nobody is attached while output continues
            records = read_log(log, sid)
            seqs = [s for s, _ in records]
            assert len(seqs) >= 400, len(seqs)            # about 200 records/s for 3 s
            assert seqs == list(range(len(seqs))), "the durable log has a gap or a duplicate"
            assert all(len(p) == 1024 for _, p in records)
            assert stat.S_IMODE(os.stat(log).st_mode) == 0o700
            assert all(stat.S_IMODE(os.stat(f).st_mode) == 0o600 for f in log.iterdir())
            # a later view reattaches from its old cursor and sees exactly what followed
            back = Conn(host.socket)
            cursor = 100
            reply = back.call(op="subscribe", session=sid, after=cursor)
            assert reply["gap"] is False and reply["from"] == cursor + 1
            got = []
            while len(got) < 150:
                message = back.read()
                if message.get("ev") == "rec":
                    got.append(message["seq"])
            assert got == list(range(cursor + 1, cursor + 1 + len(got)))
        finally:
            host.stop()


def test_slow_subscriber_cannot_stall_host():
    with private_dir() as directory:
        host, log = start(directory, sessions=1, producing=1, rate=1000, history=0, retain=100000,
                          extra=["--queue-bytes", "65536"])
        try:
            sid = host.sessions[0]
            slow = Conn(host.socket)                      # subscribes and never reads again
            slow.call(op="subscribe", session=sid, after=-1, snapshot=True)
            fast = Conn(host.socket)
            fast.call(op="subscribe", session=sid, after=-1, snapshot=True)
            probe = Conn(host.socket)
            first = probe.call(op="status")["sessions"][0]["seq"]
            started = time.monotonic()
            seqs = []
            while time.monotonic() - started < 4.0:
                message = fast.read()
                if message.get("ev") == "rec":
                    seqs.append(message["seq"])
            elapsed = time.monotonic() - started
            last = probe.call(op="status")["sessions"][0]
            # the host kept producing at its own pace, and the attentive view got every record
            assert (last["seq"] - first) / elapsed > 800, (last["seq"] - first, elapsed)
            assert seqs == list(range(seqs[0], seqs[0] + len(seqs)))
            assert last["subscribers"] == 1               # only the stalled view was dropped
            # the stalled view is told to resynchronise instead of being left stuck
            slow.sock.settimeout(5)
            events = []
            try:
                while True:
                    events.append(slow.read())
                    if events[-1].get("ev") == "overflow":
                        break
            except (EOFError, socket.timeout):
                pass
            assert events and events[-1].get("ev") == "overflow"
            assert host.proc.poll() is None
        finally:
            host.stop()


def test_retention_boundary_reports_gap_and_snapshot():
    with private_dir() as directory:
        host, log = start(directory, sessions=1, producing=0, history=1000, retain=100)
        try:
            sid = host.sessions[0]
            conn = Conn(host.socket)
            status = conn.call(op="status")["sessions"][0]
            oldest, last = status["oldest"], status["seq"]
            # at least `retain` records are kept, trimmed in whole segments of retain/4
            assert last == 999 and 100 <= last - oldest + 1 <= 125, status
            files = list(log.glob(f"{sid}.*.seg"))
            assert len(read_log(log, sid)) <= 125                     # disk retention is bounded too
            assert files, "nothing was persisted"

            def subscribe(cursor):
                c = Conn(host.socket)
                reply = c.call(op="subscribe", session=sid, after=cursor)
                c.close()
                return reply

            inside = subscribe(oldest - 1)                  # the first retained record is still replayable
            assert inside["gap"] is False and inside["from"] == oldest
            outside = subscribe(oldest - 2)                 # one record older: the host cannot replay it
            assert outside["gap"] is True and outside["from"] == outside["snapshot"]["seq"] + 1
            assert subscribe(last)["gap"] is False          # nothing missed
            assert subscribe(last + 5)["gap"] is True       # a cursor from the future is not trusted
        finally:
            host.stop()
