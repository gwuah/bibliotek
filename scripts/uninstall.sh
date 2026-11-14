#!/bin/bash

set -euo pipefail

NAME=liotek
LABEL="${LIOTEK_LABEL:-com.$(id -un).$NAME}"
DOMAIN="gui/$(id -u)"
SITE_HOST="${LIOTEK_HOSTNAME:-$NAME.test}"
CADDY_SITE="${LIOTEK_CADDY_SITES:-$HOME/.config/caddy/sites}/$SITE_HOST.caddy"

caddyfile() {
  if [ -n "${LIOTEK_CADDYFILE:-}" ]; then
    echo "$LIOTEK_CADDYFILE"
  elif command -v brew >/dev/null; then
    echo "$(brew --prefix)/etc/Caddyfile"
  fi
}

remove_caddy_site() {
  [ -f "$CADDY_SITE" ] || return 0
  rm -f "$CADDY_SITE"
  echo "Removed $CADDY_SITE"
  local main
  main=$(caddyfile)
  if command -v caddy >/dev/null && [ -n "$main" ] && [ -f "$main" ]; then
    caddy reload --config "$main" --adapter caddyfile >/dev/null 2>&1 \
      || echo "uninstall: warning: could not reload Caddy; it serves $SITE_HOST until restarted" >&2
  fi
}

launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
rm -f "$HOME/Library/LaunchAgents/$LABEL.plist" "$HOME/.local/bin/$NAME" "$HOME/.local/bin/$NAME.previous"
remove_caddy_site
echo "Removed the $LABEL agent. Left in place: ~/.$NAME (config and data), ~/Library/Logs/$NAME"
