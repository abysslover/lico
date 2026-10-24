import time

from lico_measure.captures import ProductWindow, TEXT_X


def test_the_panels_text_is_on_the_screen_after_ctrl_n_and_ctrl_a_not_only_in_the_windows_model():
    with ProductWindow() as w:
        start = w.text_rows()
        assert w.ui()["panel"] is None and start >= 3, start                  # the first screen says what to do
        w.key("ctrl+n")
        panel = w.ui()["panel"]
        assert panel["title"].startswith("New session: which provider")
        assert w.text_rows() >= 4, w.text_rows()                              # title, two providers, the way out: drawn, not just known
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
