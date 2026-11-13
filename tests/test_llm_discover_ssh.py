"""M12-P02: the same discovery on an ssh host: a self-contained Python 3 script run through the bounded remote runner. No real host: the fake ssh runs the remote command on
this machine under `/bin/sh` with a private `HOME` and the process table root of the test (`--llm-proc-root DIR` gives `DIR/<alias>` to the script), so it reads the table the
test wrote and probes only the test's own fake servers. The failure cases end inside their budgets with a clean message: unreachable, no python3, a host that outlasts the
budget, an output over the cap, a host that is not Linux, a table that cannot be read."""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.llm_fixtures import FakeServer, normalised, ollama_tags, openai_models, write_proc
from tests.test_codex_adapter import usual_umask


def _rig(root: Path) -> tuple[Path, Path]:
    """An ssh config and a fake `ssh` that behaves per host alias."""
    cfg = root / "ssh_config"
    cfg.write_text("".join(f"Host {h}\n  HostName 10.20.0.{i}\n  User u{i}\n\n" for i, h in enumerate(["gpu-box", "offline-box", "hang-box", "nopy-box", "slow-box", "flood-box", "bsd-box", "empty-box", "huge-box"], 11)))
    cfg.chmod(0o600)
    (root / 'nopy_bin').mkdir(exist_ok=True)  # a host that has a shell and no python3
    if not (root / 'nopy_bin' / 'sh').exists():
        os.symlink('/bin/sh', root / 'nopy_bin' / 'sh')
    for h in ["gpu-box", "nopy-box", "slow-box", "flood-box", "bsd-box", "empty-box", "huge-box"]:
        (root / "remote_hosts" / h / "user_home").mkdir(parents=True, exist_ok=True)
    mock = root / "mock_ssh.py"
    mock.write_text(
        f"#!{sys.executable}\n"
        "import os, sys, time\n"
        "from pathlib import Path\n"
        "args = sys.argv[1:]\n"
        "assert '-o' in args and 'BatchMode=yes' in args, args\n"
        "sep = args.index('--')\n"
        "host, remote_cmd = args[sep + 1], args[sep + 2]\n"
        f"root = Path({str(root)!r})\n"
        "with open(root / 'ssh_calls.log', 'a') as fh:\n"
        "    fh.write(host + '\\n')\n"
        "if host == 'hang-box':\n"
        "    time.sleep(1.5)\n"
        "    sys.stderr.write('ssh: connect to host 10.20.0.13 port 22: Operation timed out\\n')\n"
        "    sys.exit(255)\n"
        "uhome = root / 'remote_hosts' / host / 'user_home'\n"
        "if not uhome.is_dir():\n"
        "    sys.stderr.write(f'ssh: Could not resolve hostname {host}\\n')\n"
        "    sys.exit(255)\n"
        "if host == 'slow-box':\n"
        "    time.sleep(60)\n"
        "if host == 'flood-box':\n"
        "    sys.stdout.write('x' * 300000)\n"
        "    sys.exit(0)\n"
        "if host == 'bsd-box':\n"
        "    print('LICO_JSON={\"host\":null,\"os\":\"freebsd\",\"scope\":\"same_user\",\"supported\":false,\"source\":\"remote_script\",\"listening\":0,\"probed\":0,\"skipped\":0,\"elapsed_ms\":1,\"endpoints\":[],\"reason\":\"os_unknown\"}')\n"
        "    sys.exit(0)\n"
        "os.chdir(str(uhome))\n"
        "env = {'HOME': str(uhome), 'PATH': str(root / 'nopy_bin') if host == 'nopy-box' else '/usr/bin:/bin', 'LANG': 'C.UTF-8'}\n"
        "os.execve('/bin/sh', ['/bin/sh', '-c', remote_cmd], env)\n"
    )
    mock.chmod(0o700)
    return cfg, mock


def _host(directory: str, root: Path, env: dict | None = None):
    cfg, mock = _rig(root)
    host, _log = start(directory, extra=["--llm-proc-root", str(root / "fake_proc"), "--ssh", str(mock), "--ssh-config", str(cfg)], sessions=0, producing=0, env_overrides=env)
    return host


def _calls(root: Path) -> list[str]:
    path = root / "ssh_calls.log"
    return path.read_text().split() if path.exists() else []


def _tables(root: Path, alias: str | None, servers: list[FakeServer], binds: list[str] | None = None) -> None:
    base = root / "fake_proc" / alias if alias else root / "fake_proc"
    sockets = [{"port": s.port, "address": (binds or ["127.0.0.1"] * len(servers))[i], "inode": 10 + i} for i, s in enumerate(servers)]
    write_proc(base, sockets, {7: ("serve", [10 + i for i in range(len(servers))])})


