from pathlib import Path
import sys
import unittest

SDK_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SDK_ROOT))
sys.path.insert(0, str(SDK_ROOT / "examples"))

from arena_sdk import DIRECTIONS, Observation
from basic_bot import BasicBot
from strategic_bot import StrategicBot
from test_sdk import snapshot


def moves_by_destination(observation, actions):
    robots = {robot["id"]: robot for robot in observation.robots}
    destinations = []
    for action in actions:
        if action["action"] == "MOVE":
            robot = robots[action["robot_id"]]
            dx, dy = DIRECTIONS[action["direction"]]
            destinations.append((robot["x"] + dx, robot["y"] + dy))
    return destinations


class ExampleStrategyTests(unittest.TestCase):
    def test_both_bots_pick_then_return_full_load(self):
        for strategy_type in (BasicBot, StrategicBot):
            for team, depot_x, resource_x, direction in (("A", 0, 3, "LEFT"), ("B", 14, 11, "RIGHT")):
                with self.subTest(bot=strategy_type.__name__, team=team):
                    data = snapshot(team=team, resources=[{"x": resource_x, "y": 3, "amount": 4}])
                    robot = next(robot for robot in data["state"]["robots"] if robot["id"] == f"{team}0")
                    robot["x"] = resource_x
                    strategy = strategy_type()
                    observation = Observation.from_json(data)
                    pick_action = next(action for action in strategy(observation) if action["robot_id"] == robot["id"])
                    self.assertEqual(pick_action["action"], "PICK")
                    robot["cargo"] = 4
                    data["state"]["resources"] = []
                    data["turn"] = 2
                    return_action = next(action for action in strategy(Observation.from_json(data)) if action["robot_id"] == robot["id"])
                    self.assertEqual(return_action["direction"], direction)
                    self.assertEqual(strategy.turns_seen, 2)
                    self.assertEqual(strategy_type().turns_seen, 0)

    def test_strategic_bot_assigns_distinct_targets_and_destinations(self):
        resources = [{"x": x, "y": y, "amount": 4} for x, y in ((3, 5), (3, 7), (3, 9), (7, 7))]
        observation = Observation.from_json(snapshot(resources=resources))
        strategy = StrategicBot()
        actions = strategy(observation)
        self.assertEqual(len(strategy.targets), 3)
        self.assertEqual(len(set(strategy.targets.values())), 3)
        destinations = moves_by_destination(observation, actions)
        self.assertEqual(len(destinations), len(set(destinations)))
        occupied = {(robot["x"], robot["y"]) for robot in observation.state["robots"]}
        self.assertTrue(occupied.isdisjoint(destinations))

    def test_strategic_bot_reroutes_after_unchanged_move_position(self):
        data = snapshot(resources=[{"x": 3, "y": y, "amount": 4} for y in (3, 7, 11)])
        strategy = StrategicBot()
        observation = Observation.from_json(data)
        first = strategy(observation)
        self.assertEqual(next(action for action in first if action["robot_id"] == "A0")["direction"], "RIGHT")
        data["turn"] = 2
        # The server still reports the same positions: previous moves failed.
        second = strategy(Observation.from_json(data))
        self.assertNotEqual(next(action for action in second if action["robot_id"] == "A0").get("direction"), "RIGHT")

    def test_strategic_bot_returns_partial_cargo_before_turn_limit(self):
        data = snapshot(turn=198, resources=[{"x": 5, "y": 3, "amount": 4}])
        data["state"]["robots"][0].update(x=2, cargo=1)
        action = next(action for action in StrategicBot()(Observation.from_json(data)) if action["robot_id"] == "A0")
        self.assertEqual(action, {"robot_id": "A0", "action": "MOVE", "direction": "LEFT"})

    def test_basic_target_memory_survives_observations(self):
        data = snapshot(resources=[{"x": 3, "y": 3, "amount": 4}])
        strategy = BasicBot()
        strategy(Observation.from_json(data))
        self.assertEqual(strategy.targets["A0"], (3, 3))
        data["state"]["robots"][0].update(x=1)
        data["turn"] = 2
        strategy(Observation.from_json(data))
        self.assertEqual(strategy.targets["A0"], (3, 3))
        self.assertEqual(BasicBot().targets, {})


if __name__ == "__main__":
    unittest.main()
