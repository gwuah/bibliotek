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
| Dev | `cd web && npm run dev` (hot reload on :5173) | `cargo run` (API on :5678) |
| Release | Built into binary | `make release` produces single executable |

No `build.rs` script. Keep the build process explicit and simple.

---

## Part 2: macOS Daemon (launchd)

**Location:** `~/Library/LaunchAgents/com.gwuah.bibliotek.plist`

### Minimal plist

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
        <string>--config-path</string>
        <string>~/.config/bibliotek/config.yaml</string>
    </array>

    <key>EnvironmentVariables</key>
    <dict>
        <key>AWS_ACCESS_KEY_ID</key>
        <string>TODO</string>
        <key>AWS_SECRET_ACCESS_KEY</key>
        <string>TODO</string>
    </dict>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>WorkingDirectory</key>
    <string>/Users/gwuah/.config/bibliotek</string>

    <key>StandardOutPath</key>
    <string>~/Library/Logs/bibliotek.log</string>

    <key>StandardErrorPath</key>
    <string>~/Library/Logs/bibliotek.error.log</string>
</dict>
</plist>
```

`WorkingDirectory` points to the config directory so relative paths in `config.yaml` (database, schema) resolve correctly.

### Commands

```bash
# Install
launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist

# Uninstall
launchctl unload ~/Library/LaunchAgents/com.gwuah.bibliotek.plist

# Restart after rebuild
launchctl stop com.gwuah.bibliotek && launchctl start com.gwuah.bibliotek

# Logs
tail -f ~/Library/Logs/bibliotek.log
```

---

## Installation Steps

```bash
# Automated
make install
# Then edit plist to set AWS credentials and run:
launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist
```

Or manually after `make release`:

```bash
cp target/release/bibliotek /usr/local/bin/
mkdir -p ~/.config/bibliotek
cp config.yaml ~/.config/bibliotek/
cp com.gwuah.bibliotek.plist ~/Library/LaunchAgents/
# Edit plist to set AWS credentials
launchctl load ~/Library/LaunchAgents/com.gwuah.bibliotek.plist
```
