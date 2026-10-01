import json
from common import observations

for observation in observations():
    print(json.dumps({"turn": observation["turn"], "actions": [False]}), flush=True)
