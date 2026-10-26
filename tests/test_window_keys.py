import time

from lico_measure.captures import ProductWindow, TEXT_X


def test_the_panels_text_is_on_the_screen_after_ctrl_n_and_ctrl_a_not_only_in_the_windows_model():
    with ProductWindow() as w:
        start = w.text_rows()
        assert w.ui()["panel"] is None and start >= 3, start                  # the first screen says what to do
        w.key("ctrl+n")
        panel = w.ui()["panel"]
        assert panel["title"] == "New session"
        assert w.text_rows() >= 4, w.text_rows()                              # title and the rows of the form: drawn, not just known
        assert w.bright(TEXT_X, 0, TEXT_X + 400, 120) > 300
        w.key("Escape")
        w.key("ctrl+a")
        assert w.ui()["panel"]["title"] == "Accounts"
        assert w.text_rows() >= 3, w.text_rows()


def test_the_product_window_draws_no_measurement_square_at_its_corner():
    with ProductWindow() as w:
        assert w.corner_is_background()                                      # the markers belong to the pixel-latency harness only


def test_closing_standard_input_does_not_close_the_window():
    with ProductWindow(stdin_open=False) as w:                               # what a launcher or nohup does
        time.sleep(3.0)
        assert w.alive()


def label_entry(w):
    """The window's one text field, as the folder row of the new-session screen (it has the focus when the screen opens)."""
    w.key("ctrl+n")
    assert w.ui()["panel"]["text_entry"]


def test_backspace_removes_the_last_character_and_the_prompt_row_shows_it():
    with ProductWindow() as w:
        label_entry(w)
        empty = w.prompt_ink()
        w.type_text("a")
        one = w.prompt_ink()
        w.type_text("b")
        assert w.prompt_ink() > one > empty
        w.key("BackSpace")
        assert w.ui()["panel"]["input"] == "a"
        assert w.prompt_ink() == one                                       # the same pixels as when only "a" had been typed


def test_ctrl_u_clears_the_line_and_a_held_backspace_deletes_several_characters():
    with ProductWindow() as w:
        label_entry(w)
        empty = w.prompt_ink()
        w.type_text("abc")
        w.key("ctrl+u")
        assert w.ui()["panel"]["input"] == "" and w.prompt_ink() == empty
        w.type_text("abcdefgh")
        w.hold("BackSpace", 1.6)                                             # the key repeats
        assert len(w.ui()["panel"]["input"]) <= 5


def test_backspace_edits_in_the_input_method_run_only_when_no_composition_is_open():
    with ProductWindow(ime=True) as w:
        label_entry(w)
        w.key("shift+space")                                                 # the engine starts in Latin mode; its trigger key switches to Hangul
        w.key("d", "k", "space")                                             # 아 and a space committed
        assert w.ui()["panel"]["input"] == "아 "
        w.key("BackSpace")
        assert w.ui()["panel"]["input"] == "아"                               # a committed syllable is one press
        w.key("r", "k")                                                       # 가 is being composed
        w.key("BackSpace")                                                    # belongs to the input method now
        assert w.ui()["panel"]["input"] == "아"                               # committed text is untouched
