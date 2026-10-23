"""Deterministic synthetic output records for the architecture experiment.

The proposed stream is 1 KiB records at 100 records/s for 30 s. A record is a
pure function of its sequence number, so a cursor can resume without a gap or a
duplicate and two runs produce identical bytes.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from typing import Iterator

RECORD_BYTES = 1024
RATE_PER_SECOND = 100
DURATION_SECONDS = 30


@dataclass(frozen=True)
class Record:
    seq: int
    payload: bytes


def record(seq: int) -> Record:
    if seq < 0:
        raise ValueError("sequence numbers start at 0")
    block = hashlib.sha256(seq.to_bytes(8, "big")).digest()
    payload = (block * (RECORD_BYTES // len(block) + 1))[:RECORD_BYTES]
    return Record(seq, payload)


def records(start: int, count: int) -> Iterator[Record]:
    if count < 0:
        raise ValueError("count must not be negative")
    for seq in range(start, start + count):
        yield record(seq)
