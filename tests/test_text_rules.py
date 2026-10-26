"""The rule for a session name, a folder path and the text sent to an agent is one rule in two places: the protocol (lico_protocol/v1.py) and the host (folders.rs
`valid_name` / `valid_path_shape`, ipc.rs `valid_text`). Both read tests/fixtures/text_rules.json, so changing one side alone fails here or in
`cargo test folders:: ipc::` (M02-P02-F4: the host refused C1 and the protocol accepted it, and a weaker host rule left every test green)."""
import json
from pathlib import Path

from lico_protocol import v1

TABLE = json.loads((Path(__file__).parent / "fixtures" / "text_rules.json").read_text(encoding="utf-8"))
SESSION = "s0123456789abcdef"


def entries(rule, kind):
    return [(e["text"] if "text" in e else e["unit"] * e["times"], e["why"]) for e in TABLE[rule][kind]]


def refused(rule, text):
    """The protocol check of a request that differs from a good one only in this field."""
    if rule == "name":
        return v1.validate_request("session.rename", {"v": 1, "session": SESSION, "name": text}) + v1.validate_request("session.create", {"v": 1, "kind": "claude", "name": text})
    if rule == "folder":
        return v1.validate_request("session.create", {"v": 1, "kind": "claude", "cwd": text})
    return v1.validate_request("input", {"v": 1, "session": SESSION, "generation": 1, "request_id": "r-1", "text": text})


def test_the_protocol_accepts_every_valid_entry_of_the_table():
    assert [(rule, why, refused(rule, text)) for rule in ("name", "folder", "text") for text, why in entries(rule, "valid") if refused(rule, text)] == []


def test_the_protocol_refuses_every_invalid_entry_of_the_table():
    assert [(rule, why) for rule in ("name", "folder", "text") for text, why in entries(rule, "invalid") if not refused(rule, text)] == []


def test_the_table_covers_the_groups_the_rule_names():
    names = {text for text, _ in entries("name", "invalid")}
    for char in ("\x00", "\x7f", "\x85", "\u061c", "\u200b", "\u200e", "\u202e", "\u2028", "\u2060", "\u2064", "\u2066", "\u2069", "\ufeff", "\U000e0000", "\U000e007f"):
        assert len(char) == 1 and any(char in text for text in names), hex(ord(char))
    assert {"\u200c", "\u200d"} <= {c for text, _ in entries("name", "valid") for c in text}


def test_this_file_and_the_table_hold_no_raw_invisible_character():
    for path in (Path(__file__), Path(__file__).parent / "fixtures" / "text_rules.json"):
        assert [c for c in path.read_text(encoding="utf-8") if ord(c) > 127] == [], f"{path.name}: write them as \\u escapes"
