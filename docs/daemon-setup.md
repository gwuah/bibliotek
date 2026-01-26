# Daemon Setup & Frontend Bundling

Ship bibliotek as a single self-contained binary that runs as a macOS daemon.

## Goals

1. **Single binary distribution** - Frontend assets embedded in the Rust binary
2. **Auto-start on login** - launchd manages the process lifecycle
3. **Zero runtime dependencies** - No need for Node.js, npm, or separate asset directories

---

## Part 1: Embed Frontend Assets

**Decision:** Use `rust-embed` to compile `web/dist/` into the binary.

### Changes Required

1. Add dependency:
   ```toml
   rust-embed = { version = "8", features = ["axum"] }
   ```

2. Create `src/assets.rs` with an `Assets` struct that embeds `web/dist/`

3. Replace `ServeDir::new("web/static")` in `main.rs` with a fallback handler that:
   - Serves embedded files by path
   - Returns `index.html` for SPA routes (paths without file extensions)
   - Sets correct `Content-Type` headers via `mime_guess`

4. Update Makefile:
   ```makefile
   release:
   	cd web && npm ci && npm run build
   	cargo build --release
   ```

### Development vs Production

| Mode | Frontend | Backend |
|------|----------|---------|
| Dev | `cd web && npm run dev` (hot reload on :5173) | `cargo run -- -c config.yaml` (API on :5678) |
| Release | Built into binary | `make release` produces single executable |

**Local development:** When you pass `--config` (or `-c`), the database is created in the same directory as the config file. So `cargo run -- -c config.yaml` creates `./bibliotek.db` in the project root.

**Production:** When no `--config` is passed, defaults to `~/.config/bibliotek/` for config and database.

No `build.rs` script. Keep the build process explicit and simple.

---

## Part 2: macOS Daemon (launchd)

The binary handles all path resolution automatically:
- Config: `~/.config/bibliotek/config.yaml`
- Database: `~/.config/bibliotek/bibliotek.db`

Credentials go directly in `config.yaml` (gitignored).

### Minimal plist

**Location:** `~/Library/LaunchAgents/com.gwuah.bibliotek.plist`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.gwuah.bibliotek</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/bibliotek</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>/tmp/bibliotek.log</string>

    <key>StandardErrorPath</key>
    <string>/tmp/bibliotek.error.log</string>
</dict>
</plist>
```

No `WorkingDirectory` or `EnvironmentVariables` needed - the binary resolves `~/.config/bibliotek/` internally and loads `.env` from there.

### Commands

```bash
# Install
launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist

# Uninstall
launchctl unload ~/Library/LaunchAgents/com.gwuah.bibliotek.plist

# Restart after rebuild
launchctl stop com.gwuah.bibliotek && launchctl start com.gwuah.bibliotek

# Logs
tail -f /tmp/bibliotek.log
```

---

## Installation

```bash
make install
```

This will:
1. Build the release binary with embedded frontend
2. Copy binary to `/usr/local/bin/bibliotek`
3. Create `~/.config/bibliotek/` with config template
4. Install the launchd plist

Then:
1. Edit `~/.config/bibliotek/config.yaml` with your AWS credentials
2. Run `launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist`

### Config Directory Structure

```
~/.config/bibliotek/
├── config.yaml    # App configuration (including credentials)
└── bibliotek.db   # SQLite database (created on first run)
```
