use axum::{
    extract::{rejection::JsonRejection, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use game_core::GameState;
use runner::{playback_channel, run_match_controlled, PlaybackHandle, RunEnd, RunnerConfig};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{
    sync::{watch, Mutex, RwLock},
    task::JoinHandle,
};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Idle,
    Running,
    Finished,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct BotNames {
    #[serde(rename = "A")]
    a: &'static str,
    #[serde(rename = "B")]
    b: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub match_id: Option<u64>,
    pub status: MatchStatus,
    pub state: GameState,
    pub error: Option<String>,
    pub turn_delay_ms: u64,
    pub paused: bool,
    pub bot_names: BotNames,
}

struct ActiveMatch {
    cancel: watch::Sender<bool>,
    playback: PlaybackHandle,
    done: watch::Receiver<bool>,
    task: JoinHandle<()>,
}

enum PlaybackAction {
    Pause,
    Resume,
    Step,
}

pub struct MatchManager {
    snapshot: Arc<RwLock<Snapshot>>,
    // Serializes launch, cancellation and speed requests, not state reads or bot I/O.
    active: Mutex<Option<ActiveMatch>>,
    next_id: AtomicU64,
    delay_ms: Arc<AtomicU64>,
    config: RunnerConfig,
}

impl MatchManager {
    pub fn new(config: RunnerConfig) -> Arc<Self> {
        Arc::new(Self {
            snapshot: Arc::new(RwLock::new(Snapshot {
                match_id: None,
                status: MatchStatus::Idle,
                state: GameState::new(),
                error: None,
                turn_delay_ms: 150,
                paused: false,
                bot_names: BotNames {
                    a: "기본 수거 봇",
                    b: "분담 전략 봇",
                },
            })),
            active: Mutex::new(None),
            next_id: AtomicU64::new(1),
            delay_ms: Arc::new(AtomicU64::new(150)),
            config,
        })
    }

    pub async fn snapshot(&self) -> Snapshot {
        self.snapshot.read().await.clone()
    }

    async fn launch(&self, restart: bool, delay: u64, paused: bool) -> Result<Snapshot, ApiError> {
        validate_delay(delay)?;
        let mut active = self.active.lock().await;
        self.launch_locked(&mut active, restart, delay, paused)
            .await
    }

    async fn launch_locked(
        &self,
        active: &mut Option<ActiveMatch>,
        restart: bool,
        delay: u64,
        paused: bool,
    ) -> Result<Snapshot, ApiError> {
        if !restart && self.snapshot.read().await.status == MatchStatus::Running {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "이미 경기가 진행 중입니다. 재시작을 사용하세요.",
            ));
        }
        if let Some(previous) = active.take() {
            let _ = previous.cancel.send(true);
            previous.task.await.map_err(|e| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("경기 정리 실패: {e}"),
                )
            })?;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.delay_ms.store(delay, Ordering::Relaxed);
        let initial = {
            let mut current = self.snapshot.write().await;
            current.match_id = Some(id);
            current.status = MatchStatus::Running;
            current.state = GameState::new();
            current.error = None;
            current.turn_delay_ms = delay;
            current.paused = paused;
            current.clone()
        };
        let config = self.config.clone();
        let shared = self.snapshot.clone();
        let delay = self.delay_ms.clone();
        let (cancel, receiver) = watch::channel(false);
        let (playback, playback_receiver) = playback_channel(paused);
        let (done_sender, done) = watch::channel(false);
        let task = tokio::spawn(async move {
            let publish_to = shared.clone();
            let result = run_match_controlled(
                config,
                receiver,
                delay,
                move |state| {
                    let shared = publish_to.clone();
                    async move {
                        let mut current = shared.write().await;
                        if current.match_id == Some(id) && current.status == MatchStatus::Running {
                            current.state = state;
                        }
                    }
                },
                playback_receiver,
            )
            .await;
            let mut current = shared.write().await;
            if current.match_id != Some(id) {
                let _ = done_sender.send(true);
                return;
            }
            match result {
                Ok(RunEnd::Finished) => current.status = MatchStatus::Finished,
                Ok(RunEnd::Cancelled) => {}
                Err(error) => {
                    current.status = MatchStatus::Failed;
                    current.state.outcome = None;
                    current.error = Some(error);
                }
            }
            if current.status != MatchStatus::Running {
                current.paused = false;
            }
            let _ = done_sender.send(true);
        });
        *active = Some(ActiveMatch {
            cancel,
            playback,
            done,
            task,
        });
        Ok(initial)
    }

    async fn playback(&self, action: PlaybackAction) -> Result<Snapshot, ApiError> {
        // Serialize requests so each acknowledged step advances exactly one turn.
        let mut active = self.active.lock().await;
        let current = self.snapshot().await;
        if matches!(action, PlaybackAction::Step) && current.status == MatchStatus::Idle {
            self.launch_locked(&mut active, false, current.turn_delay_ms, true)
                .await?;
        } else if current.status != MatchStatus::Running {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "진행 중인 경기가 없습니다. 새 경기를 시작하세요.",
            ));
        } else if matches!(action, PlaybackAction::Step) && !current.paused {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "먼저 일시정지한 뒤 한 턴씩 진행하세요.",
            ));
        }
        let running = active
            .as_mut()
            .ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "진행 중인 경기가 없습니다."))?;
        let result = match action {
            PlaybackAction::Pause => running.playback.pause().await,
            PlaybackAction::Resume => running.playback.resume().await,
            PlaybackAction::Step => running.playback.step().await,
        };
        // On the final step or an execution error, wait for cleanup and the
        // terminal snapshot rather than exposing a finished board as running.
        if result.is_err() || self.snapshot().await.state.outcome.is_some() {
            while !*running.done.borrow() {
                if running.done.changed().await.is_err() {
                    break;
                }
            }
        }
        let mut current = self.snapshot.write().await;
        if current.status == MatchStatus::Running {
            result.map_err(|error| ApiError::new(StatusCode::CONFLICT, &error))?;
            current.paused = !matches!(action, PlaybackAction::Resume);
        }
        Ok(current.clone())
    }

    async fn set_speed(&self, delay: u64) -> Result<Snapshot, ApiError> {
        validate_delay(delay)?;
        let _active = self.active.lock().await;
        self.delay_ms.store(delay, Ordering::Relaxed);
        let mut snapshot = self.snapshot.write().await;
        snapshot.turn_delay_ms = delay;
        Ok(snapshot.clone())
    }

    pub async fn shutdown(&self) {
        if let Some(active) = self.active.lock().await.take() {
            let _ = active.cancel.send(true);
            if let Err(error) = active.task.await {
                eprintln!("경기 종료 작업 실패: {error}");
            }
        }
    }
}

