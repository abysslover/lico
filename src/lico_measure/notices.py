"""Third-party notices of the single executable, assembled from the crates it is built from.

Every package of `Cargo.lock` (except the program itself) gets its name, version, license expression and
the license and notice files its source ships; identical texts are written once and name the crates that
carry them. The result is committed as `src/host/hostd/NOTICES.txt` and compiled into the program.
"""

from __future__ import annotations

import os
import re
import tomllib
from pathlib import Path

from .rig import REPO

HOSTD = REPO / "src" / "host" / "hostd"
ALLOWED = {"MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "Unicode-DFS-2016", "0BSD", "CC0-1.0",
           "BSL-1.0", "Apache-2.0 WITH LLVM-exception", "Unlicense", "LGPL-2.1-or-later"}
FILE_PATTERN = re.compile(r"^(licen[sc]e|copying|notice|unlicense)([-_.].*)?$", re.IGNORECASE)


def registry_src() -> Path:
    home = Path(os.environ.get("CARGO_HOME", REPO / "private" / "cargo-home"))
    sources = sorted((home / "registry" / "src").glob("*"))
    if not sources:
        raise RuntimeError("the crate sources are not in the local registry (build once first)")
    return sources[0]


def locked_packages() -> list[tuple[str, str]]:
    """The crates of the Linux build: those of the lock file whose source cargo fetched. The lock file also names crates of
    other platforms (Android, Windows, macOS), which are neither downloaded nor compiled into this program."""
    lock = tomllib.loads((HOSTD / "Cargo.lock").read_text())
    source = registry_src()
    return sorted((p["name"], p["version"]) for p in lock["package"]
                  if p.get("source", "").startswith("registry+") and (source / f"{p['name']}-{p['version']}").is_dir())


def acceptable(expression: str) -> bool:
    """Whether the project can use the crate: every `AND` part needs an accepted license among its `OR` choices."""
    parts = re.split(r"\s+AND\s+", expression.replace("(", " ").replace(")", " "))
    return all(any(choice.strip() in ALLOWED for choice in re.split(r"\s+OR\s+|/", part)) for part in parts)


def package_info(source: Path, name: str, version: str) -> dict:
    directory = source / f"{name}-{version}"
    manifest = tomllib.loads((directory / "Cargo.toml").read_text())["package"]
    texts = []
    for path in sorted(directory.iterdir()):
        if path.is_file() and FILE_PATTERN.match(path.name):
            texts.append((path.name, path.read_text(errors="replace").strip()))
    return {"name": name, "version": version, "license": manifest.get("license") or manifest.get("license-file") or "",
            "authors": manifest.get("authors", []), "repository": manifest.get("repository", ""), "texts": texts}


def build() -> str:
    source = registry_src()
    infos = [package_info(source, n, v) for n, v in locked_packages()]
    by_text: dict[str, list[str]] = {}
    for info in infos:
        for _, text in info["texts"]:
            by_text.setdefault(text, []).append(f"{info['name']} {info['version']}")
    out = ["THIRD-PARTY NOTICES",
           "",
           "The Lico executable is built from its own source, which is under the MIT License (see `lico licenses`, first part), and from",
           "the open-source crates listed here. Each crate is listed with its license expression; where the expression offers a choice",
           "the program may be used under any one of the licenses named. The license and notice texts the crates ship follow, each once.",
           "",
           "== CRATES =="]
    for info in infos:
        authors = "; ".join(info["authors"]) or "(authors not stated)"
        out.append(f"{info['name']} {info['version']} | {info['license'] or 'NO LICENSE FIELD'} | {authors}")
    out += ["", "== CRATES THAT SHIP NO LICENSE TEXT =="]
    for info in infos:
        if not info["texts"]:
            out.append(f"{info['name']} {info['version']}: license field {info['license']!r}; the crate ships no license file and states no copyright holder here. "
                       f"Its repository, {info['repository'] or 'not stated'}, is where its license and copyright are stated; the license text it is offered under follows the crate list (the MIT License text below).")
    out += ["", "== LICENSE AND NOTICE TEXTS =="]
    for text, crates in sorted(by_text.items(), key=lambda kv: kv[1][0]):
        out += ["", "-" * 78, "Carried by: " + ", ".join(sorted(set(crates))), "-" * 78, text]
    return "\n".join(out) + "\n"


def problems() -> list[str]:
    """What is wrong with the committed notices, or with the licenses the program is built from."""
    found = []
    source = registry_src()
    notices = (HOSTD / "NOTICES.txt").read_text()
    for name, version in locked_packages():
        info = package_info(source, name, version)
        if f"{name} {version} | " not in notices:
            found.append(f"{name} {version}: not listed in the notices")
        if not info["license"]:
            found.append(f"{name} {version}: no license field")
        elif not acceptable(info["license"]):
            found.append(f"{name} {version}: license {info['license']!r} is not one the project accepts")
        if not info["texts"] and not info["authors"] and not info["repository"]:
            found.append(f"{name} {version}: ships no license text and names neither an author nor a repository")
    return found
