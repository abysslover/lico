"""M02-P02-F3: a line of agent text that begins like a record of the host (`>> `, `$ `, `> `, a bracketed note such as `[turn completed]`) comes out of both adapters
with `[agent] ` in front, once, whether the text arrives whole or in pieces and whether invisible characters or white space stand before the lead. The text is kept.
The simulators say exactly what the test gives them (mode `say:`)."""
import json
from pathlib import Path

import pytest

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import claude_host, new_claude, records
from tests.test_codex_adapter import codex_host, new_session, texts, wait_for

TAG = "[agent] "
# tests/fixtures/provider_text.json: a line of agent text and the record it becomes. The same table is read by a unit test of leads.rs and by the test of the
# reference mapping, so the program and the reference cannot drift apart.
TABLE = json.loads((Path(__file__).parent / "fixtures" / "provider_text.json").read_text(encoding="utf-8"))


def value(v):
    return v if isinstance(v, str) else v["prefix"] + v["unit"] * v["times"]


LINES = [(value(r["line"]), value(r["record"])) for r in TABLE["rows"]]
LONG = [(value(r["line"]), value(r["record"])) for r in TABLE["long"]]


def expected():
    return [record for _, record in LINES]


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
    assert expected()[-1].startswith(TAG), "the last row must be a lead, or the write at the end of an item is not covered (V3, mutation W2)"
    text = "\n".join(line for line, _ in LINES)                     # the last line has no line end: it is written when the item ends
    assert codex_records([text[k:k + 3] for k in range(0, len(text), 3)]) == expected()


def test_claude_agent_text_that_begins_like_a_host_record_is_tagged_in_one_message_and_in_many():
    assert claude_records(["\n".join(line for line, _ in LINES)]) == expected()
    assert claude_records([line for line, _ in LINES]) == expected()


@pytest.mark.parametrize("adapter", [codex_records, claude_records])
@pytest.mark.parametrize("row", range(len(LONG)))
def test_the_record_bound_counts_the_tag(adapter, row):
    line, record = LONG[row]
    assert adapter([line + "\n"] if adapter is codex_records else [line]) == [record]
