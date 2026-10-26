"""Opt-in check against the REAL `claude` and `codex` programs installed on this machine:
a folder chosen for a Lico session must not make the program load or run that folder's own configuration.

The folder holds canaries (settings hook, status line, MCP server, notify hook, skill, agent, command). Each canary appends one line to
a file when it runs, or a skill, agent or command is named in the program's own output. A row passes when what is seen is what the row
expects. Every start uses an EMPTY throwaway home and a dead proxy (or a dead model port), so no model call leaves the machine and no
sign-in is read.

What the positive controls show can be seen: for Claude a settings hook, an MCP server, and the names of the folder's skill, agent and
command. The status line and a skill's own hook never fired even with the project settings on (print mode), so a quiet row says nothing
about them. NOT checked here: whether the program reads the folder's CLAUDE.md or AGENTS.md as text for the model (that needs a model call).

The result is only as good as the versions printed on the first lines: the programs change their loading rules between releases.
Re-run this script whenever either program updates.

Usage: python3 -I real_program_canary.py [claude|codex|all] [--lico PATH_TO_LICO_BINARY]
Exit: 0 every expectation held, 1 one did not, 2 a program (or the given lico binary) is missing. Takes about 5 minutes (295 s measured, load average about 4).
Rows: claude (a) real flags `--setting-sources user`, auto and default permission mode, canary must not run; (b) positive control
`user,project,local`, canary MUST run (otherwise the method is blind); codex per folder kind (plain, repo): `old-auto` positive control, `new-auto` `new-default`
`resume-auto` `resume-default` must not load; codex through the Lico host (needs --lico): no trust entry, canary 0.
"""
import json
import os
import select
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

TEMPS: list[str] = []
CLAUDE_CTL = ('{"type":"control_request","request_id":"r1","request":{"subtype":"initialize"}}\n'
              '{"type":"control_request","request_id":"r2","request":{"subtype":"mcp_status"}}\n')
CLAUDE_CANARY = '#!/bin/sh\necho "$1 $(date +%s) cwd=$(pwd)" >> "{ran}"\nexit 0\n'
CODEX_CANARY = '#!/bin/sh\necho "$1 $(date +%s) cwd=$(pwd)" >> "$(dirname "$(pwd)")/ran.txt"\nexit 0\n'
CLAUDE_NAMES = ("canarysk", "canarycmd", "canaryagent")
RESULTS: list[bool] = []


def temp(prefix: str) -> Path:
    path = Path(tempfile.mkdtemp(prefix=prefix))     # mode 0700; short name, a unix socket path must stay short
    TEMPS.append(str(path))
    return path


def drop(path: Path) -> None:
    if path.is_symlink():
        path.unlink()
    elif path.exists():
        shutil.rmtree(path)
    assert not path.exists() and not path.is_symlink(), f"temp dir not removed: {path}"
    TEMPS.remove(str(path))


def report(ok: bool, name: str, seen: str) -> None:
    RESULTS.append(ok)
    print(f"{'PASS' if ok else 'FAIL'} {name}: {seen}", flush=True)


def write_exec(path: Path, text: str) -> None:
    path.write_text(text)
    path.chmod(0o700)


def lines_of(path: Path) -> list[str]:
    try:
        return [l for l in path.read_text().splitlines() if l.strip()]
    except OSError:
        return []


def kinds(lines: list[str]) -> list[str]:
    return sorted(l.split(" ", 1)[0] for l in lines)


# ------------------------------------------------------------------ claude

