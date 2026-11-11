import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import toolkits  # noqa: E402

toolkits.register()
from lico_measure import ime, rig  # noqa: E402

rig.CLIENT_ARGS["a2"] = ["--ui"]
result = ime.composition_run("a2")
print(json.dumps({k: v for k, v in result.items() if k != "trace"}, ensure_ascii=False), result["trace"])
