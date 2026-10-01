import hashlib
import json
from pathlib import Path
import sys
import time
from common import observations, respond

script = Path(sys.argv[0])
peer = "b" if script.stem == "a" else "a"
for observation in observations():
    turn = observation["turn"]
    # Only a test checksum is written, not a replay or state snapshot. Neither
    # bot responds until the peer has received this same action-turn snapshot.
    digest = hashlib.sha256(json.dumps({"turn": turn, "state": observation["state"]}, sort_keys=True).encode()).hexdigest()
    marker = script.parent / f"{script.stem}.{turn}.sha256"
    temporary = marker.with_suffix(".tmp")
    temporary.write_text(digest, encoding="utf-8")
    temporary.replace(marker)
    peer_marker = script.parent / f"{peer}.{turn}.sha256"
    deadline = time.monotonic() + 0.5
    while not peer_marker.exists():
        if time.monotonic() >= deadline:
            print("runner did not request both teams concurrently", file=sys.stderr, flush=True)
            raise SystemExit(2)
        time.sleep(0.001)
    if peer_marker.read_text(encoding="utf-8") != digest:
        print("teams received different starting snapshots", file=sys.stderr, flush=True)
        raise SystemExit(3)
    respond(observation)
