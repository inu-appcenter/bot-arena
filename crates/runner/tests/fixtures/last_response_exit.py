from common import observations, respond

for observation in observations():
    respond(observation)
    if observation["turn"] == 200:
        raise SystemExit(0)
