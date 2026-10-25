import pytest

from lico_measure import legacy
from lico_measure.ime import SENTENCE_KEYS, SENTENCE_TEXT, CompositionRig, composition_run

CANDIDATES = legacy.clients(["a", "a2", "b", "c2"])


@pytest.mark.parametrize("name", CANDIDATES)
def test_korean_composition_survives_typing_and_output(name):
    with CompositionRig() as rig:                  # one display and one input method for the four runs; each run has its own host and client
        for label, delay_ms, producing in (("normal", 80, 0), ("fast", 8, 0), ("under output", 40, 1),
                                           ("fast under output", 8, 1)):
            result = composition_run(name, delay_ms=delay_ms, producing=producing, rig=rig)
            # nothing already committed is ever removed or rewritten while typing
            assert result["committed_only_grows"], (label, result["trace"])
            # the committed sentence is exactly what the keys spell, with no stray characters
            assert result["final_text"] == SENTENCE_TEXT, (label, result["final_text"])
            assert result["ime_events"] > len(SENTENCE_KEYS) // 2, label   # the input method really took part
