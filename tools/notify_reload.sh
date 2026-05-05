#!/usr/bin/env bash
# Notify the lazytime daemon to reload project rules via UNIX socket.
# Usage: notify_reload.sh [SOCKET_PATH]
set -euo pipefail
SOCKET=${1:-"$HOME/.local/run/lazytime.sock"}
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
MSG="{\"type\":\"projects_updated\",\"timestamp\":\"${TIMESTAMP}\"}\n"

if [ ! -S "$SOCKET" ]; then
  echo "Socket not found: $SOCKET" >&2
  exit 2
fi

printf "%s" "$MSG" | socat - UNIX-CONNECT:"$SOCKET"
echo "sent projects_updated to $SOCKET"
