#!/bin/bash

set -euo pipefail

NAME=liotek
LABEL="com.$(id -un).$NAME"
DOMAIN="gui/$(id -u)"

launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
rm -f "$HOME/Library/LaunchAgents/$LABEL.plist" "$HOME/.local/bin/$NAME" "$HOME/.local/bin/$NAME.previous"
echo "Removed the $LABEL agent. Left in place: ~/.$NAME (config and data), ~/Library/Logs/$NAME"
