#!/usr/bin/env bash
# Launch Compose VST standalone (Rust audio + Compose UI)
# Usage:
#   ./run.sh                              # default 440Hz sine
#   ./run.sh --tone noise                 # white noise
#   ./run.sh --tone sweep                 # frequency sweep
#   ./run.sh --wav sample.wav             # loop a WAV file
#   ./run.sh --wav sample.wav --freq 440  # (--freq ignored with --wav)
# Press Ctrl+C or close the terminal to shut everything down.

set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUST_ARGS=("$@")

cleanup() {
    echo ""
    echo "Shutting down..."
    [[ -n "$RUST_PID" ]] && kill "$RUST_PID" 2>/dev/null
    [[ -n "$UI_PID" ]] && kill "$UI_PID" 2>/dev/null
    wait 2>/dev/null
    echo "Done."
}
trap cleanup EXIT INT TERM

# Build and launch Rust standalone
echo "▶ Starting Rust audio engine..."
cd "$SCRIPT_DIR/plugin"
cargo run --features standalone --bin compose-vst-standalone -- "${RUST_ARGS[@]}" &
RUST_PID=$!

# Give the Rust IPC server a moment to bind
sleep 2

# Build and launch Compose UI
echo "▶ Starting Compose UI..."
cd "$SCRIPT_DIR/ui"
if [ ! -f gradlew ]; then
    echo "  (generating Gradle wrapper...)"
    gradle wrapper
fi
./gradlew run &
UI_PID=$!

echo ""
echo "✅ Both running. Press Ctrl+C to stop."
echo ""

# Wait for either process to exit
wait -n "$RUST_PID" "$UI_PID" 2>/dev/null
