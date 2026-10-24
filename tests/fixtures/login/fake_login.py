"""A test double for a provider's login command (never a real provider): MODE is `code` or `hang`."""
import json
import os
import sys
import time

mode = sys.argv[1]
home = os.environ["HOME"]
with open(os.path.join(home, "seen.json"), "w") as out:
    json.dump({"env": sorted(os.environ), "cwd": os.getcwd(), "words": sys.argv[2:], "pid": os.getpid()}, out)
with open(os.path.join(home, "pid"), "w") as out:
    out.write(str(os.getpid()))
print("\x1b[1mOpen https://login.example.invalid/authorize?state=CANARYURLSTATE7 to sign in.\x1b[0m", flush=True)
if mode == "hang":
    time.sleep(120)
    sys.exit(1)
print("Paste code here if prompted >", flush=True)
code = sys.stdin.readline().strip()
with open(os.path.join(home, "received_code"), "w") as out:
    out.write(code)
config = os.environ.get("CODEX_HOME") or os.environ.get("CLAUDE_CONFIG_DIR")
with open(os.path.join(config, "auth.json"), "w") as out:
    out.write("{}")
sys.exit(0 if code == "CANARYCODE9" else 3)
