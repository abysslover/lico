"""Opt-in check of the layout of the real program's own conversation record (the file the conversation reader of P03 S05 reads), across two versions of the program.

  run    ONE session on the program given by `--claude` in a private host, TWO short haiku turns: turn 1 asks for one call of the `sessions` tool of Lico (a tool use and a tool
         result in the record), turn 2 for a one-word answer. The record the program wrote is copied to `--out` (a private folder) as `<version>.jsonl`.
  scrub  `--in RECORD --out FIXTURE`: the record with everything that is not layout replaced: ids made up in order of appearance, the folder and branch made up, thinking and
         signatures, and every record that is not a message reduced to its keys; the text of a message and the input and result of a tool are kept when short. Then the
         output is searched for what must not be in a tracked file, and the script stops when it finds any.
  table  `--reference FILE...` and `--candidate FILE...`: what the records hold, as a table: every record type, every content block type, every key, the shapes of `content`, the
         values of `version`, and for each the words same / new / gone of the candidate against the reference; the rows the reader reads are marked.

    PYTHONPATH=src python3 layout_check.py run --lico PATH_TO_LICO --claude PROGRAM --out DIR [--status-files GLOB]
    python3 layout_check.py table --reference A.jsonl B.jsonl ... --candidate C.jsonl [--md]
    python3 layout_check.py scrub --in RECORD.jsonl --out FIXTURE.jsonl

The sign-in is the one of the home the script runs under; it is never read here. Only the layout is printed: keys, types, counts, and the version field, never a word of a conversation."""
import argparse
import getpass
import json
import re
import shutil
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE.parent / "00002_s04_real_program_check")]
import s04_check as base  # noqa: E402  (the private host and its driver)

