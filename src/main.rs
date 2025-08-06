use axum::{
    Router,
    // extract::{Path, Query, State},
    // http::StatusCode,
    // response::IntoResponse,
    routing::get,
};
use bibliotek::handler::healthcheck;
// use std::future::Future;
use tokio::{signal, sync::broadcast};
use tracing::{error, info};
// use serde::{Deserialize, Serialize};

struct Config {
    port: String,
}

impl Config {
    fn new() -> Self {
        // to load from environment variables
        // hardcoded for now.
        Config {
            port: "5678".to_string(),
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();
    info!("bibliotek.svc starting");

    let cfg = Config::new();
    let address = format!("0.0.0.0:{}", cfg.port.clone());

    let (sender, _) = broadcast::channel::<()>(1);
    let app = Router::new().route("/", get(healthcheck));
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .unwrap_or_else(|e| {
            error!(error = %e, "failed to setup tcp listener");
            std::process::exit(1);
        });

    info!("bibliotek.svc running on {}", &address);
    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(err) = result {
                error!(error = %err, "failed to setup tcp listener");
                std::process::exit(1);
            }
        }
        _ = signal::ctrl_c() => {
            info!("ctrl+c signal received, preparing to shutdown");
            _ = sender.send(());
        }
    }
}
