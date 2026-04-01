# compose-vst

A VST3/CLAP audio plugin with a **Rust** backend (nih-plug) and a **Compose Multiplatform** desktop UI.

## Architecture

```
┌─────────────────┐     TCP/JSON      ┌──────────────────────┐
│  DAW (host)      │    localhost:9847  │  Compose Desktop UI  │
│                  │◄─────────────────►│                      │
│  ┌──────────────┐│                   │  - Cutoff slider     │
│  │ Rust Plugin  ││   state updates   │  - Resonance slider  │
│  │ (nih-plug)   ││ ────────────────► │  - Freq response plot│
│  │              ││   param changes   │                      │
│  │ Biquad LPF   ││ ◄──────────────── │  Material 3 dark UI  │
│  └──────────────┘│                   │                      │
└─────────────────┘                   └──────────────────────┘
```

**Plugin (Rust)** — loaded by the DAW as a native `.vst3` shared library. Handles real-time audio processing with zero GC pauses.

**UI (Compose Desktop)** — separate JVM process. Communicates with the plugin over TCP with newline-delimited JSON.

## Building

### Prerequisites

- Rust (stable) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- JDK 21+ — for Compose Desktop
- Gradle wrapper is included in `ui/`

### Rust Plugin

```bash
cd plugin
cargo build --release
```

The compiled `.vst3` bundle will be in `target/release/`. Copy it to your DAW's VST3 folder.

### Compose Desktop UI

```bash
cd ui
./gradlew run          # Run directly
./gradlew packageDeb   # Package as .deb
```

## IPC Protocol

Newline-delimited JSON over TCP on `localhost:9847`.

**Plugin → UI (state updates, ~30Hz):**
```json
{"type":"state","cutoff":1000.0,"resonance":0.5}
```

**UI → Plugin (parameter changes):**
```json
{"type":"set_param","name":"cutoff","value":2000.0}
{"type":"set_param","name":"resonance","value":0.7}
```

## Project Structure

```
compose-vst/
├── plugin/                    # Rust VST3/CLAP plugin
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             # Plugin entry + DSP loop
│       ├── filter.rs          # Biquad low-pass filter
│       └── ipc.rs             # TCP server for UI communication
└── ui/                        # Compose Desktop app
    ├── build.gradle.kts
    ├── settings.gradle.kts
    └── src/main/kotlin/com/composevst/
        ├── Main.kt            # App entry + main Compose UI
        ├── IpcClient.kt       # TCP client with auto-reconnect
        └── components/
            ├── ParamSlider.kt         # Logarithmic/linear slider
            └── FrequencyResponse.kt   # Biquad magnitude response plot
```

## License

MIT
