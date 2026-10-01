import io
import json
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import unittest

SDK_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SDK_ROOT))

from arena_sdk import Observation, move, run_bot, shortest_path, wait


def snapshot(team="A", turn=1, robots=None, resources=None, obstacles=None):
    if robots is None:
        robots = [
            {"id": "A0", "team": "A", "x": 0, "y": 3, "cargo": 0},
            {"id": "A1", "team": "A", "x": 0, "y": 7, "cargo": 0},
            {"id": "A2", "team": "A", "x": 0, "y": 11, "cargo": 0},
            {"id": "B0", "team": "B", "x": 14, "y": 3, "cargo": 0},
            {"id": "B1", "team": "B", "x": 14, "y": 7, "cargo": 0},
            {"id": "B2", "team": "B", "x": 14, "y": 11, "cargo": 0},
        ]
    return {
        "turn": turn,
        "team": team,
        "state": {
            "width": 15,
            "height": 15,
            "capacity": 4,
            "max_turns": 200,
            "total_resources": 96,
            "completed_turn": turn - 1,
            "resources": [] if resources is None else resources,
            "obstacles": [] if obstacles is None else obstacles,
            "robots": robots,
            "scores": {"A": 0, "B": 0},
            "outcome": None,
        },
    }


class FlushedOutput(io.StringIO):
    def __init__(self):
        super().__init__()
        self.flushes = 0

    def flush(self):
        self.flushes += 1
        super().flush()


def read_response(process):
    readable, _, _ = select.select([process.stdout], [], [], 3)
    if not readable:
        raise AssertionError("bot did not flush a response within three seconds")
    return json.loads(process.stdout.readline())


class ProtocolTests(unittest.TestCase):
    def test_loop_keeps_strategy_state_and_flushes_every_line(self):
        seen = []

        def strategy(observation):
            seen.append(observation.turn)
            return [wait(f"A{len(seen)}")]

        source = io.StringIO("\n".join(json.dumps(snapshot(turn=turn)) for turn in (1, 2)) + "\n")
        output = FlushedOutput()
        errors = io.StringIO()
        self.assertEqual(run_bot(strategy, source, output, errors), 0)
        responses = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual([response["turn"] for response in responses], [1, 2])
        self.assertEqual([response["actions"][0]["robot_id"] for response in responses], ["A1", "A2"])
        self.assertEqual(output.flushes, 2)
        self.assertEqual(errors.getvalue(), "")

    def test_invalid_observation_has_only_stderr_diagnostic(self):
        output, errors = io.StringIO(), io.StringIO()
        self.assertEqual(run_bot(lambda _: [], io.StringIO("not json\n"), output, errors), 1)
        self.assertEqual(output.getvalue(), "")
        self.assertIn("bot input or strategy error", errors.getvalue())

    def test_action_array_keeps_duplicates_for_engine_to_validate(self):
        output = io.StringIO()
        commands = [wait("A0"), move("A0", "RIGHT")]
        self.assertEqual(run_bot(lambda _: commands, io.StringIO(json.dumps(snapshot()) + "\n"), output), 0)
        self.assertEqual(json.loads(output.getvalue())["actions"], commands)

    def test_real_process_keeps_memory_then_fresh_process_resets_it(self):
        script = (
            f"import sys; sys.path.insert(0, {str(SDK_ROOT)!r})\n"
            "from arena_sdk import run_bot, wait\n"
            "class Counter:\n"
            "    def __init__(self): self.count = 0\n"
            "    def __call__(self, observation):\n"
            "        self.count += 1\n"
            "        return [wait('A' + str(self.count))]\n"
            "raise SystemExit(run_bot(Counter()))\n"
        )
        for expected in (["A1", "A2"], ["A1"]):
            with subprocess.Popen(
                [sys.executable, "-c", script], stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            ) as process:
                try:
                    for turn, robot_id in enumerate(expected, 1):
                        process.stdin.write(json.dumps(snapshot(turn=turn)) + "\n")
                        process.stdin.flush()
                        self.assertEqual(read_response(process)["actions"][0]["robot_id"], robot_id)
                    process.stdin.close()
                    self.assertEqual(process.wait(timeout=3), 0)
                    self.assertEqual(process.stderr.read(), "")
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait(timeout=3)

    def test_examples_run_without_current_directory_import_dependency(self):
        resources = [{"x": 3, "y": y, "amount": 4} for y in (3, 7, 11)]
        with tempfile.TemporaryDirectory() as directory:
            for script in ("basic_bot.py", "strategic_bot.py"):
                with self.subTest(script=script), subprocess.Popen(
                    [sys.executable, str(SDK_ROOT / "examples" / script)],
                    cwd=directory, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE, text=True,
                ) as process:
                    try:
                        for turn in (1, 2):
                            process.stdin.write(json.dumps(snapshot(turn=turn, resources=resources)) + "\n")
                            process.stdin.flush()
                            response = read_response(process)
                            self.assertEqual(response["turn"], turn)
                            self.assertEqual({action["robot_id"] for action in response["actions"]}, {"A0", "A1", "A2"})
                        process.stdin.close()
                        self.assertEqual(process.wait(timeout=3), 0)
                        self.assertEqual(process.stderr.read(), "")
                    finally:
                        if process.poll() is None:
                            process.kill()
                            process.wait(timeout=3)


class PathTests(unittest.TestCase):
    def test_path_routes_around_obstacles_and_other_start_positions(self):
        data = snapshot(obstacles=[{"x": 1, "y": 3}])
        data["state"]["robots"][1].update(x=1, y=2)
        observation = Observation.from_json(data)
        route = shortest_path(observation, observation.robots[0], [(3, 3)])
        self.assertEqual(route[-1], (3, 3))
        self.assertNotIn((1, 3), route)
        self.assertNotIn((1, 2), route)
        self.assertEqual(route[0], (0, 4))

    def test_enemy_depot_is_unreachable_and_own_depot_is_reachable(self):
        for team in ("A", "B"):
            observation = Observation.from_json(snapshot(team=team))
            robot = observation.robots[0]
            enemy_x = 14 if team == "A" else 0
            self.assertIsNone(shortest_path(observation, robot, [(enemy_x, 0)]))
            self.assertEqual(shortest_path(observation, robot, [(observation.home_x, 3)]), [])

    def test_reserved_destination_changes_next_step(self):
        observation = Observation.from_json(snapshot())
        robot = observation.robots[0]
        route = shortest_path(observation, robot, [(3, 3)], [(1, 3)])
        self.assertNotEqual(route[0], (1, 3))
        self.assertEqual(route[-1], (3, 3))


if __name__ == "__main__":
    unittest.main()
