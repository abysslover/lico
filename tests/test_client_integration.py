import pytest

from lico_measure.p02 import gui_kill, view_switch

CLIENTS = ["a2", "c2"]


@pytest.mark.parametrize("name", CLIENTS)
def test_gui_kill_and_reattach_keeps_agent_and_sequence(name):
    r = gui_kill(name)
    assert r["views_attached"] == 1 and r["subscribers_attached"] == 1        # the view is registered with the host
    assert r["views_after_kill"] == 0 and r["subscribers_after_kill"] == 0    # the dead view is gone
    assert r["agent_pid_unchanged"] and r["agent_alive"]                      # the same agent process, still alive
    assert r["records_while_gui_absent"] >= 400, r                            # output went on without any GUI
    assert r["log_contiguous"]                                                # durable and contiguous
    assert r["view_resident"] and r["restarted_gaps"] == 0 and r["restarted_dups"] == 0
    assert r["restarted_cursor"] >= r["host_seq"] - 5                         # the restarted GUI shows current content
    assert r["views_after_restart"] == 1


@pytest.mark.parametrize("name", CLIENTS)
def test_view_switch_never_sends_composition_to_another_session(name):
    r = view_switch(name)
    # the composition in progress when the view was switched is committed to the session
    # it was typed for (녕 joins 안); it never reaches the other session
    assert r["first_session_inputs"] == r["expected_first"], r
    assert r["second_session_inputs"] == r["expected_second"], r
    assert all("녕" not in text for text in r["second_session_inputs"])
