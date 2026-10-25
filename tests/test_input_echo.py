"""P02 S02b: an input the host accepted shows in the history as a short folded echo that starts with ">> "."""
import json

from lico_measure.p02 import ProtoConn as Conn, cli, read_log, start
from lico_measure.rig import private_dir
from lico_protocol import v1

from tests.test_claude_adapter import claude_host, new_claude, sent_users, status_of, user_text
from tests.test_codex_adapter import codex_host, new_session, wait_for
from tests.test_folders import long_text
from tests.test_recovery import hard_stop

FOLD_LINES, FOLD_BYTES = 12, 2048          # the host's ECHO_LINES and ECHO_BYTES


def echoes_of(log, sid):
    """The echo records in the durable log, in order, and where each stands among all records."""
    return [(n, p.decode()) for n, (_, p) in enumerate(read_log(log, sid)) if p.startswith(b">> ")]


def synthetic(directory):
    host, log = start(directory, sessions=1, producing=0, history=0)
    return Conn(host.socket), host.sessions[0], 1, log, host.stop


def claude(directory):
    host, log, _ = claude_host(directory)
    conn = Conn(host.socket)
    sid, generation = new_claude(conn, mode="default")
    return conn, sid, generation, log, host.stop


def codex(directory):
    host, log, _ = codex_host(directory)
    conn, sid, generation = new_session(host)
    return conn, sid, generation, log, host.stop


def test_an_accepted_input_is_echoed_once_before_the_answer_for_all_three_kinds_and_a_refused_one_not_at_all():
    for name, make in (("synthetic", synthetic), ("claude", claude), ("codex", codex)):
        with private_dir() as directory:
            conn, sid, generation, log, stop = make(directory)
            try:
                give = lambda rid, text, gen=generation: conn.call(op="input", session=sid, generation=gen, request_id=rid, text=text)
                assert give("e-1", "hello 한글")["ok"] == 1, name
                assert give("e-1", "hello 한글")["duplicate"] is True, name                  # a retry of the same request id is not echoed again
                assert give("e-2", "two\nlines\n")["ok"] == 1, name
                assert give("e-3", "a\x01b")["error"] == "bad_request" and give("e-4", "")["error"] == "bad_request"
                assert give("e-5", "stale", generation + 1)["ok"] == 0
                wait_for(lambda: len(echoes_of(log, sid)) >= 3)
                assert [text for _, text in echoes_of(log, sid)] == [">> hello 한글", ">> two", ">> lines"], name
                if name == "synthetic":                                                      # the agent's answer comes after the echo, never before
                    answered = lambda: [n for n, (_, p) in enumerate(read_log(log, sid)) if p.startswith(b"echo ")]
                    assert min(wait_for(answered)) > echoes_of(log, sid)[0][0]
            finally:
                stop()


def test_a_64_kib_input_gives_twelve_lines_and_one_marker_with_exact_counts_and_a_single_long_line_is_cut_with_a_marker():
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn, mode="default")
            text = long_text(v1.MAX_INPUT_TEXT)
            assert conn.call(op="input", session=sid, generation=generation, request_id="big-1", text=text)["ok"] == 1
            wait_for(lambda: len(echoes_of(log, sid)) >= FOLD_LINES + 1)
            lines = text.split("\n")
            shown = sum(len(line.encode()) + 1 for line in lines[:FOLD_LINES])
            total = text.count("\n") + (0 if text.endswith("\n") else 1)
            got = [t for _, t in echoes_of(log, sid)]
            assert got == [f">> {line.replace(chr(9), chr(32))}" for line in lines[:FOLD_LINES]] + [
                f">> [{total - FOLD_LINES} more lines, {len(text.encode()) - shown} bytes not shown; the agent received all of it]"], got[-1]
            assert [user_text(m) for m in sent_users(received)] == [text]          # the agent still gets the whole text, not the fold
            # one very long line: cut at the byte cap on a character, and the marker says so
            line = "é" * (v1.MAX_INPUT_TEXT // 2)
            assert conn.call(op="input", session=sid, generation=generation, request_id="big-2", text=line)["ok"] == 1
            wait_for(lambda: len(echoes_of(log, sid)) >= FOLD_LINES + 3)
            got = [t for _, t in echoes_of(log, sid)][FOLD_LINES + 1:]
            assert got == [f">> {'é' * (FOLD_BYTES // 2)}",
                           f">> [a long line is cut after {FOLD_BYTES} bytes; {len(line.encode()) - FOLD_BYTES} bytes not shown; the agent received all of it]"]
            # what sanitize removes from every other record is removed from the echo too, and the agent still gets the original bytes
            odd = "a\tb\u202ec\u2028d"
            assert conn.call(op="input", session=sid, generation=generation, request_id="big-3", text=odd)["ok"] == 1
            wait_for(lambda: len(echoes_of(log, sid)) >= FOLD_LINES + 4)
            assert echoes_of(log, sid)[-1][1] == ">> a bcd"
            assert [user_text(m) for m in sent_users(received)][-1] == odd
        finally:
            host.stop()


def test_the_echo_survives_a_host_restart_and_a_second_reader_sees_it_in_the_history():
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn, mode="default")
            assert conn.call(op="input", session=sid, generation=generation, request_id="r-1", text="remember me\nsecond line")["ok"] == 1
            wait_for(lambda: len(echoes_of(log, sid)) >= 2)
            assert status_of(conn, sid)["seq"] >= 1
        finally:
            hard_stop(host)
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            snap = conn.call(op="snapshot", session=sid)
            assert [line[1] for line in snap["lines"] if line[1].startswith(">> ")] == [">> remember me", ">> second line"]
            asked = cli(host, "history", sid, str(status_of(conn, sid)["seq"] + 1), "50")      # the command line is a second reader
            assert asked.returncode == 0, asked.stderr
            assert [r[1] for r in json.loads(asked.stdout)["lines"] if r[1].startswith(">> ")] == [">> remember me", ">> second line"]
        finally:
            host.stop()
