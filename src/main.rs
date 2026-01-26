use std::sync::Arc;

use axum::http::Method;
use axum::{
    Router,
    routing::{get, post, put},
};
use bibliotek::assets::serve_embedded;
use bibliotek::commonplace;
use bibliotek::db::Database;
use bibliotek::handler::{
    AppState, create_author, create_category, create_tag, get_books, get_metadata, healthcheck, serve_index,
    update_book, upload,
};
use bibliotek::light;
use bibliotek::research;
use bibliotek::s3::ObjectStorage;
use bibliotek::{
    config::{Cli, Config, default_config_dir, default_config_path},
    handler::show_form,
};
use clap::Parser;
use tokio::{signal, sync::mpsc};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tracing;

#[tokio::main]
async fn main() {
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

    tracing_subscriber::fmt().json().init();
    tracing::info!("bibliotek.svc starting");

    let cfg = Config::new(config_path.to_str().unwrap()).unwrap_or_else(|e| {
        tracing::error!(error = %e, path = ?config_path, "failed to load config file");
        std::process::exit(1);
    });
    let db = Arc::new(Database::new(&cfg, &data_dir).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to setup database");
        std::process::exit(1);
    }));
    let s3 = Arc::new(ObjectStorage::new(&cfg).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to setup object storage");
        std::process::exit(1);
    }));

    let address = format!("0.0.0.0:{}", cfg.app.get_port().to_string());
    let cancellation_token = CancellationToken::new();
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel::<()>(1);

    // Background task to clean up stale upload sessions every 5 minutes
    let cleanup_s3 = s3.clone();
    let cleanup_token = cancellation_token.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = cleanup_s3.cleanup_stale_sessions(1800).await {
                        tracing::warn!("Failed to cleanup stale sessions: {}", e);
                    }
                }
                _ = cleanup_token.cancelled() => {
                    tracing::info!("Session cleanup task shutting down");
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
        .route("/index.html", get(serve_index))
        .route("/books", get(get_books))
        .route("/books/:id", put(update_book))
        .route("/metadata", get(get_metadata))
        .route("/authors", post(create_author))
        .route("/tags", post(create_tag))
        .route("/categories", post(create_category))
        .route("/upload", get(show_form))
        .route("/upload", post(upload))
        .nest("/commonplace", commonplace::routes())
        .nest("/light", light::routes())
        .nest("/research", research::routes())
        .fallback(serve_embedded)
        .layer(cors)
        .with_state(AppState { db, s3 });

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
