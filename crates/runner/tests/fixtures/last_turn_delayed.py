import time
from common import observations, respond

for observation in observations():
    if observation["turn"] == 200:
        # Ensure the opponent has exited before the final simultaneous state.
        time.sleep(0.1)
    respond(observation)
