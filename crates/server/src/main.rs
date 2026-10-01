use runner::RunnerConfig;
use std::{
    env,
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

fn setting(name: &str, default: u64) -> Result<u64, Box<dyn Error>> {
    let value = match env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| format!("{name}는 양의 정수여야 합니다"))?,
        Err(env::VarError::NotPresent) => default,
        Err(error) => return Err(error.into()),
    };
    if value == 0 {
        return Err(format!("{name}는 0보다 커야 합니다").into());
    }
    Ok(value)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let port = u16::try_from(setting("BOT_ARENA_PORT", 3000)?)?;
    let mut config = RunnerConfig::local(&root);
    if let Some(python) = env::var_os("BOT_ARENA_PYTHON") {
        config.python = PathBuf::from(python);
    }
    config.turn_timeout = Duration::from_millis(setting("BOT_ARENA_TIMEOUT_MS", 1_000)?);
    config.max_output_bytes = usize::try_from(setting("BOT_ARENA_OUTPUT_BYTES", 65_536)?)?;
    config.stderr_tail_bytes = usize::try_from(setting("BOT_ARENA_STDERR_BYTES", 8_192)?)?;
    let manager = server::MatchManager::new(config);
    let app = server::app(&root, manager.clone());
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("BOT ARENA  http://{address}");
    println!("로컬 예제 봇 전용 · 종료: Ctrl+C");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = terminate.recv() => {},
                }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
            manager.shutdown().await;
        })
        .await?;
    Ok(())
}
