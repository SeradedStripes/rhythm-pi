#!/usr/bin/env sh
# Run both server and client for local development.
# Starts server in background, waits until it's ready, then launches client GUI.

set -eu

PORT=${RHYTHM_PORT:-8080}
BASE_URL="http://127.0.0.1:${PORT}"
LOGFILE="./server-dev.log"

echo "Starting Rhythm PI server (logs -> ${LOGFILE})..."
# run server in background and capture its PID
cargo run -p rhythm-pi-server --bin rhythm-pi-server > "$LOGFILE" 2>&1 &
SERVER_PID=$!

cleanup() {
  echo "Shutting down..."
  if kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

# Wait for server to respond on /api/songs
echo "Waiting for server to become ready at ${BASE_URL}/api/songs..."
COUNT=0
until curl -sSf "$BASE_URL/api/songs" > /dev/null 2>&1; do
  COUNT=$((COUNT + 1))
  tail -n 200 "$LOGFILE" >&2 || true
  if [ "$COUNT" -gt 100 ]; then
    echo "Server failed to start. Showing ${LOGFILE} last 200 lines:" >&2
    tail -n 200 "$LOGFILE" >&2 || true
    exit 1
  fi
  sleep 0.5
done

echo "Server is up. Launching client GUI..."
# Run client in foreground so user can interact; when it exits, cleanup trap runs
cargo run -p rhythm-pi-client

# client exited; cleanup will run via trap
exit 0
