#!/bin/bash

set -euo pipefail

NAME=liotek
LABEL="com.$(id -un).$NAME"
DOMAIN="gui/$(id -u)"

NEW_BINARY="${1:-target/release/$NAME}"
BIN_DIR="$HOME/.local/bin"
BINARY="$BIN_DIR/$NAME"
PREVIOUS="$BINARY.previous"
CONFIG="${LIOTEK_CONFIG:-$HOME/.$NAME/config.yaml}"
LOG_DIR="$HOME/Library/Logs/$NAME"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

fail() { echo "install: $*" >&2; exit 1; }

[ -x "$NEW_BINARY" ] || fail "$NEW_BINARY is not built; run \`make release\` first"
[ -f "$CONFIG" ] || fail "$CONFIG does not exist; copy config.example.yaml there and edit it"

write_plist() {
  cat > "$1" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>$BINARY</string>
    <string>serve</string>
    <string>--config</string>
    <string>$CONFIG</string>
  </array>
  <key>RunAtLoad</key><true/>
  <!-- Restart after a crash, but not after a clean exit, and never faster than every
       30 seconds: a permanent configuration error must not become a hot loop. -->
  <key>KeepAlive</key>
  <dict><key>SuccessfulExit</key><false/></dict>
  <key>ThrottleInterval</key><integer>30</integer>
  <key>StandardOutPath</key><string>$LOG_DIR/$NAME.log</string>
  <key>StandardErrorPath</key><string>$LOG_DIR/$NAME.log</string>
</dict>
</plist>
PLIST
}

is_loaded() { launchctl print "$DOMAIN/$LABEL" >/dev/null 2>&1; }

start_or_restart() {
  if is_loaded && [ "$1" = unchanged ]; then
    launchctl kickstart -k "$DOMAIN/$LABEL"
  else
    is_loaded && launchctl bootout "$DOMAIN/$LABEL"
    launchctl bootstrap "$DOMAIN" "$PLIST"
  fi
}

wait_until_healthy() {
  for _ in $(seq 1 20); do
    "$BINARY" health --config "$CONFIG" >/dev/null 2>&1 && return 0
    sleep 0.5
  done
  return 1
}

echo "Checking $NEW_BINARY against $CONFIG"
"$NEW_BINARY" doctor --config "$CONFIG" || fail "doctor found a problem; nothing was changed"

mkdir -p "$BIN_DIR" "$LOG_DIR" "$(dirname "$PLIST")"

# Stage beside the destination so the final rename is atomic on the same filesystem.
cp "$NEW_BINARY" "$BINARY.staged"
[ -f "$BINARY" ] && cp -p "$BINARY" "$PREVIOUS"
mv -f "$BINARY.staged" "$BINARY"

write_plist "$PLIST.staged"
if [ -f "$PLIST" ] && cmp -s "$PLIST" "$PLIST.staged"; then
  plist_state=unchanged
  rm "$PLIST.staged"
else
  plist_state=changed
  mv -f "$PLIST.staged" "$PLIST"
fi

start_or_restart "$plist_state"

if wait_until_healthy; then
  echo "Installed: $("$BINARY" --version)"
  echo "Logs: $LOG_DIR/$NAME.log"
  exit 0
fi

echo "install: the new binary did not become healthy" >&2
if [ -f "$PREVIOUS" ]; then
  mv -f "$PREVIOUS" "$BINARY"
  launchctl kickstart -k "$DOMAIN/$LABEL"
  if wait_until_healthy; then
    fail "restored the previous binary, which is running again; see $LOG_DIR/$NAME.log"
  fi
  fail "restored the previous binary, but it is not healthy either; see $LOG_DIR/$NAME.log"
fi
fail "there is no previous binary to restore; see $LOG_DIR/$NAME.log"
