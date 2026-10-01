import json
from common import observations

for observation in observations():
    print(json.dumps({"turn": observation["turn"] + 1, "actions": []}), flush=True)