def claude_folder(root: Path, ran: Path) -> Path:
    scripts, proj = root / "bin", root / "proj"
    scripts.mkdir()
    touch = scripts / "touch.sh"
    write_exec(touch, CLAUDE_CANARY.format(ran=ran))
    (proj / ".claude" / "commands").mkdir(parents=True)
    (proj / ".claude" / "agents").mkdir()
    (proj / ".claude" / "skills" / "canarysk").mkdir(parents=True)
    t = str(touch)
    (proj / ".claude" / "settings.json").write_text(json.dumps({
        "enableAllProjectMcpServers": True,
        "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": f"{t} settings_hook"}]}]},
        "statusLine": {"type": "command", "command": f"{t} statusline"}}))
    (proj / ".mcp.json").write_text(json.dumps({"mcpServers": {"canary": {"command": t, "args": ["mcp_json"]}}}))
    (proj / ".claude" / "commands" / "canarycmd.md").write_text("canary command\n")
    (proj / ".claude" / "agents" / "canaryagent.md").write_text("---\nname: canaryagent\ndescription: canary agent\n---\nprompt\n")
    (proj / ".claude" / "skills" / "canarysk" / "SKILL.md").write_text(
        f"---\nname: canarysk\ndescription: canary skill\nhooks:\n  PreToolUse:\n    - hooks:\n        - type: command\n          command: {t} skill_hook\n---\nbody\n")
    if shutil.which("git"):
        subprocess.run(["git", "init", "-q"], cwd=proj, capture_output=True)
    return proj


def claude_start(sources: str, mode: str = "auto") -> tuple[list[str], str]:
    """One start the way hostd starts it: no user message, control requests only. Returns the canary lines and the raw stdout."""
    root = temp("lc-c")
    try:
        ran = root / "ran.txt"
        ran.write_text("")
        (root / "home").mkdir()
        proj = claude_folder(root, ran)
        env = {"PATH": os.environ["PATH"], "LANG": os.environ.get("LANG", "C.UTF-8"), "HOME": str(root / "home"),
               "HTTPS_PROXY": "http://127.0.0.1:9", "HTTP_PROXY": "http://127.0.0.1:9", "NO_PROXY": "",
               "DISABLE_AUTOUPDATER": "1", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "DISABLE_TELEMETRY": "1"}
        argv = ["claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--replay-user-messages",
                "--setting-sources", sources, "--permission-mode", "default" if mode == "default" else "bypassPermissions", "--permission-prompt-tool", "stdio"]
        out = open(root / "stdout", "wb")
        p = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=out, stderr=subprocess.DEVNULL, env=env, cwd=proj, start_new_session=True)

        def feed():
            try:
                time.sleep(2)
                p.stdin.write(CLAUDE_CTL.encode())
                p.stdin.flush()
                time.sleep(10)
                p.stdin.close()
            except OSError:
                pass
        threading.Thread(target=feed, daemon=True).start()
        try:
            p.wait(timeout=30)
        except subprocess.TimeoutExpired:
            pass
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except OSError:
            pass
        p.wait()
        out.close()
        time.sleep(0.5)
        return lines_of(ran), (root / "stdout").read_text(errors="replace")
    finally:
        drop(root)


def run_claude() -> None:
    for mode in ("auto", "default"):
        lines, text = claude_start("user", mode)
        names = [n for n in CLAUDE_NAMES if n in text]
        report(not lines and not names, f"claude {mode} setting-sources=user (how Lico starts it)",
               f"canary ran {len(lines)} times {kinds(lines)}; folder items named in its output: {names or 'none'}")
    lines, text = claude_start("user,project,local")
    k = kinds(lines)
    names = [n for n in CLAUDE_NAMES if n in text]
    missing = [n for n in ("settings_hook", "mcp_json") if n not in k] + [n for n in CLAUDE_NAMES if n not in names]
    report(not missing, "claude positive control setting-sources=user,project,local",
           f"canary ran {len(lines)} times {sorted(set(k))}; folder items named in its output: {names or 'none'}"
           + (f" (not seen: {missing}; the method is blind)" if missing else ""))


# ------------------------------------------------------------------ codex

SCENARIOS = {  # name: (thread approvalPolicy, thread sandbox, turn approvalPolicy, turn sandboxPolicy, resume)
    "old-auto": ("never", "danger-full-access", None, None, False),
    "new-auto": ("never", "read-only", "never", {"type": "dangerFullAccess"}, False),
    "new-default": ("untrusted", "read-only", "untrusted", {"type": "readOnly"}, False),
    "resume-auto": ("never", "read-only", "never", {"type": "dangerFullAccess"}, True),
    "resume-default": ("untrusted", "read-only", "untrusted", {"type": "readOnly"}, True),
}


