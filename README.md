# 중고등학생 대상 온라인 전략 봇 대전 경진대회

주최 측 Python 봇 두 개가 재활용품을 수거하는 **로컬 MVP**다. Rust 엔진이 실제 봇 응답으로 경기를 판정하고, 브라우저에서 진행 상황과 결과를 관전한다.

고정 15×15 지도, 팀당 로봇 3대, 적재 한도 4개, 재활용품 총 96개, 최대 200턴을 사용한다. 경기와 실행 로그는 메모리에만 유지한다. DB·회원가입·제출·순위표·리플레이 저장·참가자 코드 격리는 MVP에 포함하지 않는다.

## 실행

필수 환경은 Rust stable/Cargo와 Python 3.10 이상이다. Python 외부 패키지, Node.js, 프런트엔드 빌드는 실행에 필요하지 않다. Rust 1.98.1과 Python 3.14.4에서 검증했다.

저장소 루트에서 실행한다.

```sh
cargo run -p server --locked
```

[http://127.0.0.1:3000](http://127.0.0.1:3000)을 열고 **경기 시작**을 누른다. 서버가 두 Python 프로세스를 실행하며, 관전 속도를 바꾸거나 진행 중 **새 경기로 재시작**할 수 있다. 종료 시 최종 점수와 승리 팀 또는 무승부가 표시된다. 서버는 `Ctrl+C`로 종료한다.

관전 속도에서 **1초/턴**을 선택하면 약 1초 간격으로 자동 진행한다. **일시정지**를 누르면 진행 중인 턴의 판정을 마친 뒤 멈추며, **한 턴 진행**을 누를 때마다 양 팀의 행동을 한 번씩 처리하고 다시 멈춘다. 초기 대기 화면에서도 **한 턴 진행**으로 첫 턴부터 수동 관전할 수 있다. **자동 진행**으로 돌아가면 선택한 간격 후 다음 턴부터 이어서 진행한다. 일시정지 상태에서 재시작하면 0턴으로 초기화한 뒤 멈춰 있다.

Cargo가 PATH에 없지만 rustup으로 설치되어 있다면 `~/.cargo/bin/cargo run -p server --locked`를 사용할 수 있다. Python 실행 파일 이름이 다르면 다음처럼 지정한다.

```sh
BOT_ARENA_PYTHON=/absolute/path/to/python3 cargo run -p server --locked
```

첫 빌드에는 crates.io 연결이 필요하다. 웹 파일과 봇 경로는 빌드 시 저장소 위치를 기준으로 해석하므로 실행 시 작업 디렉터리가 달라도 동작한다. 이 MVP의 바이너리는 빌드한 저장소와 함께 사용한다.

## 구성

| 경로 | 역할 |
| --- | --- |
| `crates/game-core` | 동기식 상태 전이, 동시 이동·충돌, 수거·반납·점수·승패 |
| `crates/runner` | 지속적인 Python 프로세스, JSON 통신, 제한 시간·출력 크기, 종료·회수 |
| `crates/server` | axum HTTP API, 활성 경기 하나의 메모리 상태, 정적 웹 파일 |
| `bot-sdk/arena_sdk.py` | 표준 라이브러리 SDK와 관측 기반 BFS 경로 탐색 |
| `bot-sdk/examples/basic_bot.py` | 가까운 자원을 수거하고 자기 구역으로 귀환 |
| `bot-sdk/examples/strategic_bot.py` | 목표 분담, 이동 실패 후 우회, 종료 직전 귀환 |
| `web` | ES Modules, Custom Elements·open Shadow DOM, Canvas 2D 관전 |

양 팀은 같은 턴 시작 스냅샷을 받는다. 이동은 시작 시 빈칸에만 가능하며, 이번 턴에 비울 칸 진입이나 자리 교환은 실패한다. 같은 빈칸으로 향하는 유효 이동이 둘 이상이면 모두 실패한다. 자기 반납 구역에서는 행동 처리 후 전량 자동 반납한다.

화면에는 판정이 끝난 상태만 전달한다. 관전 속도는 턴 사이 대기 간격이며 판정 결과에 영향을 주지 않는다. 지도 위 자원이 소진되어도 적재물이 남으면 운반이 계속된다. 96개 전량 반납 또는 200번째 턴의 반납 처리 후 종료한다. 미반납 적재물은 점수가 아니다.

## 고정 지도

원점은 왼쪽 위 `(0,0)`, 오른쪽이 `x`, 아래쪽이 `y`다. 장애물이 없으며 아래 24개 칸에 4개씩 배치한다.

| 구역 | 재활용품 좌표 |
| --- | --- |
| A측 · 36개 | `(3,2)`, `(3,7)`, `(3,12)`, `(4,4)`, `(4,10)`, `(5,3)`, `(5,7)`, `(5,11)`, `(6,6)` |
| 중앙 · 24개 | `(7,2)`, `(7,4)`, `(7,6)`, `(7,8)`, `(7,10)`, `(7,12)` |
| B측 · 36개 | `(11,2)`, `(11,7)`, `(11,12)`, `(10,4)`, `(10,10)`, `(9,3)`, `(9,7)`, `(9,11)`, `(8,6)` |

A팀 반납 구역은 `x=0` 전체, B팀은 `x=14` 전체다. 시작 로봇은 A0 `(0,3)`, A1 `(0,7)`, A2 `(0,11)`, B0 `(14,3)`, B1 `(14,7)`, B2 `(14,11)`이며 점수·적재량은 0이다. 각 팀은 자기 반납 구역만 진입할 수 있다.

## HTTP API

| 메서드·경로 | 동작 |
| --- | --- |
| `GET /api/match` | 현재 스냅샷 조회. 조회로 턴을 진행하지 않는다. |
| `POST /api/match/start` | 새 경기 시작. 진행 중이면 `409`. |
| `POST /api/match/restart` | 이전 프로세스 종료·회수 후 새 경기 시작. |
| `POST /api/match/speed` | 턴 간 표시 간격 변경. 이미 시작된 대기에는 다음 턴부터 반영될 수 있다. |
| `POST /api/match/pause` | 현재 턴의 판정이 끝난 경계에서 일시정지하고 상태 반환. |
| `POST /api/match/resume` | 선택한 표시 간격으로 자동 진행 재개. |
| `POST /api/match/step` | 일시정지 중 정확히 한 턴 판정 후 상태 반환. 대기 상태에서는 새 수동 경기를 시작하고 첫 턴 판정. |

시작·재시작·속도 변경 본문은 `{"turn_delay_ms":150}`이다. 시작·재시작에는 `"paused":true`를 추가해 0턴에서 대기할 수 있다. 허용 간격은 0~2000ms이며 웹의 속도 선택은 1000/450/150/40ms다. 일시정지·재개·한 턴 진행에는 본문이 없다. 자동 진행 중 한 턴 요청이나 종료된 경기의 한 턴 요청은 `409`, 잘못된 JSON·값은 `400`과 `{"error":"설명"}`을 반환한다. 임의 코드나 실행 경로를 입력받는 API는 없다.

```sh
curl http://127.0.0.1:3000/api/match
curl -X POST http://127.0.0.1:3000/api/match/start \
  -H 'Content-Type: application/json' -d '{"turn_delay_ms":150}'
```

스냅샷은 `match_id`, `status`, `state`, `error`, `turn_delay_ms`, `paused`, `bot_names`를 포함한다. `status`는 `idle`·`running`·`finished`·`failed`이며 초기 상태에는 경기 ID가 없다. 일시정지는 `status:running`과 `paused:true`로 표시한다. `state.completed_turn`은 완료된 판정 수로 0~200이다. 점수는 `state.scores.A/B`, 결과는 `state.outcome`에서 읽는다. 정상 종료 결과의 `winner:null`은 무승부이며 `reason`은 `all_delivered` 또는 `turn_limit`이다.

## 봇 JSON 계약

봇당 하나의 Python 프로세스가 경기 내내 유지된다. 서버가 표준입력에 관측 JSON 한 줄을 쓰면 봇은 표준출력에 행동 JSON 한 줄을 즉시 flush한다. 진단 로그는 표준오류로 출력한다. SDK의 `run_bot(전략객체)`는 이 반복 입출력을 처리한다.

다음은 A팀 첫 턴에 실제로 제공하는 관측의 형식이다. 실제 전송은 줄바꿈 없는 한 줄이다.

```json
{
  "turn": 1,
  "team": "A",
  "state": {
    "width": 15, "height": 15, "capacity": 4,
    "max_turns": 200, "total_resources": 96, "completed_turn": 0,
    "resources": [
      {"x":3,"y":2,"amount":4}, {"x":11,"y":2,"amount":4},
      {"x":3,"y":7,"amount":4}, {"x":11,"y":7,"amount":4},
      {"x":3,"y":12,"amount":4}, {"x":11,"y":12,"amount":4},
      {"x":4,"y":4,"amount":4}, {"x":10,"y":4,"amount":4},
      {"x":4,"y":10,"amount":4}, {"x":10,"y":10,"amount":4},
      {"x":5,"y":3,"amount":4}, {"x":9,"y":3,"amount":4},
      {"x":5,"y":7,"amount":4}, {"x":9,"y":7,"amount":4},
      {"x":5,"y":11,"amount":4}, {"x":9,"y":11,"amount":4},
      {"x":6,"y":6,"amount":4}, {"x":8,"y":6,"amount":4},
      {"x":7,"y":2,"amount":4}, {"x":7,"y":4,"amount":4},
      {"x":7,"y":6,"amount":4}, {"x":7,"y":8,"amount":4},
      {"x":7,"y":10,"amount":4}, {"x":7,"y":12,"amount":4}
    ],
    "obstacles": [],
    "robots": [
      {"id":"A0","team":"A","x":0,"y":3,"cargo":0},
      {"id":"A1","team":"A","x":0,"y":7,"cargo":0},
      {"id":"A2","team":"A","x":0,"y":11,"cargo":0},
      {"id":"B0","team":"B","x":14,"y":3,"cargo":0},
      {"id":"B1","team":"B","x":14,"y":7,"cargo":0},
      {"id":"B2","team":"B","x":14,"y":11,"cargo":0}
    ],
    "scores": {"A":0,"B":0}, "outcome": null
  }
}
```

응답은 요청받은 `turn`을 그대로 반환하고 로봇별 명령 배열을 사용한다.

```json
{"turn":1,"actions":[{"robot_id":"A0","action":"MOVE","direction":"RIGHT"},{"robot_id":"A1","action":"MOVE","direction":"RIGHT"},{"robot_id":"A2","action":"WAIT"}]}
```

명령은 `MOVE`와 `UP|DOWN|LEFT|RIGHT`, `PICK`, `WAIT`다. 이동과 수거를 같은 턴에 수행할 수 없다. 누락·불가능한 명령은 `WAIT`, 같은 로봇의 중복 명령은 해당 로봇의 `WAIT`, 소유하지 않은 로봇 명령은 무시한다. `PICK`·`WAIT`에는 `direction`을 생략하거나 `null`로 둔다. 배열 원소는 객체이고 `robot_id`는 문자열이어야 하며, action 또는 direction의 잘못된 값은 해당 명령의 `WAIT`로 처리한다.

## 실행 제한과 오류 정책

| 환경 변수 | 기본값 | 의미 |
| --- | --- | --- |
| `BOT_ARENA_PORT` | `3000` | 루프백 주소 `127.0.0.1`의 포트 |
| `BOT_ARENA_PYTHON` | `python3` | 실행할 Python 경로 또는 명령 |
| `BOT_ARENA_TIMEOUT_MS` | `1000` | 팀당 매 턴 관측 쓰기·응답 읽기 제한 |
| `BOT_ARENA_OUTPUT_BYTES` | `65536` | 응답 한 줄 최대 바이트, 마지막 개행 포함 |
| `BOT_ARENA_STDERR_BYTES` | `8192` | 봇당 메모리에 유지하는 stderr 꼬리 크기 |

수치 환경 변수는 양의 정수여야 한다. 양 팀의 통신은 병행하며 각각 같은 제한을 적용한다. stderr는 계속 소비하여 출력 때문에 봇이 막히지 않도록 한다. 실패 화면에는 제한된 최근 로그를 표시할 수 있다.

JSON·응답 구조 오류, 턴 불일치, 응답 시간 초과, 출력 한도 초과, 예상하지 않은 봇 종료는 `failed`로 처리한다. 양쪽 프로세스를 회수하며 승리·무승부·몰수승을 부여하지 않는다. 이는 **로컬 MVP용 기본 정책**이고 대회 운영 규정은 미정이다.

새 경기·재시작·서버 종료에는 이전 프로세스와 전략 상태를 폐기한다. 실행 대상은 저장소의 주최 측 예제 두 개로 고정했으며 공개 제출을 격리하는 실행 환경은 제공하지 않는다.

## 검증

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s bot-sdk/tests -v
cargo build -p server --locked
python3 scripts/smoke_test.py
```

Rust 테스트는 핵심 규칙과 실제 Python 프로세스의 상태 유지, 같은 스냅샷의 병행 통신, 실행 오류·취소·프로세스 회수를 검증한다. HTTP smoke test는 별도 임시 로컬 포트에서 서버를 실행하여 시작·중복 시작·재시작·속도·최종 결과·오류 표시용 상태·서버 재실행 시 초기화·종료를 확인한다. 테스트 시에도 Python 명령은 `python3`가 필요하다.

브라우저에서는 초기 화면 → 시작 → 점수·적재량 갱신 → 실행 중 재시작 → 종료 결과를 확인한다. 390px 모바일 화면, 서버 중단 후 연결 오류와 재연결, 봇 실행 오류도 확인한다. 제공 봇의 시범 경기는 52턴에 A 48점 / B 48점으로 96개를 전량 반납했다. 대회용 전략 평가·시작 진영 교환·운영 규정 확정은 후속 범위다.
