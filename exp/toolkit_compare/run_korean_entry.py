"""usage: run_korean_entry.py CANDIDATE -> composition of the sentence in the entry line through the private ibus (rig experiment, unchanged)"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import toolkits  # noqa: E402

toolkits.register()
from lico_measure import ime  # noqa: E402

name = sys.argv[1]
result = ime.composition_run(name)
out = Path(__file__).resolve().parent / "data" / f"korean_entry_{name}.json"
out.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n")
print(json.dumps({k: v for k, v in result.items() if k != "trace"}, ensure_ascii=False), result["trace"][-6:])
