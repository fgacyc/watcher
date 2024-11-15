use std::{path::PathBuf, time::Duration};

use axum::{
    routing::{get, patch, post},
    Router,
};
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
    timeout::TimeoutLayer,
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{handler::*, state::AppState};

mod config;
// mod handler;
// mod state;
// mod types;
mod websocket;

type Error = Box<dyn std::error::Error + Send + Sync>;

fn main() {
    // This returns an error if the `.env` file doesn't exist, but that's not what we want
    // since we're not going to use a `.env` file if we deploy this application.
    dotenvy::dotenv().ok();

    // Parse configuration
    // NOTE: We need to call these functions once to initialise the `OnceLock`
    let config = config::config();

    // Enable tracing.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let rt = tokio::runtime::Runtime::new().expect("failed to start a tokio runtime");

    rt.block_on(async move {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");

        let pool = sqlx::PgPool::connect(&config.database_url).await?;
        let state = AppState::new(pool);

        let router = Router::new()
            .fallback_service(ServeDir::new(assets_dir))
            .route("/ws", get(ws_handler))
            .route("/assets", get(get_account_handler))
            // Cross Origin Resource Sharing (CORS)
            .layer(CorsLayer::permissive())
            // Tracing for each HTTP request
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(DefaultMakeSpan::default().include_headers(true)),
            )
            .layer(TimeoutLayer::new(Duration::from_secs(10)))
            .with_state(state);

        let addr = format!("{}:{}", config.address, config.port);
        tracing::info!("Starting the server on {}", addr);
        let listener = tokio::net::TcpListener::bind(addr).await?;

        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal())
        .await?;

        Ok::<_, Error>(())
    })
    .expect("failed running the web server");
}

/// Detect a shutdown signal (typically CTRL + C)
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
