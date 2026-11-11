"""Start one candidate on a private display with 100 sessions, show one, read the marker pixels, take a screenshot."""

from __future__ import annotations

import subprocess
import sys
import time

from lico_measure import rig
from lico_measure.xpixel import PixelReader

sys.path.insert(0, __file__.rsplit("/", 1)[0])
import toolkits  # noqa: E402

toolkits.register()


def main() -> int:
    name, shot = sys.argv[1], sys.argv[2]
    scale = int(sys.argv[3]) if len(sys.argv) > 3 else 1
    rig.DISPLAY_SCALE = scale
    xvfb = rig.Xvfb(f"{1200 * scale}x{800 * scale}x24")
    with rig.private_dir() as directory:
        host = rig.Host(directory, 100, 1, history=500)
        client = None
        try:
            client = rig.Client(name, host.socket, xvfb.display)
            client.send("load")
            client.expect("state", "loaded")
            sid = host.sessions[3]
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            time.sleep(1.5)
            reader = PixelReader(xvfb.display)
            print("session rgb expected", hex(rig.session_rgb(sid)), "got", hex(reader.pixel(8, 8)), "seq block", hex(reader.pixel(24, 8)))
            print("state", client.state())
            reader.close()
            subprocess.run(["import", "-display", xvfb.display, "-window", "root", shot], check=False)
        finally:
            if client:
                client.kill()
            host.stop()
            xvfb.stop()
    return 0


sys.exit(main())
