"""M02-P02-S04 in the product window: the new-session screen starts a claude session in a folder with a name, a model and an effort, remembers them,
offers the folder again, and says why a folder that is not there is refused."""
import json
import sys
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import HOSTD_DIR, LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.account_helpers import account
from tests.test_claude_adapter import FIXTURE, SIM, argv_of, lines_of
from tests.test_codex_adapter import wait_for


def claude_args(directory):
    received = Path(directory) / "claude_received.txt"
    args = ["--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(SIM), "--claude-arg", "--received", "--claude-arg", str(received),
            "--claude-arg", "--fixture", "--claude-arg", str(FIXTURE), "--cred-bin", str(HOSTD_DIR / "lico-cred"), "--cred-dir", str(Path(directory) / "credentials")]
    return args, received


def screen(w):
    return w.ui()["panel"]


def test_the_new_session_screen_starts_a_session_in_the_folder_with_its_name_model_and_effort_then_offers_them_again():
    with private_dir() as directory:
        project = Path(directory).resolve() / "project"
        project.mkdir()
        args, received = claude_args(directory)
        made = {}

        def work_account(w):
            made["account"] = account(Conn(w.socket), "Work", "claude")

        with ProductWindow(host_binary=LICO, host_args=args, before_window=work_account) as w:
            conn = Conn(w.socket)
            w.key("ctrl+n")
            first = screen(w)
            form = first["form"]
            assert first["title"] == "New session" and first["text_entry"] and form["focus"] == "Folder"
            assert (form["provider"], form["account"], form["models"], form["recent"], form["problem"]) == ("claude", made["account"], ["fable", "opus", "sonnet"], [], None)
            assert form["efforts"] == ["low", "medium", "high", "xhigh", "max"] and first["error"] == ""
            w.type_text(str(project))
            w.key("Tab")                                                       # the name row (no recent folder is offered yet)
            w.type_text("api")
            w.key("Down")                                                      # the model row: default, then fable, then opus
            w.key("Right", "Right")
            w.key("Down")
            w.key("Right", "Right", "Right")                                   # the effort row: default, low, medium, high
            form = screen(w)["form"]
            assert (form["folder"], form["name"], form["model"], form["effort"], form["focus"]) == (str(project), "api", "opus", "high", "Effort")
            assert conn.call(op="status")["sessions"] == []                    # choosing creates nothing
            w.key("Return")
            state = wait_for(lambda: (u := w.ui())["panel"] is None and len(u["rows"]) == 1 and u)
            entry = conn.call(op="status")["sessions"][0]
            assert (entry["cwd"], entry["name"], entry["chosen_model"], entry["chosen_effort"], entry["scratch"]) == (str(project), "api", "opus", "high", False)
            assert state["selected"] == entry["session"] and "1 api " in state["list_text"], state["list_text"]   # the list calls it by its name
            wait_for(lambda: f"CWD {project}" in lines_of(received))
            argv = argv_of(received)[-1]
            assert argv[argv.index("--model") + 1] == "opus" and argv[argv.index("--effort") + 1] == "high"
            # the next screen starts from what was used, offers the folder, and a folder that is not there is refused with the reason
            w.key("ctrl+n")
            again = screen(w)
            form = again["form"]
            assert (form["folder"], form["name"], form["model"], form["effort"], form["recent"]) == (str(project), "", "opus", "high", [str(project)])
            assert any(str(project) in line for line in again["body"][3:4]), again["body"]                  # the recent folder is a row of its own
            w.key("ctrl+u")
            w.type_text(str(project / "not_there"))
            w.key("Return")
            refused = screen(w)
            assert refused is not None and any("The session was not started. That folder does not exist." in line for line in refused["body"]), refused["body"]
            assert len(conn.call(op="status")["sessions"]) == 1
            w.key("Escape")
            assert screen(w) is None and json.dumps(w.ui()["rows"]).count(entry["session"]) == 1
