import json
import os
import subprocess
from pathlib import Path

from lico_measure.keyring_rig import PrivateKeyring
from lico_measure.p02 import HOSTD_DIR
from lico_measure.rig import private_dir

CRED = HOSTD_DIR / "lico-cred"
CANARY = "canary-key-7f3e9a41c0d25b86"


def cred(env, data_dir, *args, stdin=None):
    return subprocess.run([str(CRED), "--data-dir", str(data_dir), *args], env=env, input=stdin, capture_output=True, text=True, timeout=30)


def tree_files(*roots):
    return [p for root in roots for p in Path(root).rglob("*") if p.is_file()]


def test_secret_values_stay_in_the_keyring_and_every_other_sink_holds_none():
    with PrivateKeyring() as keyring, private_dir() as data:
        added = cred(keyring.env, data, "add", "--provider", "claude", "--account", "a-7c1e", "--kind", "api_key", "--label", "key ending c0d2", stdin=CANARY + "\n")
        assert added.returncode == 0, added.stderr
        reference = json.loads(added.stdout)["ref"]
        # the secret is never an argument: a command line with the secret in it is refused before anything happens
        refused = cred(keyring.env, data, "add", "--provider", "claude", "--account", "a-1", "--kind", "api_key", "--label", "x", "--secret", CANARY)
        assert refused.returncode == 2 and CANARY not in refused.stdout + refused.stderr
        # positive control: the helper mode returns it from the keyring to a caller that asked by reference
        got = cred(keyring.env, data, "helper", "--ref", reference)
        assert got.returncode == 0 and got.stdout.strip() == CANARY
        # nothing else holds it: outputs of every call, the metadata, every file of the run, every process of the keyring
        for out in (added, refused):
            assert CANARY not in out.stdout + out.stderr
        assert CANARY not in " ".join(added.args)
        assert CANARY not in cred(keyring.env, data, "list").stdout
        meta = json.loads((Path(data) / "credentials.json").read_text())
        assert len(meta) == 1 and set(meta[0]) == {"ref", "provider", "account", "kind", "masked_label", "created"}
        assert meta[0]["ref"] == reference and meta[0]["masked_label"] == "key ending c0d2" and meta[0]["ref"] != meta[0]["account"]
        assert oct((Path(data) / "credentials.json").stat().st_mode & 0o777) == "0o600"
        for path in tree_files(data, keyring.root):
            assert CANARY.encode() not in path.read_bytes(), path
        for pid in keyring.pids():
            assert CANARY.encode() not in Path(f"/proc/{pid}/cmdline").read_bytes()
            assert CANARY.encode() not in Path(f"/proc/{pid}/environ").read_bytes()
        # removing it removes the keyring entry and the metadata, and the helper then finds nothing
        assert cred(keyring.env, data, "remove", "--ref", reference).returncode == 0
        assert cred(keyring.env, data, "helper", "--ref", reference).returncode == 4
        assert json.loads((Path(data) / "credentials.json").read_text()) == []


def test_locked_or_missing_keyring_fails_closed_with_an_action():
    def assert_closed(env, data, expect_word):
        status = cred(env, data, "status")
        assert status.returncode == 3 and json.loads(status.stdout)["keyring"] != "ready", status
        assert expect_word in json.loads(status.stdout)["action"]
        added = cred(env, data, "add", "--provider", "claude", "--account", "a-1", "--kind", "api_key", "--label", "key ending", stdin=CANARY + "\n")
        assert added.returncode == 3 and added.stdout == "" and expect_word in added.stderr, added
        # no plaintext fallback: no metadata, no file anywhere holds the secret
        assert not (Path(data) / "credentials.json").exists()
        for path in tree_files(data, env["XDG_RUNTIME_DIR"].rsplit("/run", 1)[0]):
            assert CANARY.encode() not in path.read_bytes(), path

    with private_dir() as data:
        # no session bus at all (the standard fallback path is pointed at an empty directory)
        with PrivateKeyring(daemon=False) as keyring:
            env = {k: v for k, v in keyring.env.items() if k != "DBUS_SESSION_BUS_ADDRESS"}
            assert_closed(env, data, "session bus")
            # a bus with no Secret Service on it
            env = keyring.env
            assert_closed(env, data, "Secret Service")
        # a Secret Service with no default keyring (never created or unlocked)
        with PrivateKeyring(unlock=False) as keyring:
            env = keyring.env
            assert_closed(env, data, "keyring")
        # a keyring that was unlocked and then locked
        with PrivateKeyring() as keyring:
            env = keyring.env
            keyring.lock()
            assert_closed(env, data, "unlock")
