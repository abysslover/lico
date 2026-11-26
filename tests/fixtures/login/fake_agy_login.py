"""A test double for the sign-in of `agy` (`agy acp --login`, never the real program): MODE is `code` or `hang`. It says what the program says (a line of words, the address, then the
prompt of a code with no newline after it), keeps its state under `$HOME/.gemini/antigravity-cli/` as the program does, and notes what it was started with (the NAMES of its environment)."""
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
print("Open this link in a browser to log in with Google:", flush=True)
print("https://accounts.example.invalid/o/oauth2/auth?state=CANARYURLSTATE7", flush=True)
if mode == "hang":
    time.sleep(120)
    sys.exit(1)
sys.stdout.write("Then paste the authorization code, or the full URL of the page you land on, and press Enter: ")
sys.stdout.flush()
code = sys.stdin.readline().strip()
with open(os.path.join(home, "received_code"), "w") as out:
    out.write(code)
place = os.path.join(home, ".gemini", "antigravity-cli")
os.makedirs(place, exist_ok=True)
with open(os.path.join(place, "signed_in"), "w") as out:
    out.write("{}")
sys.exit(0 if code == "CANARYCODE9" else 3)