def codex_folder(work: Path, kind: str) -> Path:
    canary = work / "canary.sh"
    write_exec(canary, CODEX_CANARY)
    folder = work / "proj"
    (folder / ".codex").mkdir(parents=True)
    if kind == "repo":
        (folder / ".git").mkdir()
    (folder / ".codex" / "config.toml").write_text(
        f'model = "canary-model"\nnotify = ["{canary}", "codex_notify"]\n\n[mcp_servers.canary]\ncommand = "{canary}"\nargs = ["codex_mcp"]\n\n'
        f'[model_providers.canary]\nname = "canary"\nbase_url = "http://127.0.0.1:9/v1"\n')
    (folder / "AGENTS.md").write_text("The code word of this folder is pelican.\n")
    return folder


def trust_lines(home: Path) -> list[str]:
    try:
        return [l.strip() for l in (home / ".codex" / "config.toml").read_text().splitlines() if "trust_level" in l or l.startswith("[projects")]
    except OSError:
        return []


class Server:
    def __init__(self, home: Path, folder: Path):
        env = {"PATH": os.environ["PATH"], "HOME": str(home), "LANG": "C.UTF-8", "HTTPS_PROXY": "http://127.0.0.1:9", "HTTP_PROXY": "http://127.0.0.1:9"}
        self.p = subprocess.Popen(["codex", "app-server"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env, cwd=folder, text=True)
        self.n = 0
        self.call("initialize", {"clientInfo": {"name": "canary", "title": "canary", "version": "0"}, "capabilities": {}}, wait=1.5)
        self.p.stdin.write(json.dumps({"method": "initialized"}) + "\n")
        self.p.stdin.flush()

    def drain(self, seconds: float) -> list[dict]:
        out, end = [], time.time() + seconds
        while time.time() < end:
            if select.select([self.p.stdout], [], [], 0.3)[0]:
                line = self.p.stdout.readline()
                if not line:
                    break
                try:
                    out.append(json.loads(line))
                except ValueError:
                    pass
        return out

    def call(self, method: str, params: dict, wait: float = 4) -> dict:
        self.n += 1
        self.p.stdin.write(json.dumps({"id": self.n, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        for m in self.drain(wait):
            if m.get("id") == self.n:
                return m
        return {"error": "no answer"}

    def stop(self) -> None:
        self.p.kill()
        self.p.wait()


def codex_scenario(scenario: str, kind: str) -> dict:
    pol, sbx, tpol, tsbx, resume = SCENARIOS[scenario]
    work = temp("lc-x")
    try:
        home = work / "home"
        home.mkdir()
        (work / "ran.txt").write_text("")
        folder = codex_folder(work, kind)
        seen = {"errors": []}
        srv = Server(home, folder)
        start = srv.call("thread/start", {"cwd": str(folder), "approvalPolicy": pol, "sandbox": sbx, "approvalsReviewer": "user"})
        thread = ((start.get("result") or {}).get("thread") or {}).get("id")
        if not thread:
            seen["errors"].append("thread/start: " + json.dumps(start)[:120])
        elif tpol:
            turn = srv.call("turn/start", {"threadId": thread, "input": [{"type": "text", "text": "hi", "text_elements": []}],
                                           "approvalPolicy": tpol, "sandboxPolicy": tsbx}, wait=6)
            if "result" not in turn:
                seen["errors"].append("turn/start: " + json.dumps(turn)[:120])
        if resume and thread:
            srv.stop()
            srv = Server(home, folder)
            back = srv.call("thread/resume", {"threadId": thread, "excludeTurns": True, "cwd": str(folder), "approvalPolicy": pol,
                                              "sandbox": sbx, "approvalsReviewer": "user"})
            if "result" not in back:
                seen["errors"].append("thread/resume: " + json.dumps(back)[:120])
            else:
                turn = srv.call("turn/start", {"threadId": thread, "input": [{"type": "text", "text": "hi again", "text_elements": []}],
                                               "approvalPolicy": tpol, "sandboxPolicy": tsbx}, wait=6)
                if "result" not in turn:
                    seen["errors"].append("turn/start after resume: " + json.dumps(turn)[:120])
        body = srv.call("config/read", {"includeLayers": True, "cwd": str(folder)}, wait=3).get("result") or {}
        seen["mcp_servers"] = sorted(((body.get("config") or {}).get("mcp_servers") or {}).keys())
        seen["project_layer"] = "absent"
        for layer in body.get("layers") or []:
            if (layer.get("name") or {}).get("type") == "project":
                seen["project_layer"] = "disabled" if layer.get("disabledReason") else "loaded"
        srv.stop()
        seen["canary"] = kinds(lines_of(work / "ran.txt"))
        seen["trust"] = trust_lines(home)
        return seen
    finally:
        drop(work)


def describe(s: dict) -> str:
    text = f"project layer {s['project_layer']}, mcp servers {s['mcp_servers'] or 'none'}, canary ran {len(s['canary'])} times {s['canary']}, trust entries {len(s['trust'])}"
    return text + (f", errors {s['errors']}" if s["errors"] else "")


def run_codex() -> None:
    for kind in ("plain", "repo"):
        for scenario in SCENARIOS:
            s = codex_scenario(scenario, kind)
            name = f"codex {kind} {scenario}"
            if scenario == "old-auto":
                ok = bool(s["canary"]) and bool(s["trust"]) and s["project_layer"] == "loaded" and not s["errors"]
                report(ok, name + " (positive control, must load)", describe(s) + ("" if ok else " (the method is blind)"))
            else:
                ok = s["project_layer"] == "disabled" and "canary" not in s["mcp_servers"] and not s["canary"] and not s["trust"] and not s["errors"]
                report(ok, name + " (must not load)", describe(s))


# ------------------------------------------------------------------ codex through the Lico host

class Conn:
    def __init__(self, path: str, timeout: float = 30.0):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.connect(path)
        self.sock.settimeout(timeout)
        self.buf = b""

    def call(self, **request) -> dict:
        self.sock.sendall((json.dumps(request) + "\n").encode())
        while True:
            while b"\n" not in self.buf:
                chunk = self.sock.recv(1 << 20)
                if not chunk:
                    raise EOFError
                self.buf += chunk
            line, self.buf = self.buf.split(b"\n", 1)
            message = json.loads(line)
            if "ok" in message:
                return message


def run_lico(lico: str) -> None:
    for kind in ("plain", "repo"):
        work, sock_dir = temp("lc-v"), temp("lc-s")
        host = None
        problems, trust_seen = [], []
        try:
            (work / "home").mkdir()
            (work / "codexhome").mkdir()
            (work / "codexhome" / "config.toml").write_text('model = "dead-model"\nmodel_provider = "dead"\n\n[model_providers.dead]\nname = "dead"\nbase_url = "http://127.0.0.1:9/v1"\n')
            (work / "ran.txt").write_text("")
            folder = codex_folder(work, kind)
            env = {"PATH": os.environ["PATH"], "LANG": os.environ.get("LANG", "C.UTF-8"), "HOME": str(work / "home"), "CODEX_HOME": str(work / "codexhome")}
            sock = str(sock_dir / "s")
            host = subprocess.Popen([lico, "host", "--socket", sock, "--state-dir", str(work / "state"), "--sessions", "0", "--producing", "0"],
                                    env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=work)
            for _ in range(100):
                if os.path.exists(sock):
                    break
                time.sleep(0.1)
            try:
                conn = Conn(sock)
                for mode in ("auto", "default"):
                    made = conn.call(op="session.create", kind="codex", mode=mode, cwd=str(folder), name=f"canary-{mode}")
                    if made.get("ok") != 1:
                        problems.append(f"{mode}: create failed {json.dumps(made)[:100]}")
                        continue
                    sid, gen = made["session"], made["generation"]
                    conn.call(op="input", session=sid, generation=gen, request_id=f"{mode}-1", text="hi")
                    time.sleep(8)                      # the turn fails against the dead port; give it time to try
                    trust_seen += trust_lines_codexhome(work)
                    conn.call(op="session.close", session=sid, generation=gen)
                    time.sleep(1)
                    back = conn.call(op="session.resume", session=sid, generation=gen)
                    if back.get("ok") != 1:
                        problems.append(f"{mode}: resume failed {json.dumps(back)[:100]}")
                        continue
                    conn.call(op="input", session=sid, generation=back["generation"], request_id=f"{mode}-2", text="hi again")
                    time.sleep(8)
                    trust_seen += trust_lines_codexhome(work)
            except (OSError, EOFError, ValueError) as e:
                problems.append(f"connection: {type(e).__name__}")
            finally:
                subprocess.run([lico, "--socket", sock, "host", "stop", "--force"], env=env, capture_output=True, timeout=30)
                try:
                    host.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    host.kill()
                    host.wait()
            ran = kinds(lines_of(work / "ran.txt"))
            trust_seen += trust_lines_codexhome(work)
            ok = not ran and not trust_seen and not problems
            report(ok, f"lico path codex {kind} (auto and default, create/input/close/resume/input)",
                   f"canary ran {len(ran)} times {ran}, trust entries {len(set(trust_seen))}" + (f", problems {problems}" if problems else ""))
        finally:
            if host is not None and host.poll() is None:
                host.kill()
                host.wait()
            drop(work)
            drop(sock_dir)


def trust_lines_codexhome(work: Path) -> list[str]:
    try:
        return [l.strip() for l in (work / "codexhome" / "config.toml").read_text().splitlines() if "trust_level" in l or l.startswith("[projects")]
    except OSError:
        return []


# ------------------------------------------------------------------ main

def version(program: str) -> str | None:
    if not shutil.which(program):
        return None
    r = subprocess.run([program, "--version"], capture_output=True, text=True, timeout=30)
    return (r.stdout or r.stderr).strip().splitlines()[0] if (r.stdout or r.stderr).strip() else "unknown"


def main(argv: list[str]) -> int:
    args, lico = list(argv), None
    if "--lico" in args:
        i = args.index("--lico")
        if i + 1 >= len(args):
            print("usage: real_program_canary.py [claude|codex|all] [--lico PATH]")
            return 2
        lico = args[i + 1]
        del args[i:i + 2]
    target = args[0] if args else "all"
    if target not in ("claude", "codex", "all") or len(args) > 1:
        print("usage: real_program_canary.py [claude|codex|all] [--lico PATH]")
        return 2
    cv, xv = version("claude"), version("codex")
    print(f"claude --version: {cv or 'MISSING'}")
    print(f"codex --version: {xv or 'MISSING'}", flush=True)
    need = {"claude": [("claude", cv)], "codex": [("codex", xv)], "all": [("claude", cv), ("codex", xv)]}[target]
    missing = [n for n, v in need if v is None]
    if missing:
        print(f"missing program: {', '.join(missing)}")
        return 2
    if lico is not None and not os.access(lico, os.X_OK):
        print(f"missing lico binary: {lico}")
        return 2
    try:
        if target in ("claude", "all"):
            run_claude()
        if target in ("codex", "all"):
            run_codex()
            if lico is None:
                print("SKIP lico path: no --lico", flush=True)
            else:
                run_lico(os.path.abspath(lico))
    finally:
        for t in list(TEMPS):
            p = Path(t)
            if p.is_symlink():
                p.unlink()
            else:
                shutil.rmtree(p, ignore_errors=True)
            assert not p.exists(), f"temp dir not removed: {t}"
    print(f"{sum(RESULTS)} of {len(RESULTS)} rows passed", flush=True)
    return 0 if all(RESULTS) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
