"""Test-only process markers and JSON helpers; never used by the application."""

import json
import os
from pathlib import Path
import sys
import time

script = Path(sys.argv[0])
script.with_suffix(".pid").write_text(str(os.getpid()), encoding="utf-8")
# Let both test markers exist before an immediate-error fixture responds. This
# avoids racing the peer interpreter's startup when checking both child PIDs.
deadline = time.monotonic() + 2
while not all((script.parent / name).exists() for name in ("a.pid", "b.pid")):
    if time.monotonic() >= deadline:
        raise RuntimeError("both fixture processes did not start")
    time.sleep(0.005)


def observations():
    for line in sys.stdin:
        yield json.loads(line)


def respond(observation):
    print(json.dumps({"turn": observation["turn"], "actions": []}), flush=True)
