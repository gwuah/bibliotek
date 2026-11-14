#!/bin/bash

set -euo pipefail

NAME=liotek
LABEL="${LIOTEK_LABEL:-com.$(id -un).$NAME}"
DOMAIN="gui/$(id -u)"

NEW_BINARY="${1:-target/release/$NAME}"
BIN_DIR="$HOME/.local/bin"
BINARY="$BIN_DIR/$NAME"
PREVIOUS="$BINARY.previous"
FAILED="$BINARY.failed"
CONFIG="${LIOTEK_CONFIG:-$HOME/.$NAME/config.yaml}"
LOG_DIR="$HOME/Library/Logs/$NAME"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
SITE_HOST="${LIOTEK_HOSTNAME:-$NAME.test}"
CADDY_SITES="${LIOTEK_CADDY_SITES:-$HOME/.config/caddy/sites}"
CADDY_SITE="$CADDY_SITES/$SITE_HOST.caddy"

fail() { echo "install: $*" >&2; exit 1; }
warn() { echo "install: warning: $*" >&2; }

[ -x "$NEW_BINARY" ] || fail "$NEW_BINARY is not built; run \`make release\` first"
[ -f "$CONFIG" ] || fail "$CONFIG does not exist; copy config.example.yaml there and edit it"

configured_bind() {
  sed -nE 's/^bind:[[:space:]]*["'\'']?([^"'\''#[:space:]]+).*/\1/p' "$CONFIG" | head -n 1
}

upstream_for() {
  local host="${1%:*}" port="${1##*:}"
  case "$host" in
    0.0.0.0 | "[::]") echo "127.0.0.1:$port" ;;
    *) echo "$1" ;;
  esac
}

BIND=$(configured_bind)
[[ "$BIND" == *:[0-9]* ]] || fail "could not read \`bind: HOST:PORT\` from $CONFIG; nothing was changed"
UPSTREAM=$(upstream_for "$BIND")

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

# Taken with the installed binary: any liotek command migrates the database it opens,
# so the new one would back up an already-migrated schema the old one cannot read.
back_up_before_upgrade() {
  if [ ! -x "$BINARY" ]; then
    return
  fi
  if [ "${LIOTEK_SKIP_BACKUP:-}" = 1 ]; then
    echo "Skipping the pre-upgrade backup (LIOTEK_SKIP_BACKUP=1)"
    return
  fi
  local output
  output=$("$BINARY" backup --config "$CONFIG") \
    || fail "could not back up the database before upgrading; nothing was changed"
  BACKUP_NAME=$(printf '%s\n' "$output" | sed -nE 's/.*"name": *"([^"]+)".*/\1/p' | head -n 1)
  [ -n "$BACKUP_NAME" ] || fail "the backup did not report its name; nothing was changed"
  echo "Backed up the database as $BACKUP_NAME"
}

# Used when the previous binary cannot start either: the new one has migrated the
# database to a schema the previous one refuses.
restore_pre_upgrade_backup() {
  launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
  sleep 1
  "$FAILED" restore --config "$CONFIG" --from "$BACKUP_NAME" --set-aside-current \
    || fail "could not restore $BACKUP_NAME; liotek is stopped. Restore it by hand with \`liotek restore\`"
  launchctl bootstrap "$DOMAIN" "$PLIST"
}

caddyfile() {
  if [ -n "${LIOTEK_CADDYFILE:-}" ]; then
    echo "$LIOTEK_CADDYFILE"
  elif command -v brew >/dev/null; then
    echo "$(brew --prefix)/etc/Caddyfile"
  fi
}

write_caddy_site() {
  mkdir -p "$CADDY_SITES"
  cat > "$CADDY_SITE" <<SITE
http://$SITE_HOST {
	@elsewhere not remote_ip 127.0.0.1 ::1
	abort @elsewhere
	reverse_proxy $UPSTREAM
}
SITE
}

# Caddy only adds the hostname; liotek is already installed and healthy, so nothing
# here may fail the install.
install_caddy_site() {
  write_caddy_site
  echo "Wrote $CADDY_SITE (http://$SITE_HOST -> $UPSTREAM)"

  if ! command -v caddy >/dev/null; then
    warn "Caddy is not installed; http://$SITE_HOST works once it is (see ctx.md, Local hostname)"
    return
  fi
  local main
  main=$(caddyfile)
  if [ -z "$main" ] || [ ! -f "$main" ]; then
    warn "no main Caddyfile found${main:+ at $main}; set LIOTEK_CADDYFILE or create it (see ctx.md)"
    return
  fi
  if ! grep -qF "$CADDY_SITES" "$main"; then
    warn "$main does not import $CADDY_SITES/*.caddy, so Caddy will not serve $SITE_HOST"
    return
  fi
  local output
  if output=$(caddy reload --config "$main" --adapter caddyfile 2>&1); then
    echo "Reloaded Caddy: http://$SITE_HOST"
  else
    warn "could not reload Caddy (is it running? \`brew services start caddy\`):"
    printf '%s\n' "$output" >&2
  fi
}

echo "Checking $NEW_BINARY against $CONFIG"
"$NEW_BINARY" doctor --config "$CONFIG" || fail "doctor found a problem; nothing was changed"

BACKUP_NAME=""
back_up_before_upgrade

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
  rm -f "$FAILED"
  echo "Installed: $("$BINARY" --version)"
  echo "Logs: $LOG_DIR/$NAME.log"
  install_caddy_site
  exit 0
fi

echo "install: the new binary did not become healthy; it is kept as $FAILED" >&2
[ -f "$PREVIOUS" ] || fail "there is no previous binary to go back to; see $LOG_DIR/$NAME.log"
mv -f "$BINARY" "$FAILED"
mv -f "$PREVIOUS" "$BINARY"
launchctl kickstart -k "$DOMAIN/$LABEL"
if wait_until_healthy; then
  fail "went back to the previous binary, which is running again; see $LOG_DIR/$NAME.log"
fi

[ -n "$BACKUP_NAME" ] || fail "the previous binary is not healthy either, and there is no backup to restore; see $LOG_DIR/$NAME.log"
echo "install: the previous binary cannot start on the upgraded database; restoring $BACKUP_NAME" >&2
restore_pre_upgrade_backup
if wait_until_healthy; then
  fail "restored $BACKUP_NAME and the previous binary is running again; the upgraded database is under set-aside/ in the data directory"
fi
fail "restored $BACKUP_NAME, but the previous binary is still not healthy; see $LOG_DIR/$NAME.log"
