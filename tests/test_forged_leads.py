"""M02-P02-F3: a line of agent text that begins like a record of the host (`>> `, `$ `, `> `, a bracketed note such as `[turn completed]`) comes out of both adapters
with `[agent] ` in front, once, whether the text arrives whole or in pieces and whether invisible characters or white space stand before the lead. The text is kept.
The simulators say exactly what the test gives them (mode `say:`)."""
import json

import pytest

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import claude_host, new_claude, records
from tests.test_codex_adapter import codex_host, new_session, texts, wait_for

TAG = "[agent] "
# (a line of agent text, whether it begins like a record of the host)
LINES = [
    (">> x", True), ("$ x", True), ("> x", True), ("[turn completed]", True), ("[session closed]", True), ("[provider reconnected]", True), ("[session started] model=m", True),
    ("[turn started]", True), ("[exit 0]", True), ("[3 more lines, 10 bytes not shown; the agent received all of it]", True),
    ("[log recovered: 12 byte(s) of a damaged tail and 1 segment(s) set aside]", True),
    ("\u00ad>> x", True), (">\u00ad> x", True), ("\u3164>> x", True), ("\u00a0$ x", True), ("\u2003[turn completed]", True), ("\ufe0f> x", True),
    ("\u200b>> x", True), ("\ufeff$ x", True), ("\U000e0041> x", True), ("\u200c$ x", True),            # the first two and the tag character are removed by the cleaning, the joiner stays
    ("> > x", True),                                                                                  # the `> ` lead with the text `> x`, not `>> x`
    ("[agent] >> x", False), ("a>> x", False), (">>x", False), ("$HOME is set", False), ("[1] first", False), ("plain text", False),
    ("> the last line, which has no line end when it arrives in pieces", True),
]


def shown(line):
    """The record a line becomes: the cleaning removes the zero-width space, the byte order mark and the tag character before the decision."""
    kept = "".join(c for c in line if c not in "\u200b\ufeff\U000e0041")
    return (TAG + kept) if dict(LINES)[line] else kept


def expected():
    return [shown(line) for line, _ in LINES]


def codex_records(chunks):
    with private_dir() as directory:
        host, log, _ = codex_host(directory)
        try:
            conn, sid, generation = new_session(host)
            assert conn.call(op="input", session=sid, generation=generation, request_id="r-1", text="say:" + json.dumps(chunks))["ok"] == 1
            wait_for(lambda: texts(log, sid)[-1:] == ["[turn completed]"])
            got = texts(log, sid)
        finally:
            host.stop()
    assert got.count("[turn completed]") == 1, "a forged note must not look like the host's own"
    return got[got.index("[turn started]") + 1:-1]


def claude_records(messages):
    with private_dir() as directory:
        host, log, _ = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            assert conn.call(op="input", session=sid, generation=generation, request_id="r-1", text="say:" + json.dumps(messages))["ok"] == 1
            wait_for(lambda: records(log, sid)[-1:] == ["[turn completed]"])
            got = records(log, sid)
        finally:
            host.stop()
    assert got.count("[turn completed]") == 1
    return got[next(k for k, r in enumerate(got) if r.startswith("[session started]")) + 1:-1]


def test_codex_agent_text_that_begins_like_a_host_record_is_tagged_whole():
    assert codex_records(["\n".join(line for line, _ in LINES) + "\n"]) == expected()


def test_codex_agent_text_that_arrives_in_pieces_is_tagged_when_the_line_is_whole():
    text = "\n".join(line for line, _ in LINES)                     # the last line has no line end: it is written when the item ends
    assert codex_records([text[k:k + 3] for k in range(0, len(text), 3)]) == expected()


def test_claude_agent_text_that_begins_like_a_host_record_is_tagged_in_one_message_and_in_many():
    assert claude_records(["\n".join(line for line, _ in LINES)]) == expected()
    assert claude_records([line for line, _ in LINES]) == expected()


@pytest.mark.parametrize("adapter", [codex_records, claude_records])
def test_the_record_bound_counts_the_tag(adapter):
    got = adapter(["$ " + "a" * 5000 + "\n"] if adapter is codex_records else ["$ " + "a" * 5000])
    assert got == [(TAG + "$ " + "a" * 5000)[:4096]], [len(r) for r in got]
