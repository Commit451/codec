pub mod editor;
pub mod filter;
mod ipc;
pub mod ipc_standalone;

use crossbeam_channel::{bounded, Receiver, Sender};
use filter::BiquadLpf;
use ipc::IpcServer;
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Full-depth tempo sweep spans this many octaves above/below the base cutoff.
const SWEEP_OCTAVES: f32 = 2.0;

/// Identifies an automatable parameter for edits coming in over IPC.
#[derive(Clone, Copy)]
pub enum ParamId {
    Cutoff = 0,
    Resonance = 1,
    SweepDepth = 2,
}

/// A host-parameter edit forwarded from the external Compose UI, applied on the
/// GUI thread by the editor so the host can record automation and persist it.
pub enum ParamEdit {
    Begin(ParamId),
    Set(ParamId, f32),
    End(ParamId),
}

/// State shared between the audio thread, the IPC server thread, and the editor.
pub struct Shared {
    /// Latest host tempo as `f32` bits; `0.0` means the host reported no tempo.
    pub bpm: AtomicU32,
    /// Whether an external Compose UI is currently connected.
    pub ui_connected: AtomicBool,
    /// Queue of parameter edits from the Compose UI, drained on the GUI thread.
    pub edit_tx: Sender<ParamEdit>,
}

struct ComposeVstPlugin {
    params: Arc<ComposeVstParams>,
    filters: Vec<BiquadLpf>,
    ipc: Option<IpcServer>,
    shared: Arc<Shared>,
    /// Receiver half of [`Shared::edit_tx`], handed to the editor when it spawns.
    edit_rx: Receiver<ParamEdit>,
    /// Throttle state sends to ~30 Hz.
    samples_since_last_send: u32,
}

#[derive(Params)]
struct ComposeVstParams {
    #[id = "cutoff"]
    cutoff: FloatParam,
    #[id = "resonance"]
    resonance: FloatParam,
    #[id = "sweep"]
    sweep_depth: FloatParam,

    /// Persisted in-host editor window size/open state.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
}

impl Default for ComposeVstPlugin {
    fn default() -> Self {
        let (edit_tx, edit_rx) = bounded::<ParamEdit>(1024);
        Self {
            params: Arc::new(ComposeVstParams::default()),
            filters: Vec::new(),
            ipc: None,
            shared: Arc::new(Shared {
                bpm: AtomicU32::new(0),
                ui_connected: AtomicBool::new(false),
                edit_tx,
            }),
            edit_rx,
            samples_since_last_send: 0,
        }
    }
}

impl Default for ComposeVstParams {
    fn default() -> Self {
        Self {
            cutoff: FloatParam::new(
                "Cutoff",
                1000.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),

            resonance: FloatParam::new(
                "Resonance",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            sweep_depth: FloatParam::new(
                "Tempo Sweep",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            editor_state: EguiState::from_size(320, 360),
        }
    }
}

impl Plugin for ComposeVstPlugin {
    const NAME: &'static str = "Compose VST";
    const VENDOR: &'static str = "compose-vst";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(
            self.params.clone(),
            self.shared.clone(),
            self.edit_rx.clone(),
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let sr = buffer_config.sample_rate;
        self.filters = vec![BiquadLpf::new(sr); 2]; // stereo

        // Start IPC server with shared state.
        if self.ipc.is_none() {
            self.ipc = Some(IpcServer::start(self.shared.clone()));
        }

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Publish the host tempo for the editor to display.
        let transport = context.transport();
        let bpm = transport.tempo.unwrap_or(0.0) as f32;
        self.shared.bpm.store(bpm.to_bits(), Ordering::Relaxed);

        // Parameters come straight from the host (automation, host UI, or the bridged
        // Compose/in-host editor edits) — block-rate, matching the existing design.
        let cutoff_base = self.params.cutoff.smoothed.next();
        let resonance = self.params.resonance.smoothed.next();
        let depth = self.params.sweep_depth.smoothed.next();

        // Optional bar-synced sine sweep, phase-locked to the host timeline.
        let cutoff = if depth > 0.0001 {
            let lfo = beat_sine(transport);
            cutoff_base * 2.0_f32.powf(lfo * depth * SWEEP_OCTAVES)
        } else {
            cutoff_base
        };

        // Update filter coefficients (dirty-checked internally — skips trig if unchanged).
        for f in &mut self.filters {
            f.set_params(cutoff, resonance);
        }

        // Process audio.
        for mut frame in buffer.iter_samples() {
            for (ch, sample) in frame.iter_mut().enumerate() {
                if ch < self.filters.len() {
                    *sample = self.filters[ch].process(*sample);
                }
            }
        }

        // Send state to UI periodically (~30 Hz at 44100 sample rate).
        self.samples_since_last_send += buffer.samples() as u32;
        if self.samples_since_last_send >= 1470 {
            self.samples_since_last_send = 0;
            if let Some(ref ipc) = self.ipc {
                ipc.send_state(cutoff_base, resonance, depth, bpm);
            }
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        self.ipc = None;
    }
}

/// Bar-synced sine LFO in `-1.0..=1.0`, phase-locked to the host timeline.
/// Returns `0.0` (centered) when the transport is stopped or reports no position.
fn beat_sine(t: &Transport) -> f32 {
    if !t.playing {
        return 0.0;
    }
    let beats = match t.pos_beats() {
        Some(b) => b,
        None => return 0.0,
    };
    let beats_per_bar = t.time_sig_numerator.unwrap_or(4).max(1) as f64;
    let phase = beats / beats_per_bar; // one full cycle per bar
    (phase * std::f64::consts::TAU).sin() as f32
}

impl ClapPlugin for ComposeVstPlugin {
    const CLAP_ID: &'static str = "com.compose-vst.lowpass";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Low-pass filter with Compose Desktop UI");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Filter];
}

impl Vst3Plugin for ComposeVstPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"ComposeVSTv0001\0";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_clap!(ComposeVstPlugin);
nih_export_vst3!(ComposeVstPlugin);
