"""Process-tree resource scopes read from /proc.

Each category (for example "gui" or "host") is a root process plus every
descendant. RSS and PSS are kept separate; a process belonging to two
categories is refused instead of being counted twice.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field


class ScopeError(RuntimeError):
    """The requested categories do not describe disjoint process trees."""


@dataclass(frozen=True)
class ProcessSample:
    pid: int
    rss_kb: int
    pss_kb: int
    hwm_kb: int | None = None  # peak resident size; None when not readable


@dataclass(frozen=True)
class Scope:
    name: str
    processes: tuple[ProcessSample, ...] = field(default_factory=tuple)
    unreadable: tuple[int, ...] = field(default_factory=tuple)  # visible pids whose memory could not be read

    @property
    def rss_kb(self) -> int:
        return sum(p.rss_kb for p in self.processes)

    @property
    def pss_kb(self) -> int:
        return sum(p.pss_kb for p in self.processes)


def _parent_map(proc_root: str) -> dict[int, int]:
    parents: dict[int, int] = {}
    for entry in os.listdir(proc_root):
        if not entry.isdigit():
            continue
        try:
            with open(f"{proc_root}/{entry}/stat") as handle:
                stat = handle.read()
        except OSError:
            continue  # the process exited while scanning
        # the command name may contain spaces and parentheses: split after the last ')'
        fields = stat[stat.rindex(")") + 2 :].split()
        parents[int(entry)] = int(fields[1])
    return parents


def tree_pids(root_pid: int, proc_root: str = "/proc") -> list[int]:
    """The root and all its descendants, each pid once, root first."""
    parents = _parent_map(proc_root)
    if root_pid not in parents:
        raise ScopeError(f"process {root_pid} is not running")
    children: dict[int, list[int]] = {}
    for pid, parent in parents.items():
        children.setdefault(parent, []).append(pid)
    ordered, queue = [], [root_pid]
    while queue:
        pid = queue.pop(0)
        ordered.append(pid)
        queue.extend(sorted(children.get(pid, [])))
    return ordered


def _read_rollup(pid: int, proc_root: str) -> ProcessSample | None:
    values: dict[str, int] = {}
    try:
        with open(f"{proc_root}/{pid}/smaps_rollup") as handle:
            for line in handle:
                key, _, rest = line.partition(":")
                if key in ("Rss", "Pss"):
                    values[key] = int(rest.split()[0])
    except OSError:
        return None  # exited, or not readable: never recorded as zero
    if len(values) != 2:
        return None
    return ProcessSample(pid, values["Rss"], values["Pss"], _read_hwm(pid, proc_root))


def _read_hwm(pid: int, proc_root: str) -> int | None:
    try:
        with open(f"{proc_root}/{pid}/status") as handle:
            for line in handle:
                if line.startswith("VmHWM:"):
                    return int(line.split()[1])
    except OSError:
        pass
    return None


def cpu_seconds(pids: list[int], proc_root: str = "/proc") -> float:
    """User plus system CPU seconds consumed so far by the given processes."""
    ticks = 0
    for pid in pids:
        try:
            with open(f"{proc_root}/{pid}/stat") as handle:
                stat = handle.read()
        except OSError:
            continue
        fields = stat[stat.rindex(")") + 2 :].split()
        ticks += int(fields[11]) + int(fields[12])   # utime, stime
    return ticks / os.sysconf("SC_CLK_TCK")


def collect_scopes(roots: dict[str, int], proc_root: str = "/proc") -> dict[str, Scope]:
    """Sample every named process tree; categories must not share a process."""
    owner: dict[int, str] = {}
    scopes: dict[str, Scope] = {}
    for name, root_pid in roots.items():
        samples, unreadable = [], []
        for pid in tree_pids(root_pid, proc_root):
            if pid in owner:
                raise ScopeError(f"process {pid} is in both {owner[pid]!r} and {name!r}")
            owner[pid] = name
            sample = _read_rollup(pid, proc_root)
            if sample is not None:
                samples.append(sample)
            elif os.path.exists(f"{proc_root}/{pid}"):
                unreadable.append(pid)
        scopes[name] = Scope(name, tuple(samples), tuple(unreadable))
    return scopes
