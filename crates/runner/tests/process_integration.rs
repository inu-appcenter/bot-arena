//! Real Python subprocess checks for the local runner, including child reaping.
use game_core::{EndReason, GameState, MAX_TURNS};
use runner::{run_match, RunEnd, RunnerConfig};
use std::{
    fs,
    future::ready,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::watch, time::timeout};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bot-arena-runner-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        fs::copy(fixtures().join("common.py"), path.join("common.py")).unwrap();
        Self(path)
    }

    fn fixture(&self, name: &str, destination: &str) -> PathBuf {
        let path = self.0.join(format!("{destination}.py"));
        fs::copy(fixtures().join(format!("{name}.py")), &path).unwrap();
        path
    }

    fn example(&self, name: &str, destination: &str) -> PathBuf {
        let path = self.0.join(format!("{destination}.py"));
        let script = root().join(format!("bot-sdk/examples/{name}.py"));
        // Python consumes JSON's quoted string literal correctly for this path.
        let script_literal = serde_json::to_string(&script.to_string_lossy()).unwrap();
        fs::write(
            &path,
            format!("import common\nimport runpy\nrunpy.run_path({script_literal}, run_name='__main__')\n"),
        )
        .unwrap();
        path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn no_delay() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(0))
}

fn pid(script: &Path) -> u32 {
    fs::read_to_string(script.with_extension("pid"))
        .expect("bot did not create its test process marker")
        .parse()
        .unwrap()
}

fn assert_reaped(script: &Path) {
    let pid = pid(script);
    let status = Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill -0 is needed to verify test subprocess cleanup");
    assert!(
        !status.success(),
        "bot PID {pid} still exists after runner returned"
    );
}

