import json
from common import observations

for observation in observations():
    print(json.dumps({"turn": observation["turn"], "actions": [{"robot_id": 42, "action": "WAIT"}]}), flush=True)