TURN_1 = "Call the tool mcp__lico__sessions once, then answer with the single word done. Use no other tool."
TURN_2 = "Reply with the single word ok."
STAMP = re.compile(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d")
# what the reader looks at (provider_record/mod.rs): the keys it reads, by group; the groups that are not keys are read whole (the record type, the block types, the shapes of `content`)
READ_KEYS = {"keys of user": {"type", "version", "isSidechain", "isMeta", "isCompactSummary", "timestamp", "message"}, "keys of assistant": {"type", "version", "isSidechain", "isMeta", "timestamp", "message"},
             "keys of system": {"type", "subtype"}, "keys of user.message": {"content"}, "keys of assistant.message": {"content"}, "keys of a text block": {"text"}, "keys of a image block": {"source"},
             "keys of a tool_use block": {"name", "input"}, "keys of a tool_result block": {"is_error", "content"}}
READ_WHOLE = ("record types", "version values", "system subtypes", "timestamp shapes", "flags", "blocks in", "shape of")


def read_by_the_reader(group, item):
    return group.startswith(READ_WHOLE) or item in READ_KEYS.get(group, ())


def layout(paths):
    """The facts of a set of record files; every value is a count, a key or a type name."""
    facts = {"record types": Counter(), "system subtypes": Counter(), "version values": Counter(), "timestamp shapes": Counter(), "flags": Counter()}
    keys, blocks, block_keys, shapes = defaultdict(set), defaultdict(Counter), defaultdict(set), defaultdict(Counter)
    for path in paths:
        for line in Path(path).read_text().splitlines():
            if not line.strip():
                continue
            record = json.loads(line)
            kind = record.get("type", "?")
            facts["record types"][kind] += 1
            keys[kind] |= set(record)
            if kind in ("user", "assistant"):
                facts["version values"][str(record.get("version"))] += 1
            if kind == "system":
                facts["system subtypes"][str(record.get("subtype"))] += 1
            if "timestamp" in record:
                facts["timestamp shapes"]["iso" if isinstance(record["timestamp"], str) and STAMP.match(record["timestamp"]) else "other"] += 1
            for flag in ("isSidechain", "isMeta", "isCompactSummary"):
                if flag in record:
                    facts["flags"][f"{flag}={record[flag]}"] += 1
            message = record.get("message")
            if kind in ("user", "assistant") and isinstance(message, dict):
                keys[f"{kind}.message"] |= set(message)
                content = message.get("content")
                shapes[kind]["str" if isinstance(content, str) else "list" if isinstance(content, list) else "other"] += 1
                for block in content if isinstance(content, list) else []:
                    name = block.get("type", "?") if isinstance(block, dict) else "?"
                    blocks[kind][name] += 1
                    block_keys[name] |= set(block)
                    if name == "tool_result":
                        inner = block.get("content")
                        shapes["tool_result.content"]["str" if isinstance(inner, str) else "list" if isinstance(inner, list) else "other"] += 1
                        for part in inner if isinstance(inner, list) else []:
                            blocks["tool_result.content"][part.get("type", "?") if isinstance(part, dict) else "?"] += 1
    return {"facts": facts, "keys": dict(keys), "blocks": {k: Counter(v) for k, v in blocks.items()}, "block keys": dict(block_keys), "shapes": {k: Counter(v) for k, v in shapes.items()}}


def rows(summary):
    """The layout as a flat set of (group, item) pairs, for the comparison."""
    out = set()
    for group, counter in summary["facts"].items():
        out |= {(group, item) for item in counter}
    for kind, names in summary["keys"].items():
        out |= {(f"keys of {kind}", name) for name in names}
    for kind, counter in summary["blocks"].items():
        out |= {(f"blocks in {kind}", name) for name in counter}
    for kind, names in summary["block keys"].items():
        out |= {(f"keys of a {kind} block", name) for name in names}
    for kind, counter in summary["shapes"].items():
        out |= {(f"shape of {kind}", name) for name in counter}
    return out


def table(reference, candidate, markdown):
    a, b = layout(reference), layout(candidate)
    ra, rb = rows(a), rows(b)
    lines = []
    for group, item in sorted(ra | rb):
        word = "same" if (group, item) in ra and (group, item) in rb else "new" if (group, item) in rb else "gone"
        read = "read" if read_by_the_reader(group, item) else ""
        lines.append((group, item, word, read))
    counts = Counter(w for _, _, w, _ in lines)
    if markdown:
        print("| group | item | candidate against reference | read by the reader |\n|---|---|---|---|")
    for group, item, word, read in lines:
        print(f"| {group} | {item} | {word} | {read} |" if markdown else f"{word:5} {read:5} {group}: {item}")
    print(f"\n{len(reference)} reference file(s), {len(candidate)} candidate file(s); same {counts['same']}, new {counts['new']}, gone {counts['gone']}")
    for name, summary in (("reference", a), ("candidate", b)):
        print(f"{name} version values {dict(summary['facts']['version values'])}; record types {dict(summary['facts']['record types'])}")


ID_KEYS = {"uuid", "parentUuid", "sourceToolAssistantUUID", "sessionId", "promptId", "requestId", "requestRef", "toolUseID", "turnAttributionKey", "leafUuid", "id", "tool_use_id"}
KEPT_WORDS = {"type", "subtype", "role", "tool_name"}      # the words that name a layout: kept when they are plain
KEPT_TEXT = 300      # a text longer than this is not kept
MESSAGE_KEYS = {"role", "content", "model", "type", "stop_reason", "stop_sequence"}
# what must not reach a tracked file: an address, a path of this machine (and the home and the user name it has), a runtime handle, and the shapes of a secret
FORBIDDEN = re.compile("|".join([re.escape(str(Path.home())), re.escape(getpass.getuser()), r"@", r"/(?:home|data|tmp|mnt|Users)/", r"term_", r"Bearer", r"sk-", r"eyJ", r"ghp_", r"(?i:password|passwd|secret|authorization)"]))


class Scrubber:
    def __init__(self):
        self.ids = {}

    def fake(self, value, key):
        if not isinstance(value, str):
            return value
        if value not in self.ids:
            n = len(self.ids) + 1
            self.ids[value] = f"toolu_{n:04d}" if value.startswith("toolu_") else f"msg_{n:04d}" if key == "id" and value.startswith("msg_") else f"req_{n:04d}" if value.startswith("req_") else f"00000000-0000-4000-8000-{n:012d}"
        return self.ids[value]

    def skeleton(self, value, key=""):
        """The keys of a value and the kind of what they hold; ids are made up, numbers and booleans stay."""
        if isinstance(value, dict):
            return {k: self.skeleton(v, k) for k, v in value.items()}
        if isinstance(value, list):
            return [self.skeleton(v, key) for v in value[:2]]
        if isinstance(value, str):
            if key in KEPT_WORDS and re.fullmatch(r"[A-Za-z0-9_-]{1,40}", value):
                return value
            return self.fake(value, key) if key in ID_KEYS else "x"
        return value

    def text(self, value):
        return value if isinstance(value, str) and len(value) <= KEPT_TEXT else "(scrubbed)"

    def block(self, block):
        kind = block.get("type") if isinstance(block, dict) else None
        if kind == "text":
            return {**block, "text": self.text(block.get("text"))}
        if kind == "thinking":
            return {**block, "thinking": "(scrubbed)", "signature": "(scrubbed)"}
        if kind == "tool_use":
            out = {**block, "id": self.fake(block.get("id"), "id"), "input": {k: self.text(v) if isinstance(v, str) else self.skeleton(v, k) for k, v in (block.get("input") or {}).items()}}
            return {**out, **({"caller": self.skeleton(block["caller"], "caller")} if "caller" in block else {})}
        if kind == "tool_result":
            inner = block.get("content")
            inner = self.text(inner) if isinstance(inner, str) else [self.block(b) for b in inner] if isinstance(inner, list) else inner
            return {**block, "tool_use_id": self.fake(block.get("tool_use_id"), "tool_use_id"), "content": inner}
        return self.skeleton(block)

    def message(self, message):
        out = {k: v for k, v in message.items() if k in MESSAGE_KEYS and k != "content"}
        out["id"] = self.fake(message["id"], "id") if "id" in message else None
        if out["id"] is None:
            del out["id"]
        content = message.get("content")
        out["content"] = self.text(content) if isinstance(content, str) else [self.block(b) for b in content] if isinstance(content, list) else content
        out.update({k: self.skeleton(v, k) for k, v in message.items() if k not in MESSAGE_KEYS and k != "id"})
        return out

    def record(self, record):
        kind = record.get("type")
        if kind not in ("user", "assistant", "system"):
            return self.skeleton(record) | {"type": kind} | ({"timestamp": record["timestamp"]} if "timestamp" in record else {})
        out = {}
        for key, value in record.items():
            if key == "message" and isinstance(value, dict):
                out[key] = self.message(value)
            elif key == "cwd":
                out[key] = "/work/proj"
            elif key == "gitBranch":
                out[key] = "main"
            elif key in ID_KEYS:
                out[key] = self.fake(value, key)
            elif key in ("toolUseResult", "hookInfos", "hookErrors", "hookAdditionalContext", "content", "stopReason"):
                out[key] = self.skeleton(value, key)
            else:
                out[key] = value if not isinstance(value, (dict, list)) and (not isinstance(value, str) or len(value) <= 80) else self.skeleton(value, key)
        return out


def scrub(source, target):
    scrubber, lines = Scrubber(), []
    for line in Path(source).read_text().splitlines():
        if line.strip():
            lines.append(json.dumps(scrubber.record(json.loads(line)), ensure_ascii=True, separators=(",", ":")))
    text = "\n".join(lines) + "\n"
    found = sorted(set(FORBIDDEN.findall(text)))
    if found:
        raise SystemExit(f"the scrubbed record still holds {found}; nothing written")
    Path(target).write_text(text)
    print(f"{len(lines)} records, {len(text)} bytes written to {target}")


def run(args):
    base.STATUS_FILES[:] = [args.status_files] if args.status_files else []
    program = ["--claude", args.claude]
    version = subprocess.run([args.claude, "--version"], capture_output=True, text=True, timeout=30).stdout.split()[0]
    before = base.allowance()
    base.say("program", version, "allowance before", before)
    ctx = base.Ctx(str(Path(args.lico).resolve()), program)
    try:
        sid, generation = ctx.create("layout-1", mode="default", model="haiku")
        for n, text in ((1, TURN_1), (2, TURN_2)):
            ctx.say_to(sid, generation, text, f"layout-{n}")
            if not ctx.wait(lambda: base.finished(ctx.records(sid)) >= n, 120):
                raise SystemExit(f"turn {n} did not finish")
        records = ctx.records(sid)
        tools = base.tools_in(records)
        base.say("tools called", tools, "turns", ctx.turns())
        registry = json.loads((ctx.state / "registry" / "sessions.json").read_text())["sessions"]
        provider_id = next(e["provider_session"] for e in registry if e["session"] == sid)
        found = list((Path.home() / ".claude" / "projects").glob(f"*/{provider_id}.jsonl"))
        if len(found) != 1:
            raise SystemExit(f"expected one record file of the program, found {len(found)}")
        out = Path(args.out)
        out.mkdir(parents=True, exist_ok=True)
        target = out / f"{version}.jsonl"
        shutil.copy(found[0], target)
        base.say("record copied to", target, "size", target.stat().st_size)
    finally:
        ctx.stop()
        base.say("allowance after", base.allowance())
        for path in base.TEMPS:
            shutil.rmtree(path, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    r = sub.add_parser("run")
    r.add_argument("--lico", required=True)
    r.add_argument("--claude", required=True, help="the program to run (its --version is the name of the copy)")
    r.add_argument("--out", required=True, help="a private folder for the copy of the record")
    r.add_argument("--status-files", help="a glob of the status-line files for the allowance figures (see 00002)")
    c = sub.add_parser("scrub")
    c.add_argument("--in", dest="source", required=True)
    c.add_argument("--out", required=True)
    t = sub.add_parser("table")
    t.add_argument("--reference", nargs="+", required=True)
    t.add_argument("--candidate", nargs="+", required=True)
    t.add_argument("--md", action="store_true")
    args = parser.parse_args()
    if args.command == "run":
        run(args)
    elif args.command == "scrub":
        scrub(args.source, args.out)
    else:
        table(args.reference, args.candidate, args.md)


if __name__ == "__main__":
    main()
