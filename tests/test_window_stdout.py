"""M02-P02-F2: what a person types, composes or sends is not copied to the window's standard output, which a launcher may keep in a log.
The `ime` events are for the measurement rig, which asks for them (`rig.Client` sets LICO_GUI_EVENTS=1, so every test that reads them still does)."""
import json
import queue

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import new_claude, sent_users
from tests.test_codex_adapter import wait_for
from tests.test_new_session_screen import claude_args

CANARY = "CANARY-typed-7f3a"


class Recording:
    """The window's standard-output events, kept as they are read (`ProductWindow.ui` drops what is not its answer)."""

    def __init__(self, events):
        self.events, self.seen = events, []

    def get(self, *args, **kwargs):
        event = self.events.get(*args, **kwargs)
        self.seen.append(event)
        return event

    def __getattr__(self, name):
        return getattr(self.events, name)


def what_the_window_says(env_extra=None):
    """Type the canary into a session's input line, send it, and return the window's standard-output events other than the answers to our own `ui` commands."""
    with private_dir() as directory:
        args, received = claude_args(directory)
        made = {}

        def session(w):
            made["id"], _ = new_claude(Conn(w.socket), name="canary")

        with ProductWindow(host_binary=LICO, host_args=args, before_window=session, env_extra=env_extra) as w:
            recording = w.events = Recording(w.events)
            wait_for(lambda: w.ui()["rows"])
            top, bottom, _ = next(r for r in w.ui()["layout"]["rows"] if r[2] == made["id"])
            w.click(100, (top + bottom) / 2)
            w.type_text(CANARY)
            assert CANARY in w.ui()["input"], "the text reached the window: the checks below are not vacuous"
            w.key("Return")                                                          # the draft is sent to the session
            wait_for(lambda: any(CANARY in json.dumps(m) for m in sent_users(received)))
            try:
                while True:
                    recording.get(timeout=0.5)
            except queue.Empty:
                pass
            return [e for e in recording.seen if e.get("ev") != "ui"]                # the answers to our own `ui` commands do hold the draft, by design


def test_the_product_window_prints_nothing_of_what_is_typed_or_sent():
    said = what_the_window_says()
    assert {e.get("ev") for e in said} <= {"ready", "state"}, [e for e in said if e.get("ev") not in ("ready", "state")][:5]
    assert not [e for e in said if CANARY in json.dumps(e, ensure_ascii=False)]


def test_with_the_rigs_switch_the_window_prints_the_input_events_as_before():
    said = what_the_window_says({"LICO_GUI_EVENTS": "1"})
    assert [e for e in said if e.get("ev") == "ime" and e.get("kind") == "sent" and e.get("text") == CANARY]
    assert [e for e in said if e.get("ev") == "ime" and e.get("kind") == "text" and e.get("text") == CANARY]
