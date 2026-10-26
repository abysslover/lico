"""The rule for a session name, a folder path and the text sent to an agent is one rule in two places: the protocol (lico_protocol/v1.py) and the host (folders.rs
`valid_name` / `valid_path_shape`, ipc.rs `valid_text`). Both read tests/fixtures/text_rules.json, so changing one side alone fails here or in
`cargo test folders:: ipc::` (M02-P02-F4: the host refused C1 and the protocol accepted it, and a weaker host rule left every test green)."""
import json
from pathlib import Path

from lico_measure.keyring_rig import PrivateKeyring
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_protocol import v1

from tests.test_codex_adapter import codex_host

TABLE = json.loads((Path(__file__).parent / "fixtures" / "text_rules.json").read_text(encoding="utf-8"))
SESSION = "s0123456789abcdef"
RULES = ("name", "folder", "text", "label")


def entries(rule, kind):
    return [(e["text"] if "text" in e else e["unit"] * e["times"], e["why"]) for e in TABLE[rule][kind]]


def refused(rule, text):
    """The protocol check of a request that differs from a good one only in this field."""
    if rule == "name":
        return v1.validate_request("session.rename", {"v": 1, "session": SESSION, "name": text}) + v1.validate_request("session.create", {"v": 1, "kind": "claude", "name": text})
    if rule == "folder":
        return v1.validate_request("session.create", {"v": 1, "kind": "claude", "cwd": text})
    if rule == "label":
        return v1.validate_request("account.create", {"v": 1, "provider": "codex", "label": text})
    return v1.validate_request("input", {"v": 1, "session": SESSION, "generation": 1, "request_id": "r-1", "text": text})


def test_the_protocol_accepts_every_valid_entry_of_the_table():
    assert [(rule, why, refused(rule, text)) for rule in RULES for text, why in entries(rule, "valid") if refused(rule, text)] == []


def test_the_protocol_refuses_every_invalid_entry_of_the_table():
    assert [(rule, why) for rule in RULES for text, why in entries(rule, "invalid") if not refused(rule, text)] == []


def test_the_table_covers_the_groups_the_rule_names():
    for rule in ("name", "label"):
        refused_texts = {text for text, _ in entries(rule, "invalid")}
        for char in ("\x00", "\x7f", "\x85", "\u061c", "\u200b", "\u200e", "\u202e", "\u2028", "\u2060", "\u2064", "\u2066", "\u2069", "\ufeff", "\U000e0000", "\U000e007f"):
            assert len(char) == 1 and any(char in text for text in refused_texts), (rule, hex(ord(char)))
        assert {"\u200c", "\u200d"} <= {c for text, _ in entries(rule, "valid") for c in text}


def test_a_built_host_refuses_every_invalid_account_label_of_the_table_and_takes_every_valid_one():
    """V2: the rule of the protocol and the rule of the host are one rule for a label as they are for a name (the second reviewer made `account.create`
    answer ok for a label with a right-to-left override, a zero-width space, a byte order mark and an isolate)."""
    with PrivateKeyring(), private_dir() as directory:
        host, _log, _received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            made = {text: conn.call(op="account.create", provider="codex", label=text) for text, _ in entries("label", "valid") + entries("label", "invalid")}
            assert [why for text, why in entries("label", "invalid") if made[text]["ok"] == 1] == []
            assert [why for text, why in entries("label", "valid") if made[text]["ok"] != 1] == []
            listed = {a["label"] for a in conn.call(op="account.list")["accounts"]}
            assert listed == {text for text, _ in entries("label", "valid")}
        finally:
            host.stop()


def test_this_file_and_the_table_hold_no_raw_invisible_character():
    for path in (Path(__file__), Path(__file__).parent / "fixtures" / "text_rules.json"):
        assert [c for c in path.read_text(encoding="utf-8") if ord(c) > 127] == [], f"{path.name}: write them as \\u escapes"
