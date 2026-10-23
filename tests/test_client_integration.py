import os
import subprocess
import time

import pytest

from lico_measure.ime import InputMethod, _focus
from lico_measure.rig import Client, Xvfb, private_dir

from tests.test_host_control import echoes
from tests.test_host_core import Conn, read_log, start

CLIENTS = ["a2", "c2"]


def _keys(display, *keys, delay=60):
    env = dict(os.environ, DISPLAY=display)
    subprocess.run(["xdotool", "key", "--delay", str(delay), *keys], env=env, check=True)


@pytest.mark.parametrize("name", CLIENTS)
def test_gui_kill_and_reattach_keeps_agent_and_sequence(name):
    with private_dir() as directory:
        xvfb = Xvfb()
        host = None
        client = second = None
        try:
            host, log = start(directory, sessions=1, producing=1, rate=100, history=0, retain=100000)
            sid = host.sessions[0]
            conn = Conn(host.socket)
            agents = conn.call(op="resources")["agents"]
            assert len(agents) == 1
            agent_pid = agents[0]["pid"]
            client = Client(name, host.socket, xvfb.display)
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            time.sleep(2.0)
            res = conn.call(op="resources")
            assert res["views"] == 1 and res["subscribers"] == 1       # the view is registered with the host
            before = conn.call(op="status")["sessions"][0]["seq"]
            client.kill()                                               # a crash, not a polite exit
            time.sleep(5.0)
            res = conn.call(op="resources")
            assert res["views"] == 0 and res["subscribers"] == 0        # the dead view is gone
            assert [a["pid"] for a in res["agents"]] == [agent_pid]     # the same agent process
            os.kill(agent_pid, 0)                                       # and it is still alive
            after = conn.call(op="status")["sessions"][0]["seq"]
            assert after >= before + 400, (before, after)               # output went on without any GUI
            seqs = [s for s, _ in read_log(log, sid)]
            assert seqs == list(range(len(seqs)))                       # durable, contiguous
            second = Client(name, host.socket, xvfb.display)
            second.send(f"show {sid}")
            second.expect("state", "subscribed")
            time.sleep(1.5)
            state = second.state()
            current = conn.call(op="status")["sessions"][0]["seq"]
            assert state["view_resident"] and state["gaps"] == 0 and state["dups"] == 0
            assert state["cursor"] >= current - 5 and state["cursor"] > after   # shows current content
            assert conn.call(op="resources")["views"] == 1
        finally:
            for c in (client, second):
                if c and c.proc.poll() is None:
                    c.kill()
            if host:
                host.stop()
            xvfb.stop()


@pytest.mark.parametrize("name", CLIENTS)
def test_view_switch_never_sends_composition_to_another_session(name):
    with private_dir() as directory:
        xvfb = Xvfb()
        host = method = client = None
        try:
            host, log = start(directory, sessions=2, producing=0, history=3)
            s1, s2 = host.sessions
            method = InputMethod(xvfb.display)
            client = Client(name, host.socket, xvfb.display, extra_env=method.client_env)
            client.send(f"show {s1}")
            client.expect("state", "subscribed")
            time.sleep(1.0)
            _focus(xvfb.display)
            time.sleep(0.5)
            _keys(xvfb.display, "shift+space")                          # Latin to Hangul
            time.sleep(0.5)
            _keys(xvfb.display, "d", "k", "s", "s", "u", "d")           # 안 is committed, 녕 is still composing
            time.sleep(0.5)
            client.send(f"show {s2}")                                   # switch in the middle of a composition
            client.expect("state", "subscribed")
            time.sleep(0.8)
            _focus(xvfb.display)
            time.sleep(0.8)
            _keys(xvfb.display, "g", "k", "Return")                     # 하, committed by Return, then sent
            time.sleep(1.0)
            client.send(f"show {s1}")                                   # back: the committed draft returns
            client.expect("state", "subscribed")
            time.sleep(0.8)
            _focus(xvfb.display)
            time.sleep(0.8)
            _keys(xvfb.display, "Return")                               # sends the restored draft
            time.sleep(1.5)
            first, second = echoes(log, s1), echoes(log, s2)
            # the composition in progress when the view was switched is committed to the session
            # it was typed for (녕 joins 안); it never reaches the other session
            assert first == ["안녕"], first
            assert second == ["하"], second                              # nothing from the other session's composition
            assert all("녕" not in text for text in second)
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if method:
                method.stop()
            if host:
                host.stop()
            xvfb.stop()
