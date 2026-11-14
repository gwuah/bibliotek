#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

use liotek::blob::BlobStore;
use liotek::blob::filesystem::FilesystemStore;
use liotek::config::{BlobStoreConfig, Config, DatabaseConfig, Environment, Secret};
use liotek::db::Db;
use liotek::http::{self, AppState};

pub const ADMIN_TOKEN: &str = "test-admin-token";
pub const EXTENSION_ORIGIN: &str = "chrome-extension://liotek-test";

pub struct TestApp {
    pub state: AppState,
    router: Router,
    pub dir: TempDir,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub bytes: bytes::Bytes,
}

impl TestResponse {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes)
            .unwrap_or_else(|_| panic!("body is not JSON: {}", String::from_utf8_lossy(&self.bytes)))
    }

    pub fn data(&self) -> Value {
        assert!(
            self.status.is_success(),
            "expected success, got {}: {}",
            self.status,
            self.json()
        );
        self.json()["data"].clone()
    }

    pub fn error_code(&self) -> String {
        self.json()["error"]["code"]
            .as_str()
            .expect("an error code")
            .to_string()
    }
}

impl TestApp {
    pub async fn start() -> TestApp {
        TestApp::start_configured(|_, _| {}).await
    }

    pub async fn start_in(dir: TempDir, blobs: Arc<dyn BlobStore>) -> TestApp {
        let config = config_in(dir.path());
        TestApp::start_with(dir, blobs, config).await
    }

    pub async fn start_configured(configure: impl FnOnce(&mut Config, &Path)) -> TestApp {
        let dir = tempfile::tempdir().expect("create a temporary directory");
        let blobs = Arc::new(
            FilesystemStore::open(&dir.path().join("blobs"))
                .await
                .expect("open blob store"),
        );
        let mut config = config_in(dir.path());
        configure(&mut config, dir.path());
        TestApp::start_with(dir, blobs, config).await
    }

    async fn start_with(dir: TempDir, blobs: Arc<dyn BlobStore>, config: Config) -> TestApp {
        let db = Db::open(&config.database).await.expect("open database");
        db.migrate().await.expect("migrate");

        let state = AppState {
            config: Arc::new(config),
            db: Arc::new(db),
            blobs,
        };
        TestApp {
            router: http::router(state.clone()),
            state,
            dir,
        }
    }

    pub async fn send(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("the router is infallible");
        let (parts, body) = response.into_parts();
        let bytes = body.collect().await.expect("read response body").to_bytes();
        TestResponse {
            status: parts.status,
            headers: parts.headers,
            bytes,
        }
    }

    pub async fn get(&self, path: &str) -> TestResponse {
        self.send(Request::get(path).body(Body::empty()).unwrap()).await
    }

    pub async fn admin(&self, method: Method, path: &str, body: Option<Value>) -> TestResponse {
        self.send(json_request(method, path, body, Some(ADMIN_TOKEN)))
            .await
    }

    pub async fn anonymous(&self, method: Method, path: &str, body: Option<Value>) -> TestResponse {
        self.send(json_request(method, path, body, None)).await
    }
}

pub fn config_in(dir: &Path) -> Config {
    Config {
        environment: Environment::Development,
        config_path: dir.join("config.yaml"),
        data_dir: dir.to_path_buf(),
        bind: "127.0.0.1:0".parse().unwrap(),
        admin_token: Secret::new(ADMIN_TOKEN),
        extension_origins: vec![EXTENSION_ORIGIN.to_string()],
        database: DatabaseConfig {
            path: dir.join("liotek.db"),
        },
        blob_store: BlobStoreConfig::Filesystem {
            root: dir.join("blobs"),
        },
        research_database: None,
        reader_books_dir: None,
        backups: None,
    }
}

fn json_request(method: Method, path: &str, body: Option<Value>, token: Option<&str>) -> Request<Body> {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => request.body(Body::empty()).unwrap(),
    }
}
