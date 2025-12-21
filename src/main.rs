use std::sync::Arc;

use axum::http::Method;
use axum::{
    Router,
    routing::{get, post, put},
};
use bibliotek::commonplace;
use bibliotek::db::Database;
use bibliotek::handler::{
    AppState, create_author, create_category, create_tag, get_books, get_metadata, healthcheck,
    serve_index, update_book, upload,
};
use bibliotek::light;
use bibliotek::research;
use bibliotek::s3::ObjectStorage;
use bibliotek::{
    config::{Cli, Config},
    handler::show_form,
};
use clap::Parser;
use tokio::{signal, sync::mpsc};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing;

#[tokio::main]
async fn main() {
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
    let s3 = Arc::new(ObjectStorage::new(&cfg).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to setup object storage");
        std::process::exit(1);
    }));

    let address = format!("0.0.0.0:{}", cfg.app.get_port().to_string());
    let cancellation_token = CancellationToken::new();
    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel::<()>(1);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(healthcheck))
        .route("/index.html", get(serve_index))
        .route("/upload", get(show_form))
        .route("/books", get(get_books))
        .route("/books/:id", put(update_book))
        .route("/metadata", get(get_metadata))
        .route("/authors", post(create_author))
        .route("/tags", post(create_tag))
        .route("/categories", post(create_category))
        .route("/upload", post(upload))
        .nest("/commonplace", commonplace::routes())
        .nest("/light", light::routes())
        .nest("/research", research::routes())
        .nest_service("/static", ServeDir::new("web/static"))
        .layer(cors)
        .with_state(AppState { db, s3 });

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
