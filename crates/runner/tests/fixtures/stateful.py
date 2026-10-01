import sys
from common import observations, respond

count = 0
for observation in observations():
    count += 1
    if observation["turn"] != count:
        print(f"process counter {count} != turn {observation['turn']}", file=sys.stderr, flush=True)
        raise SystemExit(2)
    respond(observation)
