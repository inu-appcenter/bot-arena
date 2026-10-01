"""First bot: find nearby recycling, collect it, and carry it home.

Run this file directly. Imports resolve relative to the script, so the server
may launch it from any working directory.
"""

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from arena_sdk import Observation, pick, move_to, run_bot, shortest_path, wait


class BasicBot:
    def __init__(self) -> None:
        # Targets live for the process lifetime and disappear with each match.
        self.targets: dict[str, tuple[int, int]] = {}
        self.turns_seen = 0

    def __call__(self, observation: Observation) -> list[dict[str, str]]:
        self.turns_seen += 1
        actions = []
        resources = observation.resources
        reserved_moves = set()
        home = [(observation.home_x, y) for y in range(observation.state["height"])]
        for robot in observation.robots:
            robot_id = robot["id"]
            position = robot["x"], robot["y"]
            if robot["cargo"] >= observation.state["capacity"] or (
                robot["cargo"] > 0 and not resources
            ):
                self.targets.pop(robot_id, None)
                route = shortest_path(observation, robot, home, reserved_moves)
            elif position in resources:
                actions.append(pick(robot_id))
                self.targets.pop(robot_id, None)
                continue
            else:
                target = self.targets.get(robot_id)
                route = None
                if target in resources:
                    route = shortest_path(observation, robot, [target], reserved_moves)
                if route is None:
                    route = shortest_path(observation, robot, resources, reserved_moves)
                    if route:
                        self.targets[robot_id] = route[-1]
            if route:
                reserved_moves.add(route[0])
                actions.append(move_to(robot, route[0]))
            else:
                actions.append(wait(robot_id))
        return actions


if __name__ == "__main__":
    raise SystemExit(run_bot(BasicBot()))
