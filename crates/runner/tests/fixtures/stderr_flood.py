import sys
import time
from common import observations

for observation in observations():
    sys.stderr.write("x" * 524288 + "\nTAIL-MARKER\n")
    sys.stderr.flush()
    time.sleep(0.03)
    print("invalid-json-after-drained-stderr", flush=True)