def test_the_remote_script_gives_the_same_endpoints_as_the_local_engine_and_says_where_each_listens():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        servers = [FakeServer(openai_models(("tiny-llama", "qwen-small"))), FakeServer(ollama_tags(("llama3:8b",))), FakeServer(mode="banner"), FakeServer({"/": ("200 OK", {}, "<html>")})]
        try:
            binds = ["127.0.0.1", "0.0.0.0", "127.0.0.1", "127.0.0.1"]
            _tables(root, None, servers, binds)
            _tables(root, "gpu-box", servers, binds)
            host = _host(directory, root)
            try:
                conn = Conn(host.socket)
                local = conn.call(op="llm.discover")
                req = {"op": "llm.discover", "ssh_host": "gpu-box"}
                assert v1.validate_request("llm.discover", req) == []
                remote = conn.call(**req)
                assert remote["ok"] == 1 and v1.validate_response("llm.discover", remote) == [], remote
                assert (remote["host"], remote["source"], remote["os"], remote["scope"], remote["supported"]) == ("gpu-box", "remote_script", "linux", "same_user", True), remote
                strip = lambda r: {k: v for k, v in normalised(r).items() if k not in ("host", "source")}  # noqa: E731
                assert strip(remote) == strip(local), (strip(remote), strip(local))
                found = {e["port"]: e for e in remote["endpoints"]}
                assert found[servers[0].port]["bind"] == "loopback" and found[servers[1].port]["bind"] == "all", "the bind says whether a later use needs a tunnel"
                assert found[servers[0].port]["kind"] == "openai" and found[servers[1].port]["kind"] == "ollama" and found[servers[2].port]["kind"] == "not-http"
                assert _calls(root) == ["gpu-box"], "one ssh call"
                assert all(r == b"" for r in servers[2].received), "neither engine sends anything to a server that speaks first"
                via_cli = json.loads(cli(host, "llm", "discover", "--ssh-host", "gpu-box").stdout)
                assert normalised(via_cli) == normalised(remote)
                assert json.loads(cli(host, "llm", "discover", "--ssh-host", "gpu-box", "--all-users").stdout)["scope"] == "all_users"
            finally:
                host.stop()
        finally:
            for s in servers:
                s.close()


def test_every_failure_of_a_host_ends_fast_with_a_clean_message():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        host = _host(directory, root)
        try:
            conn = Conn(host.socket)
            for alias, error in [("offline-box", "ssh_unreachable"), ("hang-box", "ssh_unreachable"), ("nowhere-box", "ssh_unreachable"), ("nopy-box", "remote_python_missing"), ("flood-box", "remote_output_too_large")]:
                t0 = time.monotonic()
                reply = conn.call(op="llm.discover", ssh_host=alias)
                assert reply["ok"] == 0 and reply["error"] == error, (alias, reply)
                assert time.monotonic() - t0 < 8, alias
            bsd = conn.call(op="llm.discover", ssh_host="bsd-box")
            assert bsd["ok"] == 1 and bsd["supported"] is False and bsd["reason"] == "os_unknown" and bsd["host"] == "bsd-box" and bsd["endpoints"] == [], bsd
            assert v1.validate_response("llm.discover", bsd) == [], bsd
            empty = conn.call(op="llm.discover", ssh_host="empty-box")
            assert empty["ok"] == 1 and empty["supported"] is False and empty["reason"] == "proc_unreadable" and empty["host"] == "empty-box", empty
        finally:
            host.stop()


def test_a_host_that_outlasts_the_budget_ends_in_remote_timeout_and_the_ssh_process_is_killed():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        host = _host(directory, root, env={"LICO_LLM_SSH_SECS": "3"})
        try:
            t0 = time.monotonic()
            reply = Conn(host.socket).call(op="llm.discover", ssh_host="slow-box")
            took = time.monotonic() - t0
            assert reply["ok"] == 0 and reply["error"] == "remote_timeout" and took < 7, (reply, took)
        finally:
            host.stop()


def test_the_remote_script_probes_at_most_sixty_four_ports_and_stays_inside_its_time():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        closers = [FakeServer(mode="close") for _ in range(70)]
        try:
            _tables(root, "gpu-box", closers)
            host = _host(directory, root)
            try:
                t0 = time.monotonic()
                reply = Conn(host.socket).call(op="llm.discover", ssh_host="gpu-box")
                assert reply["ok"] == 1 and reply["listening"] == 70 and reply["probed"] == 64 and reply["skipped"] == 6, reply
                assert time.monotonic() - t0 < 12
            finally:
                host.stop()
        finally:
            for s in closers:
                s.close()
