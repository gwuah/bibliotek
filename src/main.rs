use std::sync::Arc;

use axum::{
    Router,
    routing::get,
    // extract::{Path, Query, State},
    // http::StatusCode,
    // response::IntoResponse,
};
use bibliotek::config::{Cli, Config};
use bibliotek::db::Database;
use bibliotek::handler::{get_books, healthcheck};
use clap::Parser;
use tokio::{signal, sync::mpsc};
use tokio_util::sync::CancellationToken;
use tracing;

#[tokio::main]
async fn main() {
    // Load .env file if it exists
    dotenv::dotenv().ok();

    tracing_subscriber::fmt().json().init();
    tracing::info!("bibliotek.svc starting");

    let args = Cli::parse();
    let cfg = Config::new(&args.config_path).unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to load config file");
        std::process::exit(1);
    });
    let db = Arc::new(Database::new(&cfg).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to setup database");
        std::process::exit(1);
    }));
    let address = format!("0.0.0.0:{}", cfg.app.get_port().to_string());
    let cancellation_token = CancellationToken::new();
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel::<()>(1);

    let app = Router::new()
        .route("/", get(healthcheck))
        .route("/books", get(get_books));

    let app = app.with_state(db);

    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to setup tcp listener");
            std::process::exit(1);
        });

    tracing::info!("bibliotek.svc running on {}", &address);
    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(err) = result {
                tracing::error!(error = %err, "failed to setup tcp listener");
                std::process::exit(1);
            }
        }
        _ = signal::ctrl_c() => {
            tracing::info!("ctrl+c signal received, preparing to shutdown");
            cancellation_token.cancel();
        }
    }

    drop(shutdown_complete_tx);
    shutdown_complete_rx.recv().await;
    tracing::info!("bibliotek.svc going off, graceful shutdown complete");
}
