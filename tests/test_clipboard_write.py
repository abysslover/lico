"""`lico clipboard write` through the real executables (class L3; plan M02-P04, S04b; XF-04): a real host, the real window (`lico gui`) on a private Xvfb, and
`xclip` as the reader that is not ours. The window is the owner of the CLIPBOARD selection, the host only relays. A window double (a protocol client that
attaches a view of kind `gui`) stands in for the window where the case is what the window does NOT do: not confirm, confirm late, confirm from the wrong
connection, or be the older of two. A raw X11 requestor written here (no library) shows the incremental transfer from outside: the announcement, the size, the
chunks and the last empty one. The owner's protocol cases that need our own requestor are in the Rust tests (`gui::ui::clip::tests`); the file cases are in
`clipboard::open_tests`."""
import json
import os
import socket
import stat
import struct
import subprocess
import threading
import time
from pathlib import Path

import pytest

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn
from lico_measure.rig import Host, Xvfb, private_dir

from tests.test_clipboard_read import forged_head, make_png

ENV = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": "/CANARY-no-home"}
TEXT_MAX = 65536
IMAGE_MAX = 3 * 1024 * 1024
CHUNK = 128 * 1024


def cli(sock, *args, timeout=30, cwd=None, display=None):
    """`lico clipboard write ...`: (exit code, the one JSON line of standard output or None, standard error)."""
    env = dict(ENV, **({"DISPLAY": display} if display else {}))
    done = subprocess.run([str(LICO), "--socket", sock, "clipboard", "write", *args], env=env, capture_output=True, text=True, timeout=timeout, cwd=cwd,
                          stdin=subprocess.DEVNULL)
    lines = done.stdout.splitlines()
    assert len(lines) <= 1, done.stdout
    return done.returncode, (json.loads(lines[0]) if lines else None), done.stderr


def xclip_out(display, target, selection="clipboard"):
    done = subprocess.run(["xclip", "-o", "-selection", selection, "-t", target], env={"DISPLAY": display, "PATH": "/usr/bin:/bin"}, capture_output=True, timeout=20)
    return done.returncode, done.stdout


def read_back(display, *args):
    done = subprocess.run([str(LICO), "clipboard", "read", *args], env=dict(ENV, DISPLAY=display), capture_output=True, text=True, timeout=30, stdin=subprocess.DEVNULL)
    return done.returncode, json.loads(done.stdout.splitlines()[0])


def padded(head, total, seed=5):
    """A file of `total` bytes that begins with `head` and goes on with noise (the window reads the header of a picture only)."""
    import random

    rng = random.Random(seed)
    return head + bytes(rng.getrandbits(8) for _ in range(total - len(head)))


@pytest.fixture(scope="module")
def win():
    """The product window on its own Xvfb with a host that has one session (the window attaches a view of kind gui to it)."""
    with ProductWindow(host_binary=LICO, host_sessions=1) as w:
        sid = ProtoConn(w.socket).call(op="session.list")["sessions"][0]["session"]
        w.proc.stdin.write(f"show {sid}\n")                          # the window opens the session: it attaches a view of kind gui
        w.proc.stdin.flush()
        end = time.monotonic() + 20
        while time.monotonic() < end:                               # the window has attached when a write is answered at all
            code, answer, _ = cli(w.socket, "--text", "ready", "--timeout-ms", "800")
            if code == 0:
                break
            time.sleep(0.3)
        else:
            raise AssertionError(f"the window never attached a view of kind gui: {answer}")
        yield w


# ---- a minimal X11 requestor, by hand, to see the incremental transfer from outside --------------------------------------------------


