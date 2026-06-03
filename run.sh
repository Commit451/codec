#!/usr/bin/env bash
# Launch Compose VST standalone (Rust audio + Compose UI)
# Usage:
#   ./run.sh                              # loop the bundled loop.wav (default)
#   ./run.sh --tone sine                  # 440Hz sine test tone
#   ./run.sh --tone sine --freq 880       # 880Hz sine test tone
#   ./run.sh --tone noise                 # white noise
#   ./run.sh --tone sweep                 # frequency sweep
#   ./run.sh --wav sample.wav             # loop a different WAV file
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

# Default source: loop the bundled loop.wav through the effect.
# Passing --tone / --freq / --wav overrides this and skips the loop.
source_specified=0
for arg in "$@"; do
    case "$arg" in
        --tone|--freq|--wav) source_specified=1; break ;;
    esac
done

if [[ "$source_specified" == "0" ]]; then
    LOOP_WAV="$SCRIPT_DIR/loop.wav"
    if [[ -f "$LOOP_WAV" ]]; then
        echo "🔁 Looping loop.wav (pass --tone sine for a test tone)"
        RUST_ARGS=(--wav "$LOOP_WAV" "${RUST_ARGS[@]}")
    else
        echo "⚠ loop.wav not found at $LOOP_WAV — using default test tone" >&2
    fi
fi

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

# Wait for either process to exit.
# (Note: `wait -n` needs bash 4.3+; macOS ships bash 3.2, so we poll instead.)
while kill -0 "$RUST_PID" 2>/dev/null && kill -0 "$UI_PID" 2>/dev/null; do
    sleep 1
done
