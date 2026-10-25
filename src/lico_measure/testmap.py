"""Which tests a change needs: the checked-in map (tests/testmap.toml) and the reading of `git diff`.

    python -m lico_measure.suite select --changed [BASE]

A changed file selects tests by what it is:
  a test file          itself, and every test file that imports it
  a Python source      the test files that import it, directly or through other modules
  a Rust source        `cargo test --bin lico <module>::` for the module and for every module that uses it, plus the test
                       files that start a process running that code (the "areas" of the map)
  a file the map calls inert (a document)   nothing
  any change but an inert one      also the tests the map lists as `always` (guards that read the whole tree, cheap)
  anything else        EVERYTHING (default-deny): a file nobody declared is a file nobody can say is safe to skip
Imports are read from the source, not listed by hand; what the map lists is what a test starts (areas) and which modules run in
which process. Nothing is guessed from a file's name."""
import ast
import fnmatch
import re
import subprocess
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
HOSTD = "src/host/hostd/src/"
IMPORT_ROOTS = ("src/", "")         # PYTHONPATH=src:. - `lico_measure.x` lives under src/, `tests.x` under the root


@dataclass
class Selection:
    everything: bool = False
    files: set = field(default_factory=set)         # test files, repo-relative
    cargo: set = field(default_factory=set)         # cargo test filters; "" means the whole binary's tests
    acceptance: list = field(default_factory=list)  # the changed files that make the acceptance run due
    reasons: list = field(default_factory=list)     # (changed file, why)
    warnings: list = field(default_factory=list)    # what the scan could not read and therefore assumed the worst of

    def tests(self, every_file):
        return sorted(every_file) if self.everything else sorted(self.files & set(every_file))


def glob_match(path, pattern):
    """fnmatch with `**` meaning any number of directories (`*` stops at a slash)."""
    out, i = "", 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out, i = out + "(?:.*/)?", i + 3
        elif pattern.startswith("**", i):
            out, i = out + ".*", i + 2
        elif pattern[i] == "*":
            out, i = out + "[^/]*", i + 1
        elif pattern[i] == "?":
            out, i = out + "[^/]", i + 1
        else:
            out, i = out + re.escape(pattern[i]), i + 1
    return re.fullmatch(out, path) is not None


def load_map(root=REPO):
    return tomllib.loads((Path(root) / "tests" / "testmap.toml").read_text())


def git(root, *args):
    done = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True)
    if done.returncode:
        raise SystemExit(f"git {' '.join(args)}: {done.stderr.strip()}")
    return done.stdout


def tracked(root=REPO):
    """Every file git knows plus the new ones it would add (not the ignored ones)."""
    return sorted(set(git(root, "ls-files", "-co", "--exclude-standard").splitlines()))


def changed(base=None, root=REPO):
    """The files changed in the working tree: staged and unstaged edits, new files, and what was committed since BASE.
    Both sides of a rename are listed, so a moved module also names the place it left."""
    names = set(git(root, "diff", "--no-renames", "--name-only", base or "HEAD").splitlines())
    names |= set(git(root, "ls-files", "--others", "--exclude-standard").splitlines())
    return sorted(names)


# --- Python imports -------------------------------------------------------------------------------------------

def _candidates(parts):
    for prefix in IMPORT_ROOTS:
        base = prefix + "/".join(parts)
        yield base + ".py"
        yield base + "/__init__.py"


def python_graph(files, root=REPO):
    """file -> the tracked Python files it imports (a module, or a name taken from a package)."""
    known = set(files)
    graph = {}
    for path in files:
        if not path.endswith(".py") or not (path.startswith("src/") or path.startswith("tests/")):
            continue
        try:
            tree = ast.parse((Path(root) / path).read_text(), filename=path)
        except (OSError, SyntaxError, UnicodeDecodeError):
            graph[path] = set()
            continue
        package = path.rsplit("/", 1)[0].removeprefix("src/").split("/") if "/" in path else []
        wanted = []
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                wanted += [alias.name.split(".") for alias in node.names]
            elif isinstance(node, ast.ImportFrom):
                module = node.module.split(".") if node.module else []
                if node.level:
                    module = package[:len(package) - node.level + 1] + module
                wanted.append(module)
                wanted += [module + [alias.name] for alias in node.names]
        edges = set()
        for parts in wanted:
            for n in range(1, len(parts) + 1):               # `import a.b.c` runs a, a.b and a.b.c
                edges |= {c for c in _candidates(parts[:n]) if c in known}
        edges.discard(path)
        graph[path] = edges
    return graph