async fn wait_for_markers(paths: &[PathBuf; 2]) {
    timeout(Duration::from_secs(3), async {
        while paths
            .iter()
            .any(|path| !path.with_extension("pid").exists())
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("bot processes did not start");
}

async fn collect(config: RunnerConfig) -> (Result<RunEnd, String>, Vec<GameState>) {
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let mut states = Vec::new();
    let result = timeout(
        Duration::from_secs(15),
        run_match(config, cancel_rx, no_delay(), |state| {
            states.push(state);
            ready(())
        }),
    )
    .await
    .expect("the runner exceeded the integration test deadline");
    (result, states)
}

#[tokio::test]
async fn actual_examples_collect_deliver_and_finish_with_every_snapshot_valid() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.bot_paths = [
        directory.example("basic_bot", "a"),
        directory.example("strategic_bot", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (result, states) = collect(config).await;
    assert_eq!(result, Ok(RunEnd::Finished));
    assert!(!states.is_empty());
    for (index, state) in states.iter().enumerate() {
        assert_eq!(usize::from(state.completed_turn), index + 1);
        state.validate().unwrap();
        if index + 1 < states.len() {
            assert!(state.outcome.is_none());
        }
    }
    assert!(states
        .iter()
        .any(|state| state.robots.iter().any(|robot| robot.cargo > 0)));
    let final_state = states.last().unwrap();
    assert!(final_state.scores.a > 0 && final_state.scores.b > 0);
    match final_state.outcome.unwrap().reason {
        EndReason::AllDelivered => assert_eq!(final_state.scores.a + final_state.scores.b, 96),
        EndReason::TurnLimit => assert_eq!(final_state.completed_turn, MAX_TURNS),
    }
    println!(
        "example match: {} turns, A {}, B {}",
        final_state.completed_turn, final_state.scores.a, final_state.scores.b
    );
    for path in paths {
        assert_reaped(&path);
    }
}

#[tokio::test]
async fn stateful_processes_survive_200_turns_and_fresh_match_resets_counters() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.bot_paths = [
        directory.fixture("stateful", "a"),
        directory.fixture("stateful", "b"),
    ];
    let paths = config.bot_paths.clone();
    let mut first_pids = None;
    for _ in 0..2 {
        let (result, states) = collect(config.clone()).await;
        assert_eq!(result, Ok(RunEnd::Finished));
        assert_eq!(states.len(), usize::from(MAX_TURNS));
        for (index, state) in states.iter().enumerate() {
            assert_eq!(usize::from(state.completed_turn), index + 1);
            state.validate().unwrap();
        }
        assert_eq!(states.last().unwrap().scores.a, 0);
        assert_eq!(states.last().unwrap().scores.b, 0);
        let pids = [pid(&paths[0]), pid(&paths[1])];
        if let Some(previous) = first_pids {
            assert_ne!(previous, pids);
        }
        first_pids = Some(pids);
        for path in &paths {
            assert_reaped(path);
        }
    }
}

#[tokio::test]
async fn both_teams_receive_identical_start_snapshots_concurrently_every_turn() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.bot_paths = [
        directory.fixture("parallel_same_snapshot", "a"),
        directory.fixture("parallel_same_snapshot", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (result, states) = collect(config).await;
    assert_eq!(result, Ok(RunEnd::Finished));
    assert_eq!(states.len(), usize::from(MAX_TURNS));
    for state in states {
        state.validate().unwrap();
    }
    for path in paths {
        assert_reaped(&path);
    }
}

#[tokio::test]
async fn malformed_json_structure_turn_and_large_output_fail_and_reap_both_bots() {
    for (fixture, expected) in [
        ("invalid_json", "잘못된 JSON"),
        ("invalid_structure", "actions는 배열"),
        ("wrong_turn", "턴 불일치"),
        ("invalid_action_identity", "robot_id"),
        ("invalid_action_record", "객체"),
        ("oversize", "출력 한도 초과"),
    ] {
        let directory = TestDirectory::new();
        let mut config = RunnerConfig::local(&root());
        config.max_output_bytes = 1_024;
        config.bot_paths = [
            directory.fixture(fixture, "a"),
            directory.example("strategic_bot", "b"),
        ];
        let paths = config.bot_paths.clone();
        let (result, states) = collect(config).await;
        let error = result.unwrap_err();
        assert!(error.contains(expected), "{fixture}: {error}");
        assert!(
            states.is_empty(),
            "a failed exchange published a partial turn"
        );
        for path in paths {
            assert_reaped(&path);
        }
    }
}

#[tokio::test]
async fn unexpected_python_exit_fails_without_publishing_a_partial_turn() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.bot_paths = [
        directory.fixture("early_exit", "a"),
        directory.example("basic_bot", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (result, states) = collect(config).await;
    let error = result.unwrap_err();
    assert!(error.contains("종료") || error.contains("닫혔"), "{error}");
    assert!(states.is_empty());
    for path in paths {
        assert_reaped(&path);
    }
}

#[tokio::test]
async fn exit_after_final_response_is_an_execution_error_and_reaps_both_children() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.bot_paths = [
        directory.fixture("last_response_exit", "a"),
        directory.fixture("last_turn_delayed", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (result, states) = collect(config).await;
    let error = result.expect_err("exiting after the final response must still fail");
    assert!(error.contains("종료"), "{error}");
    assert_eq!(states.last().unwrap().completed_turn, MAX_TURNS);
    for state in &states {
        state.validate().unwrap();
    }
    for path in paths {
        assert_reaped(&path);
    }
}

#[tokio::test]
async fn timeout_reaps_both_children_and_never_uses_late_response() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.turn_timeout = Duration::from_millis(300);
    config.bot_paths = [
        directory.fixture("timeout", "a"),
        directory.example("basic_bot", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (result, states) = collect(config).await;
    assert!(result.unwrap_err().contains("응답 시간 초과"));
    assert!(states.is_empty());
    for path in paths {
        assert_reaped(&path);
    }
}

#[tokio::test]
async fn stderr_flood_is_drained_and_only_bounded_tail_appears_in_error() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.stderr_tail_bytes = 512;
    config.bot_paths = [
        directory.fixture("stderr_flood", "a"),
        directory.example("basic_bot", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (result, states) = collect(config).await;
    let error = result.unwrap_err();
    assert!(
        error.contains("잘못된 JSON"),
        "stderr blocked bot output: {error}"
    );
    assert!(
        error.contains("TAIL-MARKER"),
        "final stderr chunk was not drained"
    );
    assert!(
        error.len() < 1_024,
        "unbounded stderr was included in error"
    );
    assert!(states.is_empty());
    for path in paths {
        assert_reaped(&path);
    }
}

#[tokio::test]
async fn cancellation_during_bot_io_returns_promptly_and_reaps_both_children() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.turn_timeout = Duration::from_secs(10);
    config.bot_paths = [
        directory.fixture("timeout", "a"),
        directory.example("basic_bot", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let published = Arc::new(AtomicU64::new(0));
    let count = published.clone();
    let task = tokio::spawn(run_match(config, cancel_rx, no_delay(), move |_| {
        count.fetch_add(1, Ordering::Relaxed);
        ready(())
    }));
    wait_for_markers(&paths).await;
    cancel_tx.send(true).unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap(),
        Ok(RunEnd::Cancelled)
    );
    assert_eq!(published.load(Ordering::Relaxed), 0);
    for path in paths {
        assert_reaped(&path);
    }
}

#[tokio::test]
async fn cancellation_during_display_interval_does_not_wait_or_publish_another_turn() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.bot_paths = [
        directory.example("basic_bot", "a"),
        directory.example("strategic_bot", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let published = Arc::new(AtomicU64::new(0));
    let count = published.clone();
    let task = tokio::spawn(run_match(
        config,
        cancel_rx,
        Arc::new(AtomicU64::new(10_000)),
        move |state| {
            count.store(u64::from(state.completed_turn), Ordering::Relaxed);
            ready(())
        },
    ));
    timeout(Duration::from_secs(3), async {
        while published.load(Ordering::Relaxed) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    cancel_tx.send(true).unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap(),
        Ok(RunEnd::Cancelled)
    );
    assert_eq!(published.load(Ordering::Relaxed), 1);
    for path in paths {
        assert_reaped(&path);
    }
}

#[tokio::test]
async fn pre_cancelled_match_starts_no_children() {
    let directory = TestDirectory::new();
    let mut config = RunnerConfig::local(&root());
    config.bot_paths = [
        directory.fixture("stateful", "a"),
        directory.fixture("stateful", "b"),
    ];
    let paths = config.bot_paths.clone();
    let (_cancel_tx, cancel_rx) = watch::channel(true);
    assert_eq!(
        run_match(config, cancel_rx, no_delay(), |_| ready(())).await,
        Ok(RunEnd::Cancelled)
    );
    assert!(paths
        .iter()
        .all(|path| !path.with_extension("pid").exists()));
}
