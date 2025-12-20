# Daemon Setup & Frontend Bundling Plan

This document outlines how to run bibliotek as a persistent macOS daemon with the frontend assets embedded in the binary.

---

## Part 1: Embedding Frontend Assets in the Binary

### Current State

- Frontend uses Vite + React in `web/static/`
- `npm run dev` starts a dev server with hot reload and API proxying
- Rust server serves static files at runtime via `ServeDir::new("web/static")`

### Goal

Compile frontend assets into the Rust binary so a single executable serves everything.

### Implementation Steps

#### 1. Add `rust-embed` dependency

```toml
# Cargo.toml
[dependencies]
rust-embed = { version = "8", features = ["axum"] }
```

#### 2. Build frontend to `web/dist`

```bash
cd web && npm run build
```

This outputs production-ready assets to `web/dist/` (configured in `vite.config.js`).

#### 3. Create an embedded assets module

```rust
// src/assets.rs
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "web/dist"]
pub struct Assets;
```

#### 4. Serve embedded assets in axum

Replace the current `ServeDir` with embedded file serving:

```rust
// src/main.rs
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
};
use bibliotek::assets::Assets;

async fn serve_static(req: Request<Body>) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');

    // Default to index.html for SPA routing
    let path = if path.is_empty() || !path.contains('.') {
        "index.html"
    } else {
        path
    };

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
```

#### 5. Update router

```rust
let app = Router::new()
    // API routes first (they take precedence)
    .route("/books", get(get_books))
    .route("/books/:id", put(update_book))
    .route("/metadata", get(get_metadata))
    .route("/authors", post(create_author))
    .route("/tags", post(create_tag))
    .route("/categories", post(create_category))
    .route("/upload", get(show_form).post(upload))
    // Fallback to embedded static files
    .fallback(serve_static)
    .with_state(app_state);
```

#### 6. Add build script (optional)

Create a `build.rs` to automatically rebuild frontend:

```rust
// build.rs
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=web/static/");

    // Only build in release mode to speed up dev builds
    if std::env::var("PROFILE").unwrap() == "release" {
        let status = Command::new("npm")
            .args(["run", "build"])
            .current_dir("web")
            .status()
            .expect("Failed to run npm build");

        if !status.success() {
            panic!("Frontend build failed");
        }
    }
}
```

#### 7. Update Makefile for release builds

```makefile
.PHONY: build release

build:
	cargo build

release:
	cd web && npm ci && npm run build
	cargo build --release
```

### Development Workflow

For **development**, continue using:

```bash
# Terminal 1: Backend
cargo run -- --config-path config.yaml

# Terminal 2: Frontend with hot reload
cd web && npm run dev
```

For **production/release**:

```bash
make release
# Single binary at target/release/bibliotek serves everything
```

---

## Part 2: Running as a macOS Daemon (launchd)

### Prerequisites

1. Build the release binary: `make release`
2. The binary will be at `target/release/bibliotek`

### Create the launchd plist

Save to `~/Library/LaunchAgents/com.gwuah.bibliotek.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.gwuah.bibliotek</string>

    <key>ProgramArguments</key>
    <array>
        <string>/Users/gwuah/.cursor/worktrees/bibliotek/vqf/target/release/bibliotek</string>
        <string>--config-path</string>
        <string>/Users/gwuah/.cursor/worktrees/bibliotek/vqf/config.yaml</string>
    </array>

    <key>WorkingDirectory</key>
    <string>/Users/gwuah/.cursor/worktrees/bibliotek/vqf</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>AWS_ACCESS_KEY_ID</key>
        <string>YOUR_ACCESS_KEY</string>
        <key>AWS_SECRET_ACCESS_KEY</key>
        <string>YOUR_SECRET_KEY</string>
    </dict>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>/Users/gwuah/Library/Logs/bibliotek.log</string>

    <key>StandardErrorPath</key>
    <string>/Users/gwuah/Library/Logs/bibliotek.error.log</string>
</dict>
</plist>
```

### Install and manage the daemon

```bash
# Install (load) the daemon - starts immediately due to RunAtLoad
launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist

# Check status
launchctl list | grep bibliotek

# View logs
tail -f ~/Library/Logs/bibliotek.log

# Stop temporarily
launchctl stop com.gwuah.bibliotek

# Start again
launchctl start com.gwuah.bibliotek

# Uninstall (unload) the daemon
launchctl unload ~/Library/LaunchAgents/com.gwuah.bibliotek.plist

# Reload after updating the plist or binary
launchctl unload ~/Library/LaunchAgents/com.gwuah.bibliotek.plist
launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist
```

### Key launchd options explained

| Option                 | Description                                          |
| ---------------------- | ---------------------------------------------------- |
| `RunAtLoad`            | Start when plist is loaded (i.e., on login)          |
| `KeepAlive`            | Restart automatically if the process crashes         |
| `WorkingDirectory`     | CWD for the process (needed if using relative paths) |
| `EnvironmentVariables` | Set env vars the process needs                       |

---

## Part 3: Recommended Final Setup

Once frontend bundling is implemented:

1. **Binary location**: Consider copying the release binary to a stable path:

   ```bash
   sudo cp target/release/bibliotek /usr/local/bin/bibliotek
   ```

2. **Config location**: Store config in a standard location:

   ```bash
   mkdir -p ~/.config/bibliotek
   cp config.yaml ~/.config/bibliotek/
   ```

3. **Update plist paths**:

   ```xml
   <string>/usr/local/bin/bibliotek</string>
   <string>--config-path</string>
   <string>/Users/gwuah/.config/bibliotek/config.yaml</string>
   ```

4. **Remove WorkingDirectory** once assets are embedded (no longer needed).

---

## Quick Reference

```bash
# Full release build
make release

# Install daemon
launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist

# Check if running
launchctl list | grep bibliotek

# Logs
tail -f ~/Library/Logs/bibliotek.log

# Restart after rebuild
launchctl stop com.gwuah.bibliotek && launchctl start com.gwuah.bibliotek
```
