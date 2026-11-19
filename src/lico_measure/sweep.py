"""Ends the sessions a test made on its module's host, so that the next test of the file starts from an empty host.

A module's host lives for the whole file, and every session on it is a program that stays until the host stops: the Claude simulator is a Python
interpreter of about 20 MiB. A file of 25 tests that made 52 sessions held 46 of them at once, 1 GiB for two seconds of tests (found in the memory
inspection of M23, 2026-11-19). A test file opts in with an autouse fixture that calls `end_all` after each test."""


def end_all(conn):
    """Purge every session the host has (the program is stopped, its log and its tool connection are gone). A refusal is not a failure of the test."""
    for row in conn.call(op="status")["sessions"]:
        conn.call(op="session.purge", session=row["session"], generation=row["generation"])
