"""usage: calibrate.py CANDIDATE X Y [X Y ...]  clicks the points in order on a private display and saves data/calibration/<candidate>_cal.png"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402

name, nums = sys.argv[1], [int(v) for v in sys.argv[2:]]
with harness.Scene(name) as scene:
    for x, y in zip(nums[::2], nums[1::2]):
        scene.click(x, y)
    scene.shot(str(Path(__file__).resolve().parent / "data" / "calibration" / f"{name}_cal.png"))
    print(scene.notes())
