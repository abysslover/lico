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
    anon_kb: int | None = None   # resident anonymous memory (heap, stacks)
    file_kb: int | None = None   # resident file-backed pages (libraries, mapped files)
    shmem_kb: int | None = None  # resident shared memory


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
    status = _read_status(pid, proc_root)
    return ProcessSample(pid, values["Rss"], values["Pss"], status.get("VmHWM"),
                         status.get("RssAnon"), status.get("RssFile"), status.get("RssShmem"))


def _read_status(pid: int, proc_root: str) -> dict[str, int]:
    wanted = ("VmHWM", "RssAnon", "RssFile", "RssShmem")
    found: dict[str, int] = {}
    try:
        with open(f"{proc_root}/{pid}/status") as handle:
            for line in handle:
                key, _, rest = line.partition(":")
                if key in wanted:
                    found[key] = int(rest.split()[0])
    except OSError:
        pass
    return found  # a field that is absent stays absent, never zero


def cpu_seconds(pids: list[int], proc_root: str = "/proc", strict: bool = False) -> float:
    """User plus system CPU seconds consumed so far by the given processes.

    With ``strict`` a process that has exited is an error instead of being skipped,
    so a dead process can never read as an idle one.
    """
    ticks = 0
    for pid in pids:
        try:
            with open(f"{proc_root}/{pid}/stat") as handle:
                stat = handle.read()
        except OSError:
            if strict:
                raise ScopeError(f"process {pid} exited during the measurement")
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


def mapping_top(pid: int, count: int = 12, proc_root: str = "/proc") -> list[dict]:
    """The mappings that hold the most resident memory, grouped by backing file."""
    totals: dict[str, list[int]] = {}
    name = "[anonymous]"
    with open(f"{proc_root}/{pid}/smaps") as handle:
        for line in handle:
            head = line.split(None, 5)
            if len(head) >= 5 and "-" in head[0] and not line.startswith(("Rss", "Pss")):
                name = head[5].strip() if len(head) == 6 else "[anonymous]"
            elif line.startswith("Rss:"):
                totals.setdefault(name, [0, 0])[0] += int(line.split()[1])
            elif line.startswith("Pss:"):
                totals.setdefault(name, [0, 0])[1] += int(line.split()[1])
    ranked = sorted(totals.items(), key=lambda item: -item[1][0])[:count]
    return [{"mapping": n, "rss_kb": v[0], "pss_kb": v[1]} for n, v in ranked]
