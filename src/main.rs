use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::http::Method;
use axum::{
    Router,
    routing::{get, post, put},
};
use bibliotek::assets::serve_embedded;
use bibliotek::commonplace;
use bibliotek::config::{Cli, Config, default_config_dir, default_config_path};
use bibliotek::db::Database;
use bibliotek::handler::{
    AppState, abort_upload, create_author, create_category, create_tag, get_books, get_download_url, get_metadata,
    get_pending_uploads, healthcheck, update_book, upload,
};
use bibliotek::light;
use bibliotek::research;
use bibliotek::resumable::ResumableUploadManager;
use clap::Parser;
use tokio::{signal, sync::mpsc};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tracing;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args = Cli::parse();

    // Determine config path and data directory
    // If --config is provided, use its parent directory for data (database, etc.)
    // Otherwise use ~/.config/bibliotek/ for both
    let (config_path, data_dir) = match args.config_path {
        Some(path) => {
            let path = std::path::PathBuf::from(path);
            let dir = path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            (path, dir)
        }
        None => {
            let dir = default_config_dir();
            (default_config_path(), dir)
        }
    };

    // Ensure data directory exists
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("failed to create data directory {:?}: {}", data_dir, e);
        std::process::exit(1);
    }

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    tracing::info!("bibliotek.svc starting");

    let cfg = Config::new(config_path.to_str().unwrap()).unwrap_or_else(|e| {
        tracing::error!(error = %e, path = ?config_path, "failed to load config file");
        std::process::exit(1);
    });
    let db = Arc::new(Database::new(&cfg).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to setup database");
        std::process::exit(1);
    }));
    let resumable = Arc::new(ResumableUploadManager::new(&cfg).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to setup resumable upload manager");
        std::process::exit(1);
    }));

    let address = format!("0.0.0.0:{}", cfg.app.get_port().to_string());
    let cancellation_token = CancellationToken::new();
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel::<()>(1);

    db.start_sync_task(cfg.app.sync_interval_seconds, cancellation_token.clone());

    // Background task to clean up expired uploads every hour
    let cleanup_resumable = resumable.clone();
    let cleanup_token = cancellation_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600)); // 1 hour
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = cleanup_resumable.cleanup_expired(24).await { // 24 hours
                        tracing::warn!("Failed to cleanup expired uploads: {}", e);
                    }
                }
                _ = cleanup_token.cancelled() => {
                    tracing::info!("Upload cleanup task shutting down");
                    break;
                }
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(healthcheck))
        .route("/books", get(get_books))
        .route("/books/:id", put(update_book))
        .route("/metadata", get(get_metadata))
        .route("/authors", post(create_author))
        .route("/tags", post(create_tag))
        .route("/categories", post(create_category))
        .route("/upload", post(upload))
        .route("/upload/pending", get(get_pending_uploads))
        .route("/upload/abort", post(abort_upload))
        .route("/download", get(get_download_url))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .nest("/commonplace", commonplace::routes())
        .nest("/light", light::routes())
        .nest("/research", research::routes())
        .fallback(serve_embedded)
        .layer(cors)
        .with_state(AppState { db, resumable });

    let listener = tokio::net::TcpListener::bind(&address).await.unwrap_or_else(|e| {
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
