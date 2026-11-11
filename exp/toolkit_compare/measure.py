"""Drive the unchanged M01-P01 rig over the reference client (a2) and the three toolkit clients.

usage: measure.py OUT_DIR SCALE [CANDIDATES] [ONLY]
The only adaptations are in this file and toolkits.py: the toolkit clients are registered as candidates, the Korean test is allowed
for them, and the rig's private root is the short /dev/shm path (unix socket length). Git status is read from the real checkout.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import toolkits  # noqa: E402

toolkits.register()
from lico_measure import __main__ as rig_main  # noqa: E402

rig_main.REPO = toolkits.REPO              # git status and the commit come from the real checkout
rig_main.ONLY_FOR["korean_ime"] = set(rig_main.ONLY_FOR["korean_ime"]) | set(toolkits.NAMES)

out, scale = sys.argv[1], sys.argv[2]
candidates = sys.argv[3] if len(sys.argv) > 3 else "a2," + ",".join(toolkits.NAMES)
only = sys.argv[4] if len(sys.argv) > 4 else "idle_memory,hidden_growth,activation,restart,idle_cpu,inventory,korean_ime"
sys.argv = ["lico_measure", "run", "--out", out, "--candidates", candidates, "--only", only, "--text", "--ui", "--scale", scale]
sys.exit(rig_main.main())