def importers(graph, start):
    """Every file that imports one of `start`, however indirectly (start included)."""
    reverse = {}
    for path, edges in graph.items():
        for edge in edges:
            reverse.setdefault(edge, set()).add(path)
    found, todo = set(start), list(start)
    while todo:
        for path in reverse.get(todo.pop(), ()):
            if path not in found:
                found.add(path)
                todo.append(path)
    return found


# --- Rust modules ---------------------------------------------------------------------------------------------

def rust_module(path):
    """src/host/hostd/src/gui/ui.rs -> "gui" (the top-level module a file belongs to); None if it is not one of the crate's files."""
    if not path.startswith(HOSTD) or not path.endswith(".rs"):
        return None
    return path.removeprefix(HOSTD).split("/")[0].removesuffix(".rs")


def _group_heads(rest):
    """`rest` starts at the `{` of a `use` group: the first path segment of each item at the group's own level (what is nested deeper
    belongs to that item), or None where the group or an item is not a plain path (a glob, `self`, unbalanced braces)."""
    depth, current, items = 0, [], []
    for ch in rest:
        if ch == "{":
            depth += 1
            if depth == 1:
                continue
        elif ch == "}":
            depth -= 1
            if depth == 0:
                break
        elif ch == "," and depth == 1:
            items.append("".join(current))
            current = []
            continue
        current.append(ch)
    else:
        return None
    items.append("".join(current))
    heads = []
    for item in (i.strip() for i in items):
        if not item:                                                  # a trailing comma
            continue
        head = re.match(r"(\w+)(?:\s*$|\s*::|\s+as\b)", item)
        if head is None or head.group(1) in ("self", "super", "crate"):
            return None
        heads.append(head.group(1))
    return heads


def crate_names(text):
    """(the top-level modules a Rust source names through `crate::`, what it names in a shape this scan cannot read).
    Read: `crate::name::...` and a group `crate::{a, b::{c, d}, e as f}` in any layout. Comments are not code. Anything else after
    `crate::` (a glob, a path that continues on no name) is returned as unreadable, never skipped."""
    text = re.sub(r"//[^\n]*|/\*.*?\*/", " ", text, flags=re.S)
    names, unreadable = set(), []
    for match in re.finditer(r"\bcrate::", text):
        rest = text[match.end():]
        name = re.match(r"\w+", rest)
        if name:
            names.add(name.group(0))
        elif rest.startswith("{") and (heads := _group_heads(rest)) is not None:
            names |= set(heads)
        else:
            unreadable.append(text[match.start():match.end() + 24].split("\n")[0])
    return names, unreadable


def rust_uses(files, root=REPO, unreadable=None):
    """top-level module -> the top-level modules whose code it names with `crate::`. A module with a `crate::` path the scan cannot
    read is assumed to use every other module (default-deny: more is selected, never less) and is appended to `unreadable`."""
    texts = {}
    for path in files:
        module = rust_module(path)
        if module and not path.startswith(HOSTD + "bin/"):
            texts.setdefault(module, []).append((Path(root) / path).read_text(errors="replace"))
    uses = {}
    for module, parts in texts.items():
        used, bad = set(), []
        for text in parts:
            names, unread = crate_names(text)
            used |= names
            bad += unread
        uses[module] = (used & set(texts)) - {module}
        if bad:
            uses[module] = set(texts) - {module}
            if unreadable is not None:
                unreadable.append((module, bad[0]))
    return uses


def rust_dependents(uses, module):
    """The module and every module that, directly or not, uses it."""
    found, todo = {module}, [module]
    while todo:
        target = todo.pop()
        for name, used in uses.items():
            if target in used and name not in found:
                found.add(name)
                todo.append(name)
    return found


