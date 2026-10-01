//! Local, trusted Python bot execution. Game rules live exclusively in game-core.
use game_core::{Command, GameState, Team};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::VecDeque,
    future::Future,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::{watch, Mutex},
    task::JoinHandle,
    time::{sleep, timeout},
};

#[derive(Clone, Debug)]
pub struct RunnerConfig {
    pub python: PathBuf,
    pub bot_paths: [PathBuf; 2],
    pub turn_timeout: Duration,
    pub max_output_bytes: usize,
    pub stderr_tail_bytes: usize,
}

impl RunnerConfig {
    pub fn local(root: &std::path::Path) -> Self {
        Self {
            python: PathBuf::from("python3"),
            bot_paths: [
                root.join("bot-sdk/examples/basic_bot.py"),
                root.join("bot-sdk/examples/strategic_bot.py"),
            ],
            turn_timeout: Duration::from_millis(1_000),
            max_output_bytes: 65_536,
            stderr_tail_bytes: 8_192,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunEnd {
    Finished,
    Cancelled,
}

#[derive(Serialize)]
struct Observation<'a> {
    turn: u16,
    team: Team,
    state: &'a GameState,
}

struct BotProcess {
    team: Team,
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    stderr: Arc<Mutex<VecDeque<u8>>>,
    stderr_task: JoinHandle<()>,
}

impl BotProcess {
    fn spawn(config: &RunnerConfig, team: Team, path: &std::path::Path) -> Result<Self, String> {
        let mut child = tokio::process::Command::new(&config.python)
            .arg("-u")
            .arg(path)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("{team:?}팀 봇 시작 실패: {error}"))?;
        let input = child.stdin.take().expect("piped stdin");
        let output = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut stderr_pipe = child.stderr.take().expect("piped stderr");
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let stderr_copy = stderr.clone();
        let limit = config.stderr_tail_bytes;
        let stderr_task = tokio::spawn(async move {
            let mut chunk = [0_u8; 4_096];
            loop {
                let count = match stderr_pipe.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(count) => count,
                };
                let mut tail = stderr_copy.lock().await;
                for byte in &chunk[..count] {
                    if limit == 0 {
                        continue;
                    }
                    if tail.len() == limit {
                        tail.pop_front();
                    }
                    tail.push_back(*byte);
                }
            }
        });
        Ok(Self {
            team,
            child,
            input,
            output,
            stderr,
            stderr_task,
        })
    }

    async fn exchange(
        &mut self,
        state: &GameState,
        config: &RunnerConfig,
    ) -> Result<Vec<Command>, String> {
        let team = self.team;
        let exchange = async {
            if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
                return Err(format!("프로세스가 종료되었습니다 ({status})"));
            }
            let turn = state.completed_turn + 1;
            let mut request = serde_json::to_vec(&Observation { turn, team, state })
                .map_err(|e| e.to_string())?;
            request.push(b'\n');
            self.input
                .write_all(&request)
                .await
                .map_err(|e| format!("관측 전송 실패: {e}"))?;
            self.input.flush().await.map_err(|e| e.to_string())?;
            let response = read_limited_line(&mut self.output, config.max_output_bytes).await?;
            parse_actions(&response, turn)
        };
        timeout(config.turn_timeout, exchange)
            .await
            .map_err(|_| {
                format!(
                    "{team:?}팀 봇: 응답 시간 초과 ({}ms)",
                    config.turn_timeout.as_millis()
                )
            })?
            .map_err(|error| format!("{team:?}팀 봇: {error}"))
    }

    async fn close(&mut self, require_alive: bool) -> Result<String, String> {
        let exited = self.child.try_wait().map_err(|e| e.to_string())?;
        if exited.is_none() {
            self.child.start_kill().map_err(|e| e.to_string())?;
        }
        self.child.wait().await.map_err(|e| e.to_string())?;
        // A descendant must not keep a stderr-draining task alive after this match.
        self.stderr_task.abort();
        let tail: Vec<u8> = self.stderr.lock().await.iter().copied().collect();
        if require_alive {
            if let Some(status) = exited {
                return Err(format!(
                    "프로세스가 경기 정리 전에 종료되었습니다 ({status})"
                ));
            }
        }
        Ok(String::from_utf8_lossy(&tail).into_owned())
    }
}

impl Drop for BotProcess {
    fn drop(&mut self) {
        self.stderr_task.abort();
        // Child's kill_on_drop also covers an unwinding runner task.
    }
}

async fn read_limited_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|e| e.to_string())?;
        if available.is_empty() {
            return Err("응답 전에 표준출력이 닫혔습니다".into());
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if result.len().saturating_add(count) > limit {
            return Err(format!("응답 출력 한도 초과 ({limit} bytes)"));
        }
        result.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            return Ok(result);
        }
    }
}

