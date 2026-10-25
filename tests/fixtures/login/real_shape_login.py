"""A test double with the byte structure of the real `claude auth login` (measured on 2026-10-25): an OSC 8 link whose visible text repeats the address,
and a prompt WITHOUT a newline. The address and every value in it are made up."""
import json
import os
import sys

URL = "https://claude.example.invalid/oauth/authorize?code=true&client_id=FAKECLIENT&state=FAKESTATE1&code_challenge=FAKECHALLENGE2"
home = os.environ["HOME"]
with open(os.path.join(home, "seen.json"), "w") as out:
    json.dump({"env": sorted(os.environ), "browser": os.environ.get("BROWSER")}, out)
sys.stdout.write("Opening browser to sign in…\n")
sys.stdout.write("If the browser didn't open, visit: \x1b]8;;" + URL + "\x07" + URL + "\x1b]8;;\x07\n")
sys.stdout.write("Paste code here if prompted > ")
sys.stdout.flush()
raw = sys.stdin.buffer.readline()
with open(os.path.join(home, "received_raw"), "wb") as out:
    out.write(raw)
config = os.environ.get("CLAUDE_CONFIG_DIR") or os.environ.get("CODEX_HOME")
with open(os.path.join(config, "auth.json"), "w") as out:
    out.write("{}")
sys.exit(0 if raw == b"CANARYCODE9\n" else 3)
