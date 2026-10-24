import json
from pathlib import Path

from tests.test_credential_store import cred


def account(conn, label, provider="codex"):
    made = conn.call(op="account.create", provider=provider, label=label)
    assert made["ok"] == 1, made
    return made["account"]


def add_credential(conn, keyring, directory, account_id, secret):
    """Store a canary key for the account in the throwaway keyring and attach it; returns its reference."""
    done = cred(keyring.env, Path(directory) / "credentials", "add", "--provider", "claude", "--account", account_id, "--kind", "api_key",
                "--label", "key ending " + secret[-4:], stdin=secret + "\n")
    assert done.returncode == 0, done.stderr
    reference = json.loads(done.stdout)["ref"]
    assert conn.call(op="account.credential", account=account_id, credential_ref=reference)["ok"] == 1
    return reference
