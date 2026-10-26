"""M02-P02-F4b: the reference mappings (lico_protocol/codex_mapping.py and claude_mapping.py) are what the host's adapters are tested against. They give the
records tests/fixtures/provider_text.json lists for lines of provider text: cleaned, and tagged `[agent] ` when a line begins like a record of the host. The same
file is read by a unit test of leads.rs and by tests/test_forged_leads.py (the built program), so the reference and the program cannot drift apart."""
import json

from lico_measure.rig import REPO
from lico_protocol.claude_mapping import map_stream
from lico_protocol.codex_mapping import Mapper, agent_record

TABLE = json.loads((REPO / "tests" / "fixtures" / "provider_text.json").read_text(encoding="utf-8"))


def value(v):
    return v if isinstance(v, str) else v["prefix"] + v["unit"] * v["times"]


ROWS = [(value(r["line"]), value(r["record"])) for r in TABLE["rows"]]
LONG = [(value(r["line"]), value(r["record"])) for r in TABLE["long"]]


def codex_records(chunks, command=False):
    kind, method = ("commandExecution", "item/commandExecution/outputDelta") if command else ("agentMessage", "item/agentMessage/delta")
    steps = [{"dir": "server", "msg": {"method": method, "params": {"itemId": "i1", "delta": chunk}}} for chunk in chunks]
    steps.append({"dir": "server", "msg": {"method": "item/completed", "params": {"item": {"id": "i1", "type": kind, "exitCode": 0}}}})
    return Mapper().feed(steps).records


def test_the_table_is_ascii_and_covers_what_it_claims():
    text = (REPO / "tests" / "fixtures" / "provider_text.json").read_text(encoding="utf-8")
    assert all(ord(c) < 128 for c in text), "the characters are written as \\u escapes"
    tagged = [line for line, record in ROWS if record.startswith("[agent] ") and not line.startswith("[agent] ")]
    kept = {c for line, record in ROWS for c in record}
    assert len(tagged) >= 40 and {"\u200c", "\u200d"} <= kept, "U+200C and U+200D stay in a record"
    assert any("[log recovered:" in line for line in tagged) and any(line == "> > x" for line in tagged) and any(line == "a>> x" for line, _ in ROWS)
    for wanted in "\u0000\u007f\u0085\u061c\u200b\u200e\u2028\u202e\u2060\u2064\u2066\u2069\ufeff\U000e0041\u001b":
        assert any(wanted in line for line, _ in ROWS), f"no row with U+{ord(wanted):04X}"


def test_one_line_of_agent_text_becomes_the_record_of_the_table():
    wrong = [(line, agent_record(line), record) for line, record in ROWS + LONG if agent_record(line) != record]
    assert wrong == []


def test_the_codex_mapping_gives_the_records_for_agent_text_whole_and_in_pieces_and_for_command_output():
    text = "\n".join(line for line, _ in ROWS)
    want = [record for _, record in ROWS]
    assert codex_records([text + "\n"]) == want
    assert codex_records([text[k:k + 3] for k in range(0, len(text), 3)]) == want          # the last line has no line end: it is written when the item ends
    assert codex_records([text + "\n"], command=True) == want + ["[exit 0]"]


def test_the_claude_mapping_gives_the_records_for_assistant_text_in_one_message_and_in_many():
    def assistant(*texts):
        return [{"type": "assistant", "message": {"content": [{"type": "text", "text": t} for t in texts]}}]
    want = [record for _, record in ROWS]
    assert map_stream(assistant("\n".join(line for line, _ in ROWS))).records == want
    assert map_stream(assistant(*[line for line, _ in ROWS])).records == want


def test_the_text_a_person_or_the_host_wrote_is_not_tagged():
    messages = [{"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Bash", "input": {"command": "ls"}}]}},
                {"type": "user", "message": {"content": [{"type": "tool_result", "content": ">> x"}]}}]
    assert map_stream(messages).records == ["$ ls", "> >> x"]            # the lead is the host's; what follows it is the output
    steps = [{"dir": "server", "msg": {"method": "item/started", "params": {"item": {"type": "commandExecution", "command": ">> x", "id": "c"}}}}]
    assert Mapper().feed(steps).records == ["$ >> x"]
