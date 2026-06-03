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

**Plugin (Rust)** — loaded by the DAW as a native `.vst3` shared library. Handles real-time audio processing with zero GC pauses. Ships a minimal in-host **egui editor** that doubles as a bridge: it owns the host `ParamSetter`, so edits coming from the Compose UI are applied as real host parameter gestures (recorded as automation, saved with the project).

**UI (Compose Desktop)** — separate JVM process. Communicates with the plugin over TCP with newline-delimited JSON. Optional — the in-host editor works on its own.

## Automation & tempo

- `cutoff`, `resonance`, and `sweep` are real plugin parameters, so the host can automate them directly.
- Edits from the Compose UI travel over IPC and are replayed through the host parameter system **on the GUI thread by the in-host editor**, which is the only place nih-plug allows parameter writes. Keep the in-host editor window open for Compose edits to be recorded/persisted.
- The plugin reads the host transport every block: the current **BPM** is shown in both UIs, and the `sweep` parameter drives a **bar-synced** sine sweep of the cutoff (±2 octaves at full depth), phase-locked to the host timeline so it lines up on every replay.

## Building

### Prerequisites

- Rust (stable) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- JDK 21+ — for Compose Desktop
- Gradle wrapper is included in `ui/`

### Rust Plugin (release `.vst3` / `.clap`)

A loadable VST3/CLAP is a *bundle* (a structured directory with an `Info.plist`,
the right per-platform layout, and a code signature), not the raw `.dylib`/`.dll`
that `cargo build` emits. Use the bundled [`cargo xtask`](https://github.com/robbert-vdh/nih-plug/tree/master/nih_plug_xtask)
command — run it from the **repo root** (it's a Cargo workspace):

```bash
cargo xtask bundle compose-vst-plugin --release
```

Output (named via `bundler.toml`):

```
target/bundled/Compose VST.vst3
target/bundled/Compose VST.clap
```

On macOS, add `--universal` to produce a fat arm64 + x86_64 binary:

```bash
cargo xtask bundle compose-vst-plugin --release --universal
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
cp -R "target/bundled/Compose VST.vst3" ~/Library/Audio/Plug-Ins/VST3/
```

> Ableton Live does not load CLAP — use the VST3 there. In **Live → Settings →
> Plug-Ins**, enable *Use VST3 Plug-In System Folders* (or add a custom folder),
> then click **Rescan**.

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

**Plugin → UI (state updates, ~30Hz):**
```json
{"type":"state","cutoff":1000.0,"resonance":0.5,"sweep":0.0,"bpm":120.0}
```
(`sweep`/`bpm` are omitted by the standalone harness, which has no host transport.)

**UI → Plugin (parameter changes):**
```json
{"type":"set_param","name":"cutoff","value":2000.0}
{"type":"set_param","name":"resonance","value":0.7}
{"type":"set_param","name":"sweep","value":0.5}
```

**UI → Plugin (automation gestures — wrap a slider drag so the host records one gesture):**
```json
{"type":"gesture","name":"cutoff","action":"begin"}
{"type":"gesture","name":"cutoff","action":"end"}
```

## Project Structure

```
compose-vst/
├── plugin/                    # Rust VST3/CLAP plugin
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             # Plugin entry + DSP loop + transport/tempo read
│       ├── editor.rs          # Minimal in-host egui UI + host-param bridge
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

skyhook is available under the MIT license. See the LICENSE file for more info.

\ ゜o゜)ノ
