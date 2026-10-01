"""Exercise the actual Rust HTTP server and bundled Python bots, without packages.

Build first: cargo build -p server
Run: python3 scripts/smoke_test.py
"""

from contextlib import contextmanager
import json
import os
from pathlib import Path
import socket
import subprocess
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]


def check_state(state):
    assert 0 <= state["completed_turn"] <= 200
    assert len(state["robots"]) == 6
    assert len({(robot["x"], robot["y"]) for robot in state["robots"]}) == 6
    assert all(0 <= robot["cargo"] <= 4 for robot in state["robots"])
    total = sum(cell["amount"] for cell in state["resources"])
    total += sum(robot["cargo"] for robot in state["robots"])
    total += sum(state["scores"].values())
    assert total == 96, state


def request(base, path, payload=None, expected=200, method=None):
    headers = {"Content-Type": "application/json"}
    data = None if payload is None else json.dumps(payload).encode()
    req = Request(base + path, data=data, headers=headers, method=method)
    try:
        response = urlopen(req, timeout=5)
    except HTTPError as error:
        response = error
    with response:
        assert response.status == expected, (path, response.status, response.read())
        assert response.headers["Cache-Control"] == "no-store"
        return json.load(response)


@contextmanager
def running_server(extra_env=None):
    binary = ROOT / "target" / "debug" / ("server.exe" if os.name == "nt" else "server")
    if not binary.exists():
        raise SystemExit("서버를 먼저 빌드하세요: cargo build -p server")
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    environment = dict(os.environ, BOT_ARENA_PORT=str(port), PYTHONDONTWRITEBYTECODE="1")
    environment.update(extra_env or {})
    process = subprocess.Popen(
        [str(binary)], cwd=ROOT.parent, env=environment,
        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
    )
    base = f"http://127.0.0.1:{port}"
    try:
        deadline = time.monotonic() + 10
        while True:
            if process.poll() is not None:
                raise AssertionError(process.stderr.read().decode())
            try:
                request(base, "/api/match")
                break
            except (URLError, ConnectionError):
                assert time.monotonic() < deadline, "server did not become ready"
                time.sleep(0.02)
        yield base
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
            raise AssertionError("server did not shut down cleanly")
        process.stderr.close()


def wait_terminal(base, match_id):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        snapshot = request(base, "/api/match")
        assert snapshot["match_id"] == match_id, "a previous match overwrote the new one"
        check_state(snapshot["state"])
        if snapshot["status"] in ("finished", "failed"):
            return snapshot
        time.sleep(0.005)
    raise AssertionError("match did not terminate")


def main():
    with running_server() as base:
        initial = request(base, "/api/match")
        assert initial["status"] == "idle" and initial["match_id"] is None
        assert initial["state"]["completed_turn"] == 0
        assert len(initial["state"]["resources"]) == 24
        check_state(initial["state"])
        with urlopen(base + "/", timeout=5) as response:
            assert b"<html" in response.read().lower()
        for path in ("/pages/arena-page.js", "/styles/tokens.css"):
            with urlopen(base + path, timeout=5) as response:
                assert response.status == 200
        stepped = request(base, "/api/match/step", method="POST")
        assert stepped["paused"] and stepped["state"]["completed_turn"] == 1
        time.sleep(0.2)
        assert request(base, "/api/match")["state"] == stepped["state"]
        stepped = request(base, "/api/match/step", method="POST")
        assert stepped["paused"] and stepped["state"]["completed_turn"] == 2
        request(base, "/api/match/speed", {"turn_delay_ms": 1000})
        resumed_at = time.monotonic()
        resumed = request(base, "/api/match/resume", method="POST")
        assert not resumed["paused"]
        request(base, "/api/match/step", expected=409, method="POST")
        time.sleep(0.4)
        assert request(base, "/api/match")["state"]["completed_turn"] == 2
        deadline = time.monotonic() + 3
        while request(base, "/api/match")["state"]["completed_turn"] == 2:
            assert time.monotonic() < deadline
            time.sleep(0.005)
        assert time.monotonic() - resumed_at >= 0.9, "1 second playback advanced too early"
        paused = request(base, "/api/match/pause", method="POST")
        assert paused["paused"]
        time.sleep(0.15)
        assert request(base, "/api/match")["state"] == paused["state"]
        reset = request(base, "/api/match/restart", {"turn_delay_ms": 1000, "paused": True})
        assert reset["paused"] and reset["state"] == initial["state"]
        time.sleep(0.15)
        assert request(base, "/api/match")["state"]["completed_turn"] == 0
        print("HTTP 수동 첫 턴·반복 한 턴·일시정지·1초/턴·수동 재시작 확인 완료")
        first = request(base, "/api/match/restart", {"turn_delay_ms": 1000})
        request(base, "/api/match/start", {"turn_delay_ms": 0}, expected=409)
        request(base, "/api/match/restart", {"turn_delay_ms": 2001}, expected=400)
        second = request(base, "/api/match/restart", {"turn_delay_ms": 1000})
        assert first["match_id"] != second["match_id"]
        assert second["state"] == initial["state"], "restart must return a fresh state"
        deadline = time.monotonic() + 3
        while request(base, "/api/match")["state"]["completed_turn"] == 0:
            assert time.monotonic() < deadline
            time.sleep(0.005)
        one = request(base, "/api/match")
        two = request(base, "/api/match")
        assert one["state"]["completed_turn"] == two["state"]["completed_turn"]
        request(base, "/api/match/speed", {"turn_delay_ms": 0})
        final = wait_terminal(base, second["match_id"])
        assert final["status"] == "finished", final["error"]
        state = final["state"]
        assert state["scores"]["A"] > 0 and state["scores"]["B"] > 0
        assert state["outcome"] is not None
        assert state["completed_turn"] == 200 or sum(state["scores"].values()) == 96
        request(base, "/api/match/step", expected=409, method="POST")
        print(f"HTTP 경기 완료: {state['completed_turn']}턴, 점수 {state['scores']}, {state['outcome']}")
        # Server shutdown must cancel a live match, not just an already finished task.
        request(base, "/api/match/restart", {"turn_delay_ms": 1000})

    with running_server({"BOT_ARENA_PYTHON": str(ROOT / "missing-python")}) as base:
        initial = request(base, "/api/match")
        assert initial["status"] == "idle", "server restart must discard match state"
        started = request(base, "/api/match/start", {"turn_delay_ms": 0})
        failed = wait_terminal(base, started["match_id"])
        assert failed["status"] == "failed" and failed["error"]
        assert failed["state"]["outcome"] is None
        print("HTTP 오류 상태·재시작·메모리 초기화·종료 확인 완료")


if __name__ == "__main__":
    main()
