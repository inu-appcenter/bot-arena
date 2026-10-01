import time
from common import observations, respond

for observation in observations():
    time.sleep(5)
    respond(observation)
