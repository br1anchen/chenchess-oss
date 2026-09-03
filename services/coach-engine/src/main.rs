use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use chen_chess_coach_engine::{
    account_deletion::configured_account_deletion_runtime,
    app,
    auth::AuthConfig,
    beta_access::configured_beta_access_runtime,
    engine_analysis::{EngineAnalyzer, EngineWorkerLimit, ExactEngineCache, StockfishAdapter},
    human_move_model::{ExactHumanMoveCache, HumanMoveModel, MaiaHttpAdapter},
    opening_analysis::OpeningAnalysisRuntime,
    pin_record::configured_language_layer_runtime,
    quality_capture::configured_quality_capture_runtime,
    review_session_runtime::build_review_session_executors,
    review_session_transport::ReviewSessionWebBinding,
    review_share::configured_review_share_store,
    types::AppState,
};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_env();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "chen_chess_coach_engine=debug,tower_http=info,axum=info".into()
            }),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true),
        )
        .init();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8787);
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

    let human_move_model = MaiaHttpAdapter::from_env().map(|adapter| {
        Arc::new(ExactHumanMoveCache::new(Arc::new(adapter))) as Arc<dyn HumanMoveModel>
    });
    let engine_analyzer = StockfishAdapter::from_env()?
        .map(|adapter| EngineWorkerLimit::from_env(Arc::new(adapter)))
        .transpose()?
        .map(|adapter| {
            tracing::info!(
                stockfish_workers = adapter.workers(),
                "engine runtime configured"
            );
            Arc::new(ExactEngineCache::new(Arc::new(adapter))) as Arc<dyn EngineAnalyzer>
        });
    let hosted_language_layer = configured_language_layer_runtime().await;
    let quality_capture = std::sync::Arc::new(configured_quality_capture_runtime().await?);
    let account_deletion = configured_account_deletion_runtime().await?;
    let beta_access = configured_beta_access_runtime().await?;
    let review_executors = build_review_session_executors(
        engine_analyzer.clone(),
        human_move_model.clone(),
        hosted_language_layer,
    )?;
    let daily_coaching = review_executors.daily_coaching_runtime()?;
    let imported_games = review_executors.imported_games_runtime();
    daily_coaching.spawn_scheduler();
    let review_session = ReviewSessionWebBinding::new(review_executors.command())
        .with_quality_capture_runtime(quality_capture.clone())
        .with_review_share_store(configured_review_share_store()?);
    quality_capture.spawn_exporter();
    account_deletion.spawn_recovery();
    let state = Arc::new(AppState {
        account_deletion,
        auth: AuthConfig::from_env()?,
        beta_access,
        daily_coaching,
        imported_games,
        opening_analysis: OpeningAnalysisRuntime::new(engine_analyzer.clone()),
        review_session,
    });

    let app = app(state);

    let addr = listen_address(&host, port)?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("chess coach api listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn listen_address(host: &str, port: u16) -> anyhow::Result<SocketAddr> {
    let ip = host
        .parse::<IpAddr>()
        .map_err(|error| anyhow::anyhow!("HOST must be an IP address: {error}"))?;
    Ok(SocketAddr::new(ip, port))
}

fn load_env() {
    let api_env = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    dotenvy::from_path(api_env).ok();
    dotenvy::dotenv().ok();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::listen_address;

    #[test]
    fn container_listener_accepts_an_explicit_all_interfaces_host() {
        assert_eq!(
            listen_address("0.0.0.0", 8787).expect("container host should parse"),
            "0.0.0.0:8787"
                .parse()
                .expect("expected address should parse")
        );
    }

    #[test]
    fn invalid_listener_host_is_rejected() {
        assert!(listen_address("not a host", 8787).is_err());
    }
}