fn parse_actions(response: &[u8], expected_turn: u16) -> Result<Vec<Command>, String> {
    let value: Value =
        serde_json::from_slice(response).map_err(|e| format!("잘못된 JSON 응답: {e}"))?;
    if value.get("turn").and_then(Value::as_u64) != Some(u64::from(expected_turn)) {
        return Err(format!("응답 턴 불일치 (요청: {expected_turn})"));
    }
    let actions = value
        .get("actions")
        .and_then(Value::as_array)
        .ok_or("응답 actions는 배열이어야 합니다")?;
    // Action-level errors become WAIT, and records retain duplicates for engine validation.
    actions
        .iter()
        .map(|action| {
            if !action.is_object() {
                return Err("각 행동 레코드는 객체여야 합니다".to_owned());
            }
            let robot_id = action
                .get("robot_id")
                .and_then(Value::as_str)
                .ok_or("각 행동의 robot_id는 문자열이어야 합니다")?
                .to_owned();
            let invalid_direction = action
                .get("direction")
                .is_some_and(|direction| !direction.is_null() && !direction.is_string());
            let action_name = if invalid_direction {
                "WAIT"
            } else {
                action
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or("WAIT")
            };
            Ok(Command {
                robot_id,
                action: action_name.to_owned(),
                direction: if invalid_direction {
                    None
                } else {
                    action
                        .get("direction")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                },
            })
        })
        .collect()
}

/// Runs one fresh match, reporting only fully judged states. Cancellation always
/// cleans up both processes before returning; callbacks must not hold I/O locks.
pub async fn run_match<F, Fut>(
    config: RunnerConfig,
    mut cancel: watch::Receiver<bool>,
    delay_ms: Arc<AtomicU64>,
    mut publish: F,
) -> Result<RunEnd, String>
where
    F: FnMut(GameState) -> Fut,
    Fut: Future<Output = ()>,
{
    if *cancel.borrow() {
        return Ok(RunEnd::Cancelled);
    }
    let mut a = BotProcess::spawn(&config, Team::A, &config.bot_paths[0])?;
    let mut b = match BotProcess::spawn(&config, Team::B, &config.bot_paths[1]) {
        Ok(bot) => bot,
        Err(error) => {
            let _ = a.close(false).await;
            return Err(error);
        }
    };
    let run: Result<RunEnd, String> = async {
        let mut state = GameState::new();
        loop {
            if *cancel.borrow() {
                return Ok(RunEnd::Cancelled);
            }
            let actions = tokio::select! {
                biased;
                _ = cancel.changed() => return Ok(RunEnd::Cancelled),
                actions = async { tokio::try_join!(a.exchange(&state, &config), b.exchange(&state, &config)) } => actions?,
            };
            state.step(&actions.0, &actions.1)?;
            publish(state.clone()).await;
            if state.outcome.is_some() {
                return Ok(RunEnd::Finished);
            }
            tokio::select! {
                biased;
                _ = cancel.changed() => return Ok(RunEnd::Cancelled),
                _ = sleep(Duration::from_millis(delay_ms.load(Ordering::Relaxed))) => {}
            }
        }
    }
    .await;
    let require_alive = matches!(run, Ok(RunEnd::Finished));
    let (a_cleanup, b_cleanup) = tokio::join!(a.close(require_alive), b.close(require_alive));
    match run {
        Err(error) => {
            let mut details = error;
            for (team, tail) in [("A", a_cleanup), ("B", b_cleanup)] {
                match tail {
                    Ok(text) if !text.trim().is_empty() => {
                        details.push_str(&format!("\n{team}팀 로그: {}", text.trim()));
                    }
                    Err(error) => details.push_str(&format!("\n{team}팀 정리 실패: {error}")),
                    _ => {}
                }
            }
            Err(details)
        }
        Ok(end) => {
            a_cleanup.map_err(|e| format!("A팀 정리 실패: {e}"))?;
            b_cleanup.map_err(|e| format!("B팀 정리 실패: {e}"))?;
            Ok(end)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_errors_and_action_errors_are_distinct() {
        assert!(parse_actions(b"not json", 1).is_err());
        assert!(parse_actions(br#"{"turn":2,"actions":[]}"#, 1).is_err());
        assert!(parse_actions(br#"{"turn":1,"actions":{}}"#, 1).is_err());
        assert!(parse_actions(br#"{"turn":1,"actions":[false]}"#, 1).is_err());
        let commands = parse_actions(br#"{"turn":1,"actions":[{"robot_id":"A0","action":"MOVE","direction":42},{"robot_id":"A0","action":"PICK"},{"robot_id":"A1","action":"PICK","direction":42}]}"#, 1).unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].robot_id, commands[1].robot_id);
        assert_eq!(commands[0].direction, None);
        assert_eq!(commands[2].action, "WAIT");
    }

    #[tokio::test]
    async fn response_limit_applies_before_buffer_grows_without_bound() {
        let mut reader = BufReader::new(&b"123456789\n"[..]);
        assert!(read_limited_line(&mut reader, 5)
            .await
            .unwrap_err()
            .contains("한도"));
        let mut reader = BufReader::new(&b"{}\n"[..]);
        assert_eq!(read_limited_line(&mut reader, 3).await.unwrap(), b"{}\n");
        let mut reader = BufReader::new(&b"unterminated"[..]);
        assert!(read_limited_line(&mut reader, 100).await.is_err());
    }
}
