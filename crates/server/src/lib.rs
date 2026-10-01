use axum::{
    extract::{rejection::JsonRejection, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use game_core::GameState;
use runner::{run_match, RunEnd, RunnerConfig};
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
    pub bot_names: BotNames,
}

struct ActiveMatch {
    cancel: watch::Sender<bool>,
    task: JoinHandle<()>,
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

    async fn launch(&self, restart: bool, delay: u64) -> Result<Snapshot, ApiError> {
        validate_delay(delay)?;
        let mut active = self.active.lock().await;
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
            current.clone()
        };
        let config = self.config.clone();
        let shared = self.snapshot.clone();
        let delay = self.delay_ms.clone();
        let (cancel, receiver) = watch::channel(false);
        let task = tokio::spawn(async move {
            let publish_to = shared.clone();
            let result = run_match(config, receiver, delay, move |state| {
                let shared = publish_to.clone();
                async move {
                    let mut current = shared.write().await;
                    if current.match_id == Some(id) && current.status == MatchStatus::Running {
                        current.state = state;
                    }
                }
            })
            .await;
            let mut current = shared.write().await;
            if current.match_id != Some(id) {
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
        });
        *active = Some(ActiveMatch { cancel, task });
        Ok(initial)
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
    Ok(Json(
        manager
            .launch(false, options(payload)?.turn_delay_ms)
            .await?,
    ))
}

async fn restart_match(
    State(manager): State<Arc<MatchManager>>,
    payload: Result<Json<MatchOptions>, JsonRejection>,
) -> Result<Json<Snapshot>, ApiError> {
    Ok(Json(
        manager
            .launch(true, options(payload)?.turn_delay_ms)
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

pub fn app(root: &Path, manager: Arc<MatchManager>) -> Router {
    Router::new()
        .route("/api/match", get(get_match))
        .route("/api/match/start", post(start_match))
        .route("/api/match/restart", post(restart_match))
        .route("/api/match/speed", post(speed))
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
        let first = manager.launch(false, 1_000).await.unwrap();
        assert_eq!(first.state.completed_turn, 0);
        assert_eq!(
            manager.launch(false, 0).await.unwrap_err().status,
            StatusCode::CONFLICT
        );
        let second = manager.launch(true, 0).await.unwrap();
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
        manager.launch(false, 0).await.unwrap();
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
}