fn validate_delay(delay: u64) -> Result<(), ApiError> {
    if delay > 2_000 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "turn_delay_ms는 0~2000 범위여야 합니다.",
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchOptions {
    turn_delay_ms: u64,
    #[serde(default)]
    paused: bool,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: &str) -> Self {
        Self {
            status,
            message: message.to_owned(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

fn options(payload: Result<Json<MatchOptions>, JsonRejection>) -> Result<MatchOptions, ApiError> {
    payload.map(|Json(options)| options).map_err(|e| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            &format!("잘못된 요청: {}", e.body_text()),
        )
    })
}

async fn get_match(State(manager): State<Arc<MatchManager>>) -> Json<Snapshot> {
    Json(manager.snapshot().await)
}

async fn start_match(
    State(manager): State<Arc<MatchManager>>,
    payload: Result<Json<MatchOptions>, JsonRejection>,
) -> Result<Json<Snapshot>, ApiError> {
    let options = options(payload)?;
    Ok(Json(
        manager
            .launch(false, options.turn_delay_ms, options.paused)
            .await?,
    ))
}

async fn restart_match(
    State(manager): State<Arc<MatchManager>>,
    payload: Result<Json<MatchOptions>, JsonRejection>,
) -> Result<Json<Snapshot>, ApiError> {
    let options = options(payload)?;
    Ok(Json(
        manager
            .launch(true, options.turn_delay_ms, options.paused)
            .await?,
    ))
}

async fn speed(
    State(manager): State<Arc<MatchManager>>,
    payload: Result<Json<MatchOptions>, JsonRejection>,
) -> Result<Json<Snapshot>, ApiError> {
    Ok(Json(
        manager.set_speed(options(payload)?.turn_delay_ms).await?,
    ))
}

async fn pause(State(manager): State<Arc<MatchManager>>) -> Result<Json<Snapshot>, ApiError> {
    Ok(Json(manager.playback(PlaybackAction::Pause).await?))
}

async fn resume(State(manager): State<Arc<MatchManager>>) -> Result<Json<Snapshot>, ApiError> {
    Ok(Json(manager.playback(PlaybackAction::Resume).await?))
}

async fn step(State(manager): State<Arc<MatchManager>>) -> Result<Json<Snapshot>, ApiError> {
    Ok(Json(manager.playback(PlaybackAction::Step).await?))
}

pub fn app(root: &Path, manager: Arc<MatchManager>) -> Router {
    Router::new()
        .route("/api/match", get(get_match))
        .route("/api/match/start", post(start_match))
        .route("/api/match/restart", post(restart_match))
        .route("/api/match/speed", post(speed))
        .route("/api/match/pause", post(pause))
        .route("/api/match/resume", post(resume))
        .route("/api/match/step", post(step))
        .fallback_service(ServeDir::new(root.join("web")))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
    }

    #[tokio::test]
    async fn restart_replaces_processes_and_never_publishes_the_previous_match() {
        let manager = MatchManager::new(RunnerConfig::local(root()));
        let first = manager.launch(false, 1_000, false).await.unwrap();
        assert_eq!(first.state.completed_turn, 0);
        assert_eq!(
            manager.launch(false, 0, false).await.unwrap_err().status,
            StatusCode::CONFLICT
        );
        let second = manager.launch(true, 0, false).await.unwrap();
        assert_ne!(first.match_id, second.match_id);
        assert_eq!(second.state.completed_turn, 0);
        timeout(Duration::from_secs(15), async {
            loop {
                let current = manager.snapshot().await;
                assert_eq!(current.match_id, second.match_id);
                current.state.validate().unwrap();
                if current.status == MatchStatus::Finished {
                    assert!(current.state.outcome.is_some());
                    assert!(current.state.scores.a + current.state.scores.b > 0);
                    break;
                }
                assert_ne!(current.status, MatchStatus::Failed, "{:?}", current.error);
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        manager.shutdown().await;
    }

    #[tokio::test]
    async fn startup_error_is_visible_without_awarding_a_winner() {
        let mut config = RunnerConfig::local(root());
        config.python = root().join("nonexistent-python");
        let manager = MatchManager::new(config);
        manager.launch(false, 0, false).await.unwrap();
        timeout(Duration::from_secs(2), async {
            loop {
                let current = manager.snapshot().await;
                if current.status == MatchStatus::Failed {
                    assert!(current.error.unwrap().contains("시작 실패"));
                    assert!(current.state.outcome.is_none());
                    assert_eq!(current.state.completed_turn, 0);
                    break;
                }
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        manager.shutdown().await;
    }

    #[tokio::test]
    async fn manual_steps_finish_with_the_same_result_as_automatic_play() {
        let manual = MatchManager::new(RunnerConfig::local(root()));
        let mut current = manual.playback(PlaybackAction::Step).await.unwrap();
        let id = current.match_id;
        assert_eq!(current.state.completed_turn, 1);
        assert!(current.paused);
        sleep(Duration::from_millis(180)).await;
        assert_eq!(manual.snapshot().await.state, current.state);
        while current.status == MatchStatus::Running {
            let before = current.state.completed_turn;
            current = manual.playback(PlaybackAction::Step).await.unwrap();
            assert_eq!(current.state.completed_turn, before + 1);
            assert_eq!(current.match_id, id);
            current.state.validate().unwrap();
        }
        assert_eq!(current.status, MatchStatus::Finished);
        assert!(!current.paused);
        assert_eq!(
            manual
                .playback(PlaybackAction::Step)
                .await
                .unwrap_err()
                .status,
            StatusCode::CONFLICT
        );
        let automatic = MatchManager::new(RunnerConfig::local(root()));
        automatic.launch(false, 0, false).await.unwrap();
        timeout(Duration::from_secs(5), async {
            while automatic.snapshot().await.status == MatchStatus::Running {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(automatic.snapshot().await.state, current.state);
        manual.shutdown().await;
        automatic.shutdown().await;
    }

    #[tokio::test]
    async fn paused_restart_and_one_second_resume_preserve_turn_boundaries() {
        let manager = MatchManager::new(RunnerConfig::local(root()));
        let first = manager.playback(PlaybackAction::Step).await.unwrap();
        let reset = manager.launch(true, 1_000, true).await.unwrap();
        assert_ne!(reset.match_id, first.match_id);
        assert!(reset.paused);
        assert_eq!(reset.state.completed_turn, 0);
        sleep(Duration::from_millis(150)).await;
        assert_eq!(manager.snapshot().await.state.completed_turn, 0);
        manager.playback(PlaybackAction::Resume).await.unwrap();
        assert_eq!(
            manager
                .playback(PlaybackAction::Step)
                .await
                .unwrap_err()
                .status,
            StatusCode::CONFLICT
        );
        sleep(Duration::from_millis(700)).await;
        assert_eq!(manager.snapshot().await.state.completed_turn, 0);
        timeout(Duration::from_secs(2), async {
            while manager.snapshot().await.state.completed_turn == 0 {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let paused = manager.playback(PlaybackAction::Pause).await.unwrap();
        assert!(paused.paused);
        sleep(Duration::from_millis(1_150)).await;
        assert_eq!(manager.snapshot().await.state, paused.state);
        let stepped = manager.playback(PlaybackAction::Step).await.unwrap();
        assert_eq!(
            stepped.state.completed_turn,
            paused.state.completed_turn + 1
        );
        assert!(stepped.paused);
        manager.shutdown().await;
    }
}
