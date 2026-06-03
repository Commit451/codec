# Codec

A **granular cloud** audio FX plugin (VST3/CLAP) with a **Rust** backend (nih-plug)
and a **Compose Multiplatform** desktop UI. Incoming audio is captured into a rolling
buffer and resprayed as a cloud of overlapping grains you sculpt with knobs.

## Architecture

```
┌─────────────────┐     TCP/JSON      ┌──────────────────────┐
│  DAW (host)      │    localhost:9847  │  Compose Desktop UI  │
│                  │◄─────────────────►│                      │
│  ┌──────────────┐│   state + meters  │  Density / Size /    │
│  │ Rust Plugin  ││ ────────────────► │  Position / Pitch /  │
│  │ (nih-plug)   ││                   │  Spray / Feedback …  │
│  │              ││   param changes   │  knobs + buttons     │
│  │ Granular     ││ ◄──────────────── │                      │
│  │ engine       ││                   │  Material 3 dark UI  │
│  └──────────────┘│                   │                      │
└─────────────────┘                   └──────────────────────┘
```

**Plugin (Rust)** — loaded by the DAW as a native VST3/CLAP. A real-time-safe granular
engine (fixed grain pool + pre-allocated buffer, no allocation in the audio thread).
Ships a minimal in-host **egui editor** that doubles as a bridge: it owns the host
`ParamSetter`, so edits coming from the Compose UI are applied as real host parameter
gestures (recorded as automation, saved with the project).

**UI (Compose Desktop)** — separate JVM process with rotary knobs and toggle buttons.
Talks to the plugin over TCP with newline-delimited JSON. Optional — the in-host editor
works on its own.

## Parameters

| Knob | Range | What it does |
|------|-------|--------------|
| Density | 0.5–150 /s | Grains spawned per second (overlap). |
| Grain Size | 5–500 ms | Length of each grain. |
| Position | 0–1 | How far back in the buffer grains read. |
| Spray | 0–1 | Random spread of the read position. |
| Pitch | ±24 st | Per-grain transpose. |
| Pitch Spread | 0–1 | Random per-grain pitch variation. |
| Pan Spread | 0–1 | Stereo spread of grains. |
| Feedback | 0–0.95 | Wet signal fed back into the buffer (granular-delay tails). |
| Mix | 0–1 | Dry/wet blend. |

Buttons: **Sync** (lock grain size to the host tempo + **Division** selector),
**Reverse** (grains play backwards). The current **BPM**, output **level**, and active
**grain count** are shown live.

## Automation & tempo

- Every knob/button is a real plugin parameter, so the host can automate them directly.
- Edits from the Compose UI travel over IPC and are replayed through the host parameter
  system **on the GUI thread by the in-host editor** — the only place nih-plug allows
  parameter writes. Keep the in-host editor window open for Compose edits to be
  recorded/persisted.
- The plugin reads the host transport every block; with **Sync** on, grain size locks to
  the selected note division (phase-independent, derived from the host tempo).

## Building

### Prerequisites

- Rust (stable) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- JDK 21+ — for Compose Desktop
- Gradle wrapper is included in `ui/`

### Rust plugin (release `.vst3` / `.clap`)

