"""The model name rule of the protocol (lico_protocol/v1.py) and of the host (folders.rs, `valid_model`) are one rule: both read
tests/fixtures/model_names.json, so changing one side alone fails here or in `cargo test folders::`."""
import json
from pathlib import Path

from lico_protocol import v1

TABLE = json.loads((Path(__file__).parent / "fixtures" / "model_names.json").read_text(encoding="utf-8"))


def names(kind):
    return [(e["text"] if "text" in e else e["unit"] * e["times"], e["why"]) for e in TABLE[kind]]


def refused(text):
    """The protocol check of a `session.create` that differs from a valid one only in the model."""
    return v1.validate_request("session.create", {"v": 1, "kind": "claude", "model": text})


def test_the_protocol_accepts_every_valid_model_name_of_the_table():
    assert [(why, refused(text)) for text, why in names("valid") if refused(text)] == []


def test_the_protocol_refuses_every_invalid_model_name_of_the_table():
    assert [why for text, why in names("invalid") if not refused(text)] == []


def test_the_table_has_both_sides_of_the_limit():
    lengths = {len(text) for text, _ in names("valid") + names("invalid")}
    assert {60, 61} <= lengths and len(names("valid")) >= 8 and len(names("invalid")) >= 20
