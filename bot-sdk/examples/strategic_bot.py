"""Improved bot: distinct targets, collision avoidance, and timely returns.

Each robot remembers its target and previous move. If an attempted move failed,
the next plan temporarily avoids that destination to demonstrate rerouting.
"""

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from arena_sdk import Observation, pick, move_to, run_bot, shortest_path, wait


class StrategicBot:
    def __init__(self) -> None:
        self.targets: dict[str, tuple[int, int]] = {}
        self.previous_moves: dict[str, tuple[tuple[int, int], tuple[int, int]]] = {}
        self.turns_seen = 0

    def __call__(self, observation: Observation) -> list[dict[str, str]]:
        self.turns_seen += 1
        resources = observation.resources
        reserved_targets = set()
        reserved_moves = set()
        next_moves = {}
        actions = []
        home = [(observation.home_x, y) for y in range(observation.state["height"])]
        remaining_turns = observation.state["max_turns"] - observation.turn + 1

        # Loaded robots plan first, so their return routes get first reservation.
        robots = sorted(observation.robots, key=lambda item: (-item["cargo"], item["id"]))
        for robot in robots:
            robot_id = robot["id"]
            position = robot["x"], robot["y"]
            blocked = set(reserved_moves)
            previous = self.previous_moves.get(robot_id)
            if previous is not None and previous[0] == position:
                blocked.add(previous[1])

            home_route = shortest_path(observation, robot, home, blocked)
            home_distance = len(home_route) if home_route is not None else observation.state["width"]
            returning = robot["cargo"] > 0 and (
                robot["cargo"] >= observation.state["capacity"]
                or not resources
                or remaining_turns <= home_distance + 2
            )
            if returning:
                self.targets.pop(robot_id, None)
                route = home_route
            elif position in resources and robot["cargo"] < observation.state["capacity"]:
                actions.append(pick(robot_id))
                reserved_targets.add(position)
                self.targets.pop(robot_id, None)
                continue
            else:
                target = self.targets.get(robot_id)
                route = None
                if target in resources and target not in reserved_targets:
                    route = shortest_path(observation, robot, [target], blocked)
                if route is None:
                    candidates = []
                    for cell in resources:
                        if cell in reserved_targets:
                            continue
                        candidate = shortest_path(observation, robot, [cell], blocked)
                        if not candidate:
                            continue
                        # Prefer a central cell on close choices: both teams can
                        # contest it, while near-home resources can be saved.
                        distance_home = abs(cell[0] - observation.home_x)
                        if len(candidate) + 1 + distance_home > remaining_turns:
                            continue
                        central_bonus = 2 if cell[0] == observation.state["width"] // 2 else 0
                        candidates.append((len(candidate) - central_bonus, len(candidate), cell, candidate))
                    if candidates:
                        _, _, target, route = min(candidates)
                        self.targets[robot_id] = target
                    else:
                        self.targets.pop(robot_id, None)
                        if robot["cargo"] > 0:
                            route = home_route
                if route:
                    reserved_targets.add(route[-1])

            if route:
                destination = route[0]
                reserved_moves.add(destination)
                next_moves[robot_id] = position, destination
                actions.append(move_to(robot, destination))
            else:
                actions.append(wait(robot_id))
        self.previous_moves = next_moves
        return actions


if __name__ == "__main__":
    raise SystemExit(run_bot(StrategicBot()))
