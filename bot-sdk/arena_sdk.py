"""Small standard-library SDK for one persistent arena bot process.

The server owns game rules. These helpers only read observations, write actions,
and plan routes from the visible snapshot; they never change the game state.
"""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass
import json
import sys
from typing import Callable, Iterable, TextIO

Position = tuple[int, int]
Action = dict[str, str]
DIRECTIONS: dict[str, Position] = {
    "UP": (0, -1),
    "DOWN": (0, 1),
    "LEFT": (-1, 0),
    "RIGHT": (1, 0),
}


@dataclass(frozen=True)
class Observation:
    """One team's view of the shared state before the requested action turn."""

    turn: int
    team: str
    state: dict

    @classmethod
    def from_json(cls, value: dict) -> Observation:
        if not isinstance(value, dict):
            raise ValueError("observation must be a JSON object")
        turn, team, state = value.get("turn"), value.get("team"), value.get("state")
        if type(turn) is not int or turn < 1:
            raise ValueError("observation turn must be a positive integer")
        if team not in ("A", "B") or not isinstance(state, dict):
            raise ValueError("observation requires team A or B and state object")
        return cls(turn=turn, team=team, state=state)

    @property
    def robots(self) -> list[dict]:
        return sorted(
            (robot for robot in self.state["robots"] if robot["team"] == self.team),
            key=lambda robot: robot["id"],
        )

    @property
    def resources(self) -> dict[Position, int]:
        return {
            (item["x"], item["y"]): item["amount"]
            for item in self.state["resources"]
            if item["amount"] > 0
        }

    @property
    def home_x(self) -> int:
        return 0 if self.team == "A" else self.state["width"] - 1


def wait(robot_id: str) -> Action:
    return {"robot_id": robot_id, "action": "WAIT"}


def pick(robot_id: str) -> Action:
    return {"robot_id": robot_id, "action": "PICK"}


def move(robot_id: str, direction: str) -> Action:
    if direction not in DIRECTIONS:
        raise ValueError(f"unknown movement direction: {direction}")
    return {"robot_id": robot_id, "action": "MOVE", "direction": direction}


def move_to(robot: dict, destination: Position) -> Action:
    delta = destination[0] - robot["x"], destination[1] - robot["y"]
    for direction, offset in DIRECTIONS.items():
        if offset == delta:
            return move(robot["id"], direction)
    raise ValueError("a move destination must be one adjacent cell")


def shortest_path(
    observation: Observation,
    robot: dict,
    goals: Iterable[Position],
    blocked: Iterable[Position] = (),
    direction_order: Iterable[str] = ("LEFT", "RIGHT", "UP", "DOWN"),
) -> list[Position] | None:
    """BFS route to the closest goal, excluding all other start positions.

    The returned route excludes the starting cell. An empty route means the
    robot is already at a goal, while None means no goal is currently reachable.
    Additional blocked cells can reserve a teammate's next destination or help
    a strategy temporarily avoid a move that failed on the preceding turn.
    """

    state = observation.state
    start = robot["x"], robot["y"]
    targets = set(goals)
    if start in targets:
        return []
    forbidden = set(blocked)
    forbidden.update((item["x"], item["y"]) for item in state["obstacles"])
    forbidden.update(
        (other["x"], other["y"])
        for other in state["robots"]
        if other["id"] != robot["id"]
    )
    enemy_home = state["width"] - 1 if observation.team == "A" else 0
    offsets = [DIRECTIONS[direction] for direction in direction_order]
    previous: dict[Position, Position | None] = {start: None}
    queue = deque([start])
    while queue:
        x, y = queue.popleft()
        for dx, dy in offsets:
            cell = x + dx, y + dy
            if (
                not 0 <= cell[0] < state["width"]
                or not 0 <= cell[1] < state["height"]
                or cell[0] == enemy_home
                or cell in forbidden
                or cell in previous
            ):
                continue
            previous[cell] = x, y
            if cell in targets:
                route = [cell]
                cursor = previous[cell]
                while cursor != start:
                    route.append(cursor)
                    cursor = previous[cursor]
                route.reverse()
                return route
            queue.append(cell)
    return None


def run_bot(
    decide: Callable[[Observation], list[Action]],
    input_stream: TextIO | None = None,
    output_stream: TextIO | None = None,
    error_stream: TextIO | None = None,
) -> int:
    """Call one strategy object repeatedly until stdin closes.

    Keep strategy state in that object rather than reconstructing it per line.
    One JSON line is flushed per observation; diagnostics go only to stderr.
    """

    input_stream = sys.stdin if input_stream is None else input_stream
    output_stream = sys.stdout if output_stream is None else output_stream
    error_stream = sys.stderr if error_stream is None else error_stream
    try:
        for line in input_stream:
            observation = Observation.from_json(json.loads(line))
            actions = decide(observation)
            if not isinstance(actions, list):
                raise ValueError("strategy must return an action array")
            response = {"turn": observation.turn, "actions": actions}
            output_stream.write(json.dumps(response, separators=(",", ":")) + "\n")
            output_stream.flush()
    except (ValueError, KeyError, TypeError) as error:
        print(f"bot input or strategy error: {error}", file=error_stream, flush=True)
        return 1
    return 0
