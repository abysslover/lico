"""Does the real `claude` program read its --mcp-config file again after the first successful `initialize`?

No model call and no sign-in: an empty throwaway home, a dead proxy with the loopback exempt (as the host starts it), control requests only. The tool
server is this script's own minimal JSON-RPC-over-HTTP server on a loopback port; it records every request with its Authorization header. Each scenario
starts the program with the host's flags, waits until the program reports the server `connected`, changes the file, and asks the program to connect
again (`mcp_reconnect`, or `mcp_toggle` off and on). The server then says which token the new `initialize` carried: the old one (the program kept what
it read) or the new one (it read the file again), or none (it could not connect).

Run: python3 -I config_file_probe.py          (needs `claude` on PATH, Python 3.11 or newer)
"""
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

OLD, NEW = "tok-old-1111", "tok-new-2222"


class Tool(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), Handler)
        self.log = []                                   # (method, bearer)
        self.remove_at_first_initialize = None          # a file this server removes the moment it sees the first `initialize` (what the host would do)
        self.lock = threading.Lock()

    def seen(self, method):
        with self.lock:
            return [bearer for m, bearer in self.log if m == method]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))) or b"{}")
        method, bearer = body.get("method", ""), (self.headers.get("Authorization") or "").removeprefix("Bearer ")
        with self.server.lock:
            self.server.log.append((method, bearer))
            if method == "initialize" and self.server.remove_at_first_initialize is not None:
                self.server.remove_at_first_initialize.unlink()
                self.server.remove_at_first_initialize = None
        if "id" not in body:
            self.send_response(202)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        result = {}
        if method == "initialize":
            result = {"protocolVersion": body.get("params", {}).get("protocolVersion", "2025-06-18"), "capabilities": {"tools": {}}, "serverInfo": {"name": "probe", "version": "0"}}
        elif method == "tools/list":
            result = {"tools": [{"name": "sessions", "description": "x", "inputSchema": {"type": "object", "properties": {}}}]}
        answer = json.dumps({"jsonrpc": "2.0", "id": body["id"], "result": result}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(answer)))
        self.end_headers()
        self.wfile.write(answer)


def config(path: Path, port: int, token: str) -> None:
    path.write_text(json.dumps({"mcpServers": {"lico": {"type": "http", "url": f"http://127.0.0.1:{port}/mcp", "headers": {"Authorization": f"Bearer {token}"}}}}))


class Program:
    """The real program, started the way the host starts a Claude session; control requests in, stream-json out."""

    def __init__(self, root: Path, cfg: Path):
        (root / "home").mkdir()
        (root / "work").mkdir()
        env = {"PATH": os.environ["PATH"], "LANG": "C.UTF-8", "HOME": str(root / "home"), "HTTPS_PROXY": "http://127.0.0.1:9", "HTTP_PROXY": "http://127.0.0.1:9",
               "NO_PROXY": "127.0.0.1", "no_proxy": "127.0.0.1", "DISABLE_AUTOUPDATER": "1", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "DISABLE_TELEMETRY": "1"}
        argv = ["claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--replay-user-messages", "--setting-sources", "user",
                "--permission-mode", "bypassPermissions", "--permission-prompt-tool", "stdio", "--mcp-config", str(cfg), "--allowedTools", "mcp__lico__*"]
        self.out = root / "stdout"
        self.proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=open(self.out, "wb"), stderr=subprocess.DEVNULL, env=env, cwd=root / "work", start_new_session=True)
        self.n = 0

    def ask(self, **request) -> dict:
        self.n += 1
        rid = f"p{self.n}"
        self.proc.stdin.write((json.dumps({"type": "control_request", "request_id": rid, "request": request}) + "\n").encode())
        self.proc.stdin.flush()
        until = time.monotonic() + 15
        while time.monotonic() < until:
            for line in self.out.read_text(errors="replace").splitlines():
                try:
                    message = json.loads(line)
                except ValueError:
                    continue
                if message.get("type") == "control_response" and message.get("response", {}).get("request_id") == rid:
                    return message["response"]
            time.sleep(0.2)
        return {"subtype": "no answer in 15 s"}

    def status(self) -> str:
        answer = self.ask(subtype="mcp_status")
        servers = (answer.get("response") or {}).get("mcpServers") or []
        return ",".join(f"{s.get('name')}={s.get('status')}" for s in servers) or json.dumps(answer)[:120]

    def stop(self) -> None:
        try:
            os.killpg(self.proc.pid, signal.SIGKILL)
        except OSError:
            pass
        self.proc.wait()