class RawX:
    """Just enough of the X11 wire protocol to ask the CLIPBOARD owner for a target and to follow INCR. Little endian, no authority (the private Xvfb)."""

    def __init__(self, display):
        number = display.lstrip(":").split(".")[0]
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.connect(f"/tmp/.X11-unix/X{number}")
        self.sock.settimeout(10)
        self.seq = 0
        self.events = []
        self.sock.sendall(struct.pack("<BxHHHHxx", ord("l"), 11, 0, 0, 0))
        head = self.read_exact(8)
        assert head[0] == 1, "the X server refused the connection"
        body = self.read_exact(struct.unpack("<H", head[6:8])[0] * 4)
        self.base, self.mask = struct.unpack_from("<II", body, 4)
        vendor, formats = struct.unpack_from("<H", body, 16)[0], body[21]
        self.root = struct.unpack_from("<I", body, 32 + ((vendor + 3) & ~3) + 8 * formats)[0]
        self.next_id = 0
        self.window = self.new_id()
        # an InputOnly window that hears property changes (event mask PropertyChange = 0x400000)
        self.send(1, 0, struct.pack("<IIhhHHHHII", self.window, self.root, 0, 0, 1, 1, 0, 2, 0, 0x800) + struct.pack("<I", 0x400000), pad_to_words=True)
        self.names = {}

    def new_id(self):
        self.next_id += 1
        return self.base | (self.next_id & self.mask)

    def read_exact(self, n):
        data = b""
        while len(data) < n:
            chunk = self.sock.recv(n - len(data))
            if not chunk:
                raise EOFError
            data += chunk
        return data

    def send(self, opcode, data1, body, pad_to_words=False):
        body = body + b"\0" * (-len(body) % 4)
        self.seq += 1
        self.sock.sendall(struct.pack("<BBH", opcode, data1, 1 + len(body) // 4) + body)

    def message(self):
        """The next 32-byte message and, for a reply, its extra data: ('reply', fixed, extra), ('event', fixed), or ('error', fixed)."""
        fixed = self.read_exact(32)
        if fixed[0] == 1:
            extra = self.read_exact(struct.unpack_from("<I", fixed, 4)[0] * 4)
            return "reply", fixed, extra
        if fixed[0] == 0:
            return "error", fixed, b""
        return "event", fixed, b""

    def reply(self):
        while True:
            kind, fixed, extra = self.message()
            if kind == "reply":
                return fixed, extra
            assert kind != "error", f"X error {fixed[1]}"
            self.events.append(fixed)

    def atom(self, name):
        raw = name.encode()
        self.send(16, 0, struct.pack("<HH", len(raw), 0) + raw)
        atom = struct.unpack_from("<I", self.reply()[0], 8)[0]
        self.names[atom] = name
        return atom

    def atom_name(self, atom):
        if atom not in self.names:
            self.send(17, 0, struct.pack("<I", atom))
            fixed, extra = self.reply()
            self.names[atom] = extra[: struct.unpack_from("<H", fixed, 8)[0]].decode()
        return self.names[atom]

    def owner(self, selection):
        self.send(23, 0, struct.pack("<I", selection))
        return struct.unpack_from("<I", self.reply()[0], 8)[0]

    def convert(self, selection, target, prop):
        self.send(24, 0, struct.pack("<IIIII", self.window, selection, target, prop, 0))

    def next_event(self, code, timeout=10):
        end = time.monotonic() + timeout
        while True:
            for i, e in enumerate(self.events):
                if e[0] & 0x7F == code:
                    return self.events.pop(i)
            self.sock.settimeout(max(0.05, end - time.monotonic()))
            kind, fixed, _ = self.message()
            assert kind != "error", f"X error {fixed[1]}"
            if kind == "event":
                self.events.append(fixed)
            if time.monotonic() > end:
                raise TimeoutError

    def get_property(self, prop, delete=True):
        """(type atom, format, data) of the property of our window."""
        self.send(20, 1 if delete else 0, struct.pack("<IIIII", self.window, prop, 0, 0, 0x3FFFFFFF))
        fixed, extra = self.reply()
        fmt, ptype, _, count = fixed[1], *struct.unpack_from("<III", fixed, 8)
        return ptype, fmt, extra[: count * (fmt // 8)]

    def send_selection_request(self, to_window, selection, target, prop):
        """A forged SelectionRequest to a window (event code 30) as the server would send it: for the window's own client to answer."""
        event = struct.pack("<BxHIIIIII", 30, 0, 0, to_window, self.window, selection, target, prop) + b"\0" * 4
        self.send(25, 0, struct.pack("<II", to_window, 0) + event)

    def ask(self, target_name, selection_name="CLIPBOARD", stall=False):
        """Asks the owner. Returns ('refused',) or ('data', type, bytes) or ('incr', announced, chunk sizes, bytes); with `stall`, stops after the announcement."""
        selection, target, prop = self.atom(selection_name), self.atom(target_name), self.atom("LICO_RAW_PROP")
        incr = self.atom("INCR")
        self.convert(selection, target, prop)
        notify = self.next_event(31)
        if struct.unpack_from("<I", notify, 20)[0] == 0:
            return ("refused",)
        self.events = [e for e in self.events if e[0] & 0x7F != 28]          # the NewValue of the announcement itself (it came before the SelectionNotify) is not a chunk
        ptype, fmt, data = self.get_property(prop)
        if ptype != incr:
            return ("data", self.atom_name(ptype), data)
        announced = struct.unpack("<I", data[:4])[0]
        if stall:
            return ("incr", announced, [], b"")
        sizes, out = [], b""
        while True:
            event = self.next_event(28, timeout=15)                      # PropertyNotify
            _, atom, _, state = struct.unpack_from("<IIIB", event, 4)
            if atom != prop or state != 0:                                # only NewValue of our property
                continue
            _, _, chunk = self.get_property(prop)
            sizes.append(len(chunk))
            if not chunk:
                return ("incr", announced, sizes, out)
            out += chunk

    def close(self):
        self.sock.close()


# ---- the real window ----------------------------------------------------------------------------------------------------------------


def test_texts_are_written_and_read_back_exactly_by_two_readers_and_the_text_limit_is_exact(win):
    display = win.xvfb.display
    for text in ["plain ascii text", "파일을 읽습니다 안녕하세요", "an emoji \U0001F642 here", "two\nlines\n", "quotes \" back\\slash\ttab", "x"]:
        code, answer, err = cli(win.socket, "--text", text)
        assert (code, answer) == (0, {"ok": 1, "kind": "text", "bytes": len(text.encode())}), (answer, err)
        assert xclip_out(display, "UTF8_STRING") == (0, text.encode()), "reader 1: xclip"
        code, got = read_back(display, "--kind", "text")
        assert (code, got["text"]) == (0, text), "reader 2: lico clipboard read"
    big = "a" * TEXT_MAX
    code, answer, _ = cli(win.socket, "--text", big)
    assert (code, answer) == (0, {"ok": 1, "kind": "text", "bytes": TEXT_MAX}), "64 KiB exactly"
    assert xclip_out(display, "UTF8_STRING") == (0, big.encode())
    code, answer, _ = cli(win.socket, "--text", big + "a")
    assert (code, answer["error"]) == (2, "too_large"), "one byte more"
    assert xclip_out(display, "UTF8_STRING") == (0, big.encode()), "the refused write changed nothing"
    with private_dir() as d:                                          # a text file of the same size is read by the safe open, with the same limit
        exact, over = Path(d) / "exact.txt", Path(d) / "over.txt"
        exact.write_bytes("한".encode() * 21845 + b"a")               # 65536 bytes
        over.write_bytes(exact.read_bytes() + b"a")
        for f in (exact, over):
            f.chmod(0o600)
        assert cli(win.socket, "--text-file", str(exact))[:2] == (0, {"ok": 1, "kind": "text", "bytes": TEXT_MAX})
        assert xclip_out(display, "UTF8_STRING") == (0, exact.read_bytes())
        code, answer, _ = cli(win.socket, "--text-file", str(over))
        assert (code, answer["error"]) == (2, "too_large")


def test_a_small_picture_is_written_and_read_back_equal_to_the_file_by_two_readers(win):
    display = win.xvfb.display
    png, _ = make_png(40, 30)
    with private_dir() as d:
        src = Path(d) / "small.png"
        src.write_bytes(png)
        src.chmod(0o600)
        code, answer, err = cli(win.socket, "--image", str(src))
        assert (code, answer) == (0, {"ok": 1, "kind": "image", "bytes": len(png), "width": 40, "height": 30}), (answer, err)
        assert xclip_out(display, "image/png") == (0, png), "reader 1: xclip"
        out = Path(d) / "back.png"
        code, got = read_back(display, "--kind", "image", "--out", str(out))
        assert (code, got["bytes"], got["width"], got["height"]) == (0, len(png), 40, 30) and out.read_bytes() == png, "reader 2: lico clipboard read"
        # a relative name is taken against the working folder of the command
        code, answer, _ = cli(win.socket, "--image", "small.png", cwd=d)
        assert (code, answer["bytes"]) == (0, len(png))


def test_a_picture_of_two_megabytes_of_noise_goes_by_incr_and_arrives_whole(win):
    display = win.xvfb.display
    png, pixels = make_png(800, 900, noise=True)
    assert 2_000_000 < len(png) < IMAGE_MAX
    with private_dir() as d:
        src = Path(d) / "noise.png"
        src.write_bytes(png)
        src.chmod(0o600)
        code, answer, _ = cli(win.socket, "--image", str(src))
        assert (code, answer) == (0, {"ok": 1, "kind": "image", "bytes": len(png), "width": 800, "height": 900}), answer
        code, data = xclip_out(display, "image/png")
        assert (code, data == png) == (0, True), "xclip got every byte, in order"
        out = Path(d) / "noise-back.png"
        code, got = read_back(display, "--kind", "image", "--out", str(out))
        assert code == 0 and out.read_bytes() == png and got["bytes"] == len(png), "lico clipboard read got every byte too"
        # the incremental transfer, seen from outside by a requestor that follows the protocol by hand
        raw = RawX(display)
        try:
            kind, announced, sizes, data = raw.ask("image/png")
            assert kind == "incr", "the value is over the threshold: it must go by INCR"
            assert announced == len(png), "the announcement carries the size"
            assert data == png
            assert all(s <= CHUNK for s in sizes), sizes
            assert sizes[-1] == 0 and sizes.count(0) == 1, "the last chunk is empty and only the last"
            assert len(sizes) == -(-len(png) // CHUNK) + 1
            print(f"INCR evidence: announced={announced} chunks={len(sizes)} largest={max(sizes)} last={sizes[-1]}")
        finally:
            raw.close()


def test_a_second_write_replaces_the_first_and_targets_follow_what_is_held_and_the_other_form_is_refused(win):
    display = win.xvfb.display
    png, _ = make_png(20, 10)
    with private_dir() as d:
        src = Path(d) / "p.png"
        src.write_bytes(png)
        src.chmod(0o600)
        raw = RawX(display)
        try:
            assert cli(win.socket, "--text", "first")[0] == 0
            assert cli(win.socket, "--text", "second")[0] == 0
            assert xclip_out(display, "UTF8_STRING") == (0, b"second"), "the second write replaced the first"
            targets = xclip_out(display, "TARGETS")[1].decode().split()
            assert targets == ["TARGETS", "UTF8_STRING", "STRING", "TEXT"], targets
            code, data = xclip_out(display, "image/png")
            assert data == b"", "image/png asked while text is held is refused"
            assert raw.ask("image/png") == ("refused",)
            assert cli(win.socket, "--image", str(src))[0] == 0
            assert xclip_out(display, "TARGETS")[1].decode().split() == ["TARGETS", "image/png"]
            assert xclip_out(display, "image/png") == (0, png), "the picture replaced the text"
            for target in ("UTF8_STRING", "STRING", "TEXT"):
                assert raw.ask(target) == ("refused",), target
            assert xclip_out(display, "UTF8_STRING")[1] == b""
            assert cli(win.socket, "--text", "third")[0] == 0
            assert xclip_out(display, "image/png")[1] == b"", "and back: text replaced the picture"
        finally:
            raw.close()


def test_another_client_taking_the_selection_makes_the_window_release_its_content(win):
    display = win.xvfb.display
    assert cli(win.socket, "--text", "mine")[0] == 0
    raw = RawX(display)
    thief = None
    try:
        clipboard, utf8, prop = raw.atom("CLIPBOARD"), raw.atom("UTF8_STRING"), raw.atom("LICO_RAW_PROP")
        mine = raw.owner(clipboard)
        assert mine != 0
        thief = subprocess.Popen(
            ["xclip", "-quiet", "-selection", "clipboard", "-i"], env={"DISPLAY": display, "PATH": "/usr/bin:/bin"}, stdin=subprocess.PIPE,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        thief.stdin.write(b"taken by another client")
        thief.stdin.close()
        for _ in range(100):
            if raw.owner(clipboard) not in (0, mine):
                break
            time.sleep(0.05)
        assert raw.owner(clipboard) not in (0, mine), "the other client owns the selection now"
        assert xclip_out(display, "UTF8_STRING") == (0, b"taken by another client")
        # the window's helper window is asked directly, as a requestor that raced with the change would: it must refuse
        raw.send_selection_request(mine, clipboard, utf8, prop)
        notify = raw.next_event(31, timeout=5)
        assert struct.unpack_from("<I", notify, 20)[0] == 0, "the window answered with content it should have released"
        # and the window can write again: a later write takes the selection back
        assert cli(win.socket, "--text", "back again")[0] == 0
        assert xclip_out(display, "UTF8_STRING") == (0, b"back again")
    finally:
        raw.close()
        if thief:
            thief.kill()
            thief.wait()


def test_a_stalled_requestor_of_a_large_picture_does_not_block_a_second_requestor(win):
    display = win.xvfb.display
    png, _ = make_png(800, 900, noise=True)
    with private_dir() as d:
        src = Path(d) / "noise.png"
        src.write_bytes(png)
        src.chmod(0o600)
        assert cli(win.socket, "--image", str(src))[0] == 0
    stalled = RawX(display)
    try:
        assert stalled.ask("image/png", stall=True)[0] == "incr"       # deleted the announcement: the first chunk is written; then nothing
        began = time.monotonic()
        code, data = xclip_out(display, "image/png")
        assert code == 0 and data == png, "the second requestor got the whole picture"
        assert time.monotonic() - began < 6, "the second requestor waited for the stalled one"
        # and a text write still works at once afterwards
        assert cli(win.socket, "--text", "after the stall", timeout=10)[0] == 0
    finally:
        stalled.close()


def test_bad_files_are_refused_with_their_reason_and_a_good_one_of_the_exact_limit_is_taken(win):
    display = win.xvfb.display
    assert cli(win.socket, "--text", "kept")[0] == 0
    with private_dir() as d:
        folder = Path(d)

        def put(name, data, mode=0o600):
            p = folder / name
            p.write_bytes(data)
            p.chmod(mode)
            return str(p)

        def refused(path, reason):
            code, answer, _ = cli(win.socket, "--image", path)
            assert (code, answer["ok"], answer["error"]) == (2, 0, reason), (path, answer)
            assert xclip_out(display, "UTF8_STRING") == (0, b"kept"), f"{path}: the held value changed"

        real = put("real.png", make_png(10, 10)[0])
        link = folder / "link.png"
        link.symlink_to(real)
        refused(str(link), "write_failed")                                           # a symbolic link at the last name is not followed
        refused(str(folder), "write_failed")                                         # a folder
        fifo = folder / "pipe.png"
        os.mkfifo(fifo)
        began = time.monotonic()
        refused(str(fifo), "write_failed")                                           # a pipe: not opened for reading, no hang
        assert time.monotonic() - began < 5
        refused(str(folder / "missing.png"), "write_failed")
        refused("/etc/passwd", "write_failed")                                       # a regular file of another user
        refused(put("empty.png", b""), "bad_image")
        refused(put("words.png", b"this is not a picture, just words " * 5), "bad_image")
        refused(put("jpeg.png", b"\xff\xd8\xff\xe0" + b"jpeg" * 100), "bad_image")
        refused(put("wide.png", padded(forged_head(9000, 20), 400)), "too_large")   # a side over 8000 pixels
        refused(put("high.png", padded(forged_head(20, 9000), 400)), "too_large")
        refused(put("over.png", padded(forged_head(10, 10), IMAGE_MAX + 1)), "too_large")   # 3,145,729 bytes
        exact = put("exact.png", padded(forged_head(10, 10), IMAGE_MAX))                       # 3,145,728 bytes
        code, answer, _ = cli(win.socket, "--image", exact)
        assert (code, answer) == (0, {"ok": 1, "kind": "image", "bytes": IMAGE_MAX, "width": 10, "height": 10}), answer
        assert xclip_out(display, "image/png")[1] == Path(exact).read_bytes()
        # a file of the user's that others can write is still a file of the user's: the picture is not a secret; the setting file is judged elsewhere
        assert cli(win.socket, "--image", put("open.png", make_png(5, 5)[0], 0o666))[0] == 0


def test_usage_errors_exit_one_and_say_what_is_wrong_and_a_missing_host_is_said_as_the_other_commands_say_it(win):
    sock = win.socket
    for args in [(), ("--text",), ("--text", ""), ("--text", "a", "--image", "/x.png"), ("--text", "a", "--text", "b"), ("--image",), ("--image", ""),
                 ("--text", "a", "--timeout-ms", "0"), ("--text", "a", "--timeout-ms", "600001"), ("--text", "a", "--timeout-ms", "abc"),
                 ("--text", "a", "--timeout-ms", "-1"), ("--text", "a", "--bogus", "1"), ("--timeout-ms", "100"), ("hello",)]:
        code, answer, _ = cli(sock, *args)
        assert (code, answer["ok"], answer["error"]) == (1, 0, "usage") and answer["detail"], (args, code, answer)
    code, out, err = cli("/nonexistent/host.sock", "--text", "a")
    assert code == 3 and out is None and err.startswith("cannot reach the host"), (code, out, err)
    # a request line that the host cannot take (control characters cost six bytes each in JSON) is too_large, not a hang
    code, answer, _ = cli(sock, "--text", "\x01" * TEXT_MAX)
    assert (code, answer["error"]) == (2, "too_large")


# ---- the host with a window double --------------------------------------------------------------------------------------------------


@pytest.fixture()
def host():
    with private_dir() as directory:
        h = Host(directory, sessions=1, producing=0, impl="hostd")
        try:
            yield h
        finally:
            h.stop()


def double(h, kind="gui"):
    """A connection that attaches a view of the first session, as a window does."""
    c = ProtoConn(h.socket)
    made = c.call(op="view.attach", session=h.sessions[0], kind=kind, after=-1)
    assert made["ok"] == 1, made
    return c


def wait_for_event(c, name="clip.write", timeout=5):
    c.sock.settimeout(timeout)
    while True:
        m = c.read()
        if m.get("ev") == name:
            return m


def run_async(sock, *args):
    box = {}

    def go():
        began = time.monotonic()
        box["result"] = cli(sock, *args)
        box["seconds"] = time.monotonic() - began

    t = threading.Thread(target=go)
    t.start()
    return t, box


def test_without_a_window_the_answer_is_no_window_in_under_a_second(host):
    began = time.monotonic()
    code, answer, _ = cli(host.socket, "--text", "x", "--timeout-ms", "5000")
    assert (code, answer["ok"], answer["error"]) == (2, 0, "no_window") and time.monotonic() - began < 1.0
    cli_view = double(host, kind="cli")                         # a view that is not a window does not count
    began = time.monotonic()
    code, answer, _ = cli(host.socket, "--image", "/etc/hostname", "--timeout-ms", "5000")
    assert (code, answer["error"]) == (2, "no_window") and time.monotonic() - began < 1.0
    cli_view.close()
    window = double(host)                                            # a window that has gone is no window either
    window.close()
    time.sleep(0.3)
    began = time.monotonic()
    code, answer, _ = cli(host.socket, "--text", "x", "--timeout-ms", "5000")
    assert (code, answer["error"]) == (2, "no_window") and time.monotonic() - began < 2.0


def test_a_window_that_does_not_confirm_gives_timeout_within_the_limit_and_a_late_confirmation_changes_nothing(host):
    window = double(host)
    thread, box = run_async(host.socket, "--text", "hello", "--timeout-ms", "700")
    pushed = wait_for_event(window)
    assert (pushed["kind"], pushed["text"]) == ("text", "hello") and pushed["req"].startswith("c")
    thread.join(10)
    code, answer, _ = box["result"]
    assert (code, answer["ok"], answer["error"]) == (2, 0, "timeout")
    assert 0.6 <= box["seconds"] <= 1.7, box["seconds"]               # within --timeout-ms + 1 s
    late = window.call(op="clipboard.done", req=pushed["req"], ok=True, kind="text", bytes=5)
    assert (late["ok"], late["error"]) == (0, "unknown_request"), "the request was forgotten at the timeout"
    again = window.call(op="clipboard.done", req=pushed["req"], ok=True, kind="text", bytes=5)
    assert again["error"] == "unknown_request"


def test_a_window_that_goes_away_while_the_host_waits_ends_the_wait_with_no_window(host):
    window = double(host)
    thread, box = run_async(host.socket, "--text", "hello", "--timeout-ms", "20000")
    wait_for_event(window)
    window.close()
    thread.join(10)
    assert box["result"][1]["error"] == "no_window" and box["seconds"] < 5


def test_the_newest_window_gets_the_request_the_older_one_never_sees_it_and_only_the_right_confirmation_counts(host):
    older = double(host)
    newest = double(host)
    thread, box = run_async(host.socket, "--text", "for the newest", "--timeout-ms", "6000")
    pushed = wait_for_event(newest)
    assert pushed["text"] == "for the newest"
    older.sock.settimeout(0.7)
    with pytest.raises((socket.timeout, TimeoutError)):
        while True:
            m = older.read()
            assert m.get("ev") != "clip.write", "the older window was sent the request"
    older.sock.settimeout(5)
    req = pushed["req"]
    wrong_connection = older.call(op="clipboard.done", req=req, ok=True, kind="text", bytes=14)
    assert (wrong_connection["ok"], wrong_connection["error"]) == (0, "unknown_request")
    wrong_id = newest.call(op="clipboard.done", req="c" + "0" * 16, ok=True, kind="text", bytes=14)
    assert (wrong_id["ok"], wrong_id["error"]) == (0, "unknown_request")
    time.sleep(0.2)
    assert thread.is_alive(), "neither a foreign confirmation nor a wrong id ended the wait"
    right = newest.call(op="clipboard.done", req=req, ok=True, kind="text", bytes=14)
    assert right["ok"] == 1
    thread.join(10)
    assert box["result"][:2] == (0, {"ok": 1, "kind": "text", "bytes": 14})


def test_the_window_reports_its_own_failure_and_the_command_says_it(host):
    window = double(host)
    for said, wanted in [("bad_image", "bad_image"), ("too_large", "too_large"), ("write_failed", "write_failed"), ("x_error", "x_error"), ("surprise", "x_error")]:
        thread, box = run_async(host.socket, "--image", "/etc/hostname", "--timeout-ms", "5000")
        pushed = wait_for_event(window)
        assert (pushed["kind"], pushed["path"]) == ("image", "/etc/hostname") and "text" not in pushed
        assert window.call(op="clipboard.done", req=pushed["req"], ok=False, error=said)["ok"] == 1
        thread.join(10)
        code, answer, _ = box["result"]
        assert (code, answer["error"]) == (2, wanted), said


def test_the_host_refuses_malformed_requests_and_confirmations(host):
    c = ProtoConn(host.socket)
    bad = [dict(op="clipboard.write"), dict(op="clipboard.write", kind="text"), dict(op="clipboard.write", kind="text", text=""),
           dict(op="clipboard.write", kind="image", path="relative.png"), dict(op="clipboard.write", kind="text", text="x", path="/a"),
           dict(op="clipboard.write", kind="text", text="x", timeout_ms=0), dict(op="clipboard.write", kind="text", text="x", extra=1),
           dict(op="clipboard.done"), dict(op="clipboard.done", req="c1", ok=1), dict(op="clipboard.done", req="c1", ok=True, width=0)]
    for request in bad:
        answer = c.call(**request)
        assert answer["ok"] == 0 and answer["error"] in ("bad_request", "invalid_request", "bad_field", "unknown_request"), (request, answer)
    assert c.call(op="clipboard.done", req="cabcdef", ok=True)["error"] == "unknown_request"
    assert "clipboard.write" in c.call(op="hello")["ops"] and "clipboard.done" in c.call(op="hello")["ops"]