A loadable VST3/CLAP is a *bundle* (a structured directory with an `Info.plist`, the
right per-platform layout, and a code signature), not the raw `.dylib`/`.dll` that
`cargo build` emits. Use the bundled [`cargo xtask`](https://github.com/robbert-vdh/nih-plug/tree/master/nih_plug_xtask)
command — run it from the **repo root** (it's a Cargo workspace):

```bash
cargo xtask bundle codec --release
```

Output (named via `bundler.toml`):

```
target/bundled/Codec.vst3
target/bundled/Codec.clap
```

On macOS, add `--universal` to produce a fat arm64 + x86_64 binary:

```bash
cargo xtask bundle codec --release --universal
```

`cd plugin && cargo build` still works for a quick compile check, but it does **not**
produce a loadable plugin — always use `cargo xtask bundle` for that.

### Installing locally (for testing)

Copy the bundle into the standard plugin folder for your OS, then rescan in your DAW:

| OS      | VST3                                                 | CLAP                                         |
|---------|------------------------------------------------------|----------------------------------------------|
| macOS   | `~/Library/Audio/Plug-Ins/VST3/` (or `/Library/...`) | `~/Library/Audio/Plug-Ins/CLAP/`             |
| Windows | `C:\Program Files\Common Files\VST3\`                | `C:\Program Files\Common Files\CLAP\`        |
| Linux   | `~/.vst3/`                                            | `~/.clap/`                                    |

```bash
# macOS example
cp -R "target/bundled/Codec.vst3" ~/Library/Audio/Plug-Ins/VST3/
```

> Ableton Live does not load CLAP — use the VST3 there. In **Live → Settings →
> Plug-Ins**, enable *Use VST3 Plug-In System Folders* (or add a custom folder),
> then click **Rescan**. Codec is an audio effect — drop it on an audio track.

### Quick test without a DAW

```bash
./run.sh                 # loop the bundled loop.wav through the granular engine
./run.sh --tone sine     # granulate a test tone
```

This launches the standalone Rust audio engine plus the Compose UI; the knobs drive the
engine over IPC (no host tempo, so Sync/Division are inert here).

### Compose Desktop UI

```bash
cd ui
./gradlew run          # Run directly
./gradlew packageDeb   # Package the UI app as .deb (also: packageDmg / packageMsi)
```

## Distribution & installers

`cargo xtask bundle` gives you the raw bundles; for end users you'll want a real
installer that places them in the system plugin folders (and can be signed). The
**standard, well-supported** tools — which is what this repo uses — are a macOS
**`.pkg`** (`pkgbuild`/`productbuild`) and a Windows **Inno Setup** installer:

```bash
# macOS — builds dist/codec-<version>.pkg (installs the VST3 + CLAP system-wide)
./packaging/macos/build-pkg.sh

# Windows — open packaging/windows/installer.iss in Inno Setup (or: iscc installer.iss)
# after building the bundle on a Windows machine. Produces dist/codec-<version>-setup.exe
```

See [`packaging/README.md`](packaging/README.md) for signing/notarization notes and
how the scripts are wired up.

> **Why not a Compose Multiplatform installer GUI?** Compose's `packageDmg`/`packageMsi`
> tasks (jpackage) are designed to ship *the Compose app itself*, not to install a
> system-wide VST3 — doing the latter from a JVM app means re-implementing privilege
> elevation and plugin-folder placement that `.pkg`/Inno Setup already handle natively.
> So we keep the **plugin** installer standard, and use Compose's packaging only if you
> want to ship the optional UI app as a standalone download.

## IPC Protocol

Newline-delimited JSON over TCP on `localhost:9847`.

**Plugin → UI (state + meters, ~30Hz):**
```json
{"type":"state","density":25.0,"size":80.0,"position":0.1,"spray":0.0,
 "pitch":0.0,"pitch_spread":0.0,"pan_spread":0.5,"feedback":0.0,"mix":1.0,
 "sync":0,"reverse":0,"division":4,"bpm":120.0,"level":0.3,"grains":18}
```

**UI → Plugin (parameter changes — plain values; toggles use 0/1):**
```json
{"type":"set_param","name":"density","value":40.0}
{"type":"set_param","name":"pitch","value":-12.0}
{"type":"set_param","name":"reverse","value":1}
```

**UI → Plugin (automation gestures — wrap a knob drag so the host records one gesture):**
```json
{"type":"gesture","name":"density","action":"begin"}
{"type":"gesture","name":"density","action":"end"}
```

## Project Structure

```
codec/
├── Cargo.toml                # workspace (plugin + xtask)
├── bundler.toml              # bundle name mapping for cargo xtask
├── plugin/                   # Rust VST3/CLAP plugin (package: codec)
│   └── src/
│       ├── lib.rs            # Plugin entry, params, transport/tempo, process loop
│       ├── granular.rs       # Real-time granular engine (grain pool + scheduler)
│       ├── editor.rs         # Minimal in-host egui UI + host-param bridge
│       ├── ipc.rs            # TCP server for UI communication
│       ├── standalone.rs     # Standalone tester (cpal) — `codec-standalone`
│       └── ipc_standalone.rs # IPC server for standalone mode
├── xtask/                    # nih-plug bundler entry point
├── packaging/                # macOS .pkg + Windows Inno Setup installers
└── ui/                       # Compose Desktop app
    └── src/main/kotlin/com/composevst/
        ├── Main.kt           # Knob/button panel
        ├── IpcClient.kt      # TCP client with auto-reconnect
        └── components/
            ├── Knob.kt       # Rotary knob (drag + gesture)
            └── Controls.kt   # Toggle buttons, division selector, level meter
```

## License

Codec is available under the MIT license. See the LICENSE file for more info.

\ ゜o゜)ノ