def wait_until(check, seconds: float):
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        got = check()
        if got:
            return got
        time.sleep(0.2)
    return None


states = {"nothing": "unchanged", "delete": "deleted", "rewrite": "rewritten", "server": "removed by the server at the first initialize"}


def scenario(name: str, change: str, trigger: str) -> bool:
    """change: nothing | delete | rewrite (a different token) | server (removed by the server at the first initialize, before the program lists tools); trigger: reconnect | toggle | initialize (the control request again) | reload (reload_plugins)."""
    root = Path(tempfile.mkdtemp(prefix="lc-cfg-"))
    server = Tool()
    threading.Thread(target=server.serve_forever, daemon=True).start()
    cfg = root / "lico.mcp.json"
    config(cfg, server.server_address[1], OLD)
    if change == "server":
        server.remove_at_first_initialize = cfg
    program = Program(root, cfg)
    try:
        program.ask(subtype="initialize")
        connected = wait_until(lambda: "lico=connected" in program.status(), 40)
        before = len(server.seen("initialize"))
        if not connected or not before:
            print(f"FAIL {name}: the program never reported the server connected (initializes seen {before})")
            return False
        if change == "server" and cfg.exists():
            print(f"FAIL {name}: the server did not remove the file")
            return False
        if change == "delete":
            cfg.unlink()
        elif change == "rewrite":
            config(cfg, server.server_address[1], NEW)
        if trigger == "reconnect":
            answer = program.ask(subtype="mcp_reconnect", serverName="lico")
        elif trigger == "initialize":
            answer = program.ask(subtype="initialize")
        elif trigger == "reload":
            answer = program.ask(subtype="reload_plugins")
        else:
            program.ask(subtype="mcp_toggle", serverName="lico", enabled=False)
            answer = program.ask(subtype="mcp_toggle", serverName="lico", enabled=True)
        time.sleep(3)
        after = server.seen("initialize")[before:]
        verdict = "none" if not after else ("old token" if set(after) == {OLD} else "new token" if set(after) == {NEW} else f"mixed {sorted(set(after))}")
        print(f"ROW {name}: file {states[change]}, trigger {trigger}: tools/list requests {len(server.seen('tools/list'))}, new initializes {len(after)}, token carried: {verdict}; control answer {answer.get('subtype')} {json.dumps(answer.get('error', ''))[:80]}; status after: {program.status()}")
        return True
    finally:
        program.stop()
        server.shutdown()
        shutil.rmtree(root, ignore_errors=True)


def main() -> int:
    if not shutil.which("claude"):
        print("claude is not on PATH")
        return 2
    print("claude", subprocess.run(["claude", "--version"], capture_output=True, text=True).stdout.strip())
    ok = True
    for name, change, trigger in (("control", "nothing", "reconnect"), ("deleted", "delete", "reconnect"), ("rewritten", "rewrite", "reconnect"),
                                  ("deleted, toggled", "delete", "toggle"), ("rewritten, toggled", "rewrite", "toggle"),
                                  ("removed at the first initialize, reconnect", "server", "reconnect"), ("removed at the first initialize, toggled", "server", "toggle"),
                                  ("deleted, initialize again", "delete", "initialize"), ("deleted, reload plugins", "delete", "reload")):
        ok = scenario(name, change, trigger) and ok
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