# --- the selection --------------------------------------------------------------------------------------------

def select(paths, tmap=None, files=None, root=REPO):
    tmap = tmap or load_map(root)
    files = files if files is not None else tracked(root)
    test_files = [f for f in files if re.fullmatch(r"tests/test_[^/]+\.py", f)]
    graph = python_graph(files, root)
    unreadable = []
    uses = rust_uses(files, root, unreadable)
    areas_of_file = tmap.get("areas", {})
    module_areas = tmap.get("rust", {}).get("modules", {})
    result = Selection()
    result.warnings = [f"rust module {m} names `{snippet}` in a shape the scan cannot read: assumed to use every module" for m, snippet in unreadable]

    def runs(test_file, areas):
        """Does this test file start a process that runs code of these areas? A file the map does not list is assumed to."""
        mine = areas_of_file.get(test_file)
        if mine is None:
            return True
        return bool(mine) if "*" in areas else bool(set(mine) & areas)

    def everything(path, why):
        result.everything = True
        result.cargo.add("")
        result.reasons.append((path, why))

    for path in paths:
        if any(glob_match(path, pattern) for pattern in tmap.get("acceptance", {}).get("triggers", [])):
            result.acceptance.append(path)
        inert = next((entry for entry in tmap.get("inert", []) if glob_match(path, entry["glob"])), None)
        module = rust_module(path)
        if inert:
            result.reasons.append((path, f"inert: {inert['reason']}"))
        elif any(glob_match(path, pattern) for pattern in tmap.get("rust", {}).get("everything", [])):
            result.cargo.add("")
            result.files |= {f for f in test_files if runs(f, {"*"})}                   # a build input: every test that runs the binary
            result.reasons.append((path, "build input of the binary: the whole binary's tests"))
        elif (source := next((entry for entry in tmap.get("sources", []) if glob_match(path, entry["glob"])), None)):
            hit = importers(graph, set(source["tests"])) & set(test_files)
            result.files |= hit
            result.reasons.append((path, f"read by path: {source['reason']}"))
        elif module is not None and module in module_areas:
            dependents = rust_dependents(uses, module)
            areas = set().union(*(set(module_areas.get(m, ["*"])) for m in dependents))
            result.cargo |= {f"{m}::" for m in sorted(dependents)}
            result.files |= {f for f in test_files if runs(f, areas)}
            result.reasons.append((path, f"rust module {module}; used by {', '.join(sorted(dependents - {module})) or 'nothing'}; areas {', '.join(sorted(areas))}"))
        elif module is not None:
            everything(path, f"rust module {module} is not in the map")
        elif path.startswith("tests/fixtures/"):
            # a fixture is named by its file name in the test that reads it; the tests that import that test come with it
            hit = importers(graph, {f for f in test_files if Path(path).name in (Path(root) / f).read_text(errors="replace")}) & set(test_files)
            if hit:
                result.files |= hit
                result.reasons.append((path, f"fixture named by {len(hit)} test file(s)"))
            else:
                everything(path, "a fixture no test file names")
        elif path in graph:
            hit = {f for f in importers(graph, {path}) if f in test_files}
            if hit:
                result.files |= hit
                result.reasons.append((path, f"imported (directly or not) by {len(hit)} test file(s)"))
            else:
                everything(path, "a Python file no test imports and the map does not call inert")
        else:
            everything(path, "not in the map (default-deny)")
    if any(not r[1].startswith("inert") for r in result.reasons):
        result.files |= set(tmap.get("always", {}).get("tests", [])) & set(test_files)       # the cheap guards that read the whole tree
    if result.everything:
        result.files |= set(test_files)
    return result


def render(result, every_file):
    lines = [f"# warning: {w}" for w in result.warnings]
    lines += [f"# {reason[1]}  <- {reason[0]}" for reason in result.reasons]
    if result.cargo:
        lines.append("cargo " + ("(all unit tests)" if "" in result.cargo else " ".join(sorted(result.cargo))))
    lines += result.tests(every_file)
    if result.acceptance:
        lines.append("# acceptance run due (python -m lico_measure.suite acceptance): " + ", ".join(result.acceptance))
    return "\n".join(lines)
