#!/usr/bin/env bash

set -e

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

RUST_PID=""

cleanup() {
    echo
    echo "Stopping Orion development services..."

    if [[ -n "${RUST_PID:-}" ]]; then
        kill "$RUST_PID" 2>/dev/null || true
        wait "$RUST_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT INT TERM

echo "Orion root: $PROJECT_ROOT"
echo

# ─────────────────────────────────────────────
# Rust media server
# ─────────────────────────────────────────────

echo "Starting Rust media server..."

cargo run \
    --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml" \
    &

RUST_PID=$!

echo "Rust media server PID: $RUST_PID"
echo "Waiting for media server on 127.0.0.1:8787..."

# Give Cargo/Rust plenty of time to compile on the first run.
for i in {1..300}; do

    # The server is ready when its HTTP endpoint responds.
    if curl -s \
        --connect-timeout 0.2 \
        --max-time 0.5 \
        http://127.0.0.1:8787/api/library/count \
        >/dev/null 2>&1; then

        echo "Media server is ready."
        break
    fi

    # If Cargo/the Rust process has actually died, stop immediately.
    if ! kill -0 "$RUST_PID" 2>/dev/null; then
        echo
        echo "ERROR: Rust media server stopped unexpectedly."
        echo "Check the Rust error above."
        exit 1
    fi

    sleep 0.1

    if [[ "$i" -eq 300 ]]; then
        echo
        echo "ERROR: Media server did not start on port 8787."
        echo "Rust is still running, but the HTTP server never became reachable."
        exit 1
    fi

done

echo
echo "Starting Vite..."

npm exec -- vite \
    --host 0.0.0.0 \
    --port 1420