"""A stand-in for a host that this source cannot start: one that predates `host.stop`, or one that answers `bad_request` to a request it does not know.

Usage: fake_host.py --socket PATH [...]; the environment variable FAKE_HOST_BUILD is the build id its status reports ("none": the status has no build
id at all, like the hosts before build ids). Every operation except status and account.list is answered with the error in FAKE_HOST_ERROR
(default unknown_op). When FAKE_HOST_CHILD names a file, the host starts a long sleep of its own (a provider that outlives its host) and writes the pid
there. It prints one line with its pid when it listens, like the real host."""

import json
import os
import socket
import subprocess
import sys
import threading

path = sys.argv[sys.argv.index("--socket") + 1]
build = os.environ.get("FAKE_HOST_BUILD", "none")
error = os.environ.get("FAKE_HOST_ERROR", "unknown_op")

try:
    os.unlink(path)
except OSError:
    pass
server = socket.socket(socket.AF_UNIX)
server.bind(path)
server.listen(8)


def answer(request):
    reply = {"v": 1, "ok": 1}
    if "id" in request:
        reply["id"] = request["id"]
    if request.get("op") == "status":
        reply.update({"pid": os.getpid(), "logins_running": 0, "sessions": []})
        if build != "none":
            reply["build"] = build
    elif request.get("op") == "account.list":
        reply["accounts"] = [{"account": "acc1", "provider": "codex", "label": "Work", "default": True}]
    else:
        reply = {"v": 1, "ok": 0, "error": error}
        if "id" in request:
            reply["id"] = request["id"]
    return reply


def serve(conn):
    with conn, conn.makefile("rw") as stream:
        for line in stream:
            try:
                stream.write(json.dumps(answer(json.loads(line))) + "\n")
                stream.flush()
            except (ValueError, OSError):
                return


if os.environ.get("FAKE_HOST_CHILD"):
    child = subprocess.Popen(["sleep", "300"], start_new_session=True)       # leads its own group, as a provider does
    with open(os.environ["FAKE_HOST_CHILD"], "w") as out:
        out.write(str(child.pid))

print(json.dumps({"pid": os.getpid()}), flush=True)
while True:
    connection, _ = server.accept()
    threading.Thread(target=serve, args=(connection,), daemon=True).start()
