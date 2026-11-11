"""usage: run_inputs.py CANDIDATE SCENARIO   (SCENARIO: free, menu, korean_menu) -> prints JSON and writes data/inputs_<candidate>_<scenario>.json"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402

name, scenario = sys.argv[1], sys.argv[2]
fn = {"free": harness.free_controls, "menu": harness.menu_keyboard, "korean_menu": harness.korean_in_menu}[scenario]
result = fn(name)
out = Path(__file__).resolve().parent / "data" / f"inputs_{name}_{scenario}.json"
out.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n")
print(json.dumps(result, indent=1, ensure_ascii=False))
