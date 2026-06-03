pub mod editor;
pub mod granular;
mod ipc;
pub mod ipc_standalone;

use crossbeam_channel::{bounded, Receiver, Sender};
use granular::{GrainParams, GranularEngine};
use ipc::IpcServer;
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Capture buffer length in seconds.
const BUFFER_SECONDS: f32 = 4.0;

/// Note divisions for tempo-synced grain size.
#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Division {
    #[id = "1_1"]
    #[name = "1/1"]
    Whole,
    #[id = "1_2"]
    #[name = "1/2"]
    Half,
    #[id = "1_4"]
    #[name = "1/4"]
    Quarter,
    #[id = "1_4t"]
    #[name = "1/4T"]
    QuarterTriplet,
    #[id = "1_8"]
    #[name = "1/8"]
    Eighth,
    #[id = "1_8t"]
    #[name = "1/8T"]
    EighthTriplet,
    #[id = "1_16"]
    #[name = "1/16"]
    Sixteenth,
    #[id = "1_32"]
    #[name = "1/32"]
    ThirtySecond,
}

impl Division {
    /// Length of this division in quarter-note beats.
    fn beats(self) -> f64 {
        match self {
            Division::Whole => 4.0,
            Division::Half => 2.0,
            Division::Quarter => 1.0,
            Division::QuarterTriplet => 2.0 / 3.0,
            Division::Eighth => 0.5,
            Division::EighthTriplet => 1.0 / 3.0,
            Division::Sixteenth => 0.25,
            Division::ThirtySecond => 0.125,
        }
    }

    /// Stable index used for the IPC protocol (kept in sync with `from_idx`).
    pub fn index(self) -> i32 {
        match self {
            Division::Whole => 0,
            Division::Half => 1,
            Division::Quarter => 2,
            Division::QuarterTriplet => 3,
            Division::Eighth => 4,
            Division::EighthTriplet => 5,
            Division::Sixteenth => 6,
            Division::ThirtySecond => 7,
        }
    }

    pub fn from_idx(i: i32) -> Division {
        match i {
            0 => Division::Whole,
            1 => Division::Half,
            2 => Division::Quarter,
            3 => Division::QuarterTriplet,
            4 => Division::Eighth,
            5 => Division::EighthTriplet,
            6 => Division::Sixteenth,
            _ => Division::ThirtySecond,
        }
    }
}

/// Identifies a parameter for edits coming in over IPC. The `as usize`
/// discriminant indexes the editor's gesture-tracking array.
#[derive(Clone, Copy)]
pub enum ParamId {
    Density = 0,
    Size = 1,
    Position = 2,
    Spray = 3,
    Pitch = 4,
    PitchSpread = 5,
    PanSpread = 6,
    Feedback = 7,
    Mix = 8,
    Sync = 9,
    Reverse = 10,
    Division = 11,
}

pub const NUM_PARAMS: usize = 12;

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
    /// Active grain count, for the activity meter.
    pub active_grains: AtomicU32,
    /// Whether an external Compose UI is currently connected.
    pub ui_connected: AtomicBool,
    /// Queue of parameter edits from the Compose UI, drained on the GUI thread.
    pub edit_tx: Sender<ParamEdit>,
}

struct CodecPlugin {
    params: Arc<CodecParams>,
    engine: Option<GranularEngine>,
    sample_rate: f32,
    ipc: Option<IpcServer>,
    shared: Arc<Shared>,
    edit_rx: Receiver<ParamEdit>,
    samples_since_last_send: u32,
}

#[derive(Params)]
struct CodecParams {
    #[id = "density"]
    density: FloatParam,
    #[id = "size"]
    size: FloatParam,
    #[id = "position"]
    position: FloatParam,
    #[id = "spray"]
    spray: FloatParam,
    #[id = "pitch"]
    pitch: FloatParam,
    #[id = "pitchspr"]
    pitch_spread: FloatParam,
    #[id = "panspr"]
    pan_spread: FloatParam,
    #[id = "feedback"]
    feedback: FloatParam,
    #[id = "mix"]
    mix: FloatParam,
    #[id = "sync"]
    sync: BoolParam,
    #[id = "reverse"]
    reverse: BoolParam,
    #[id = "division"]
    division: EnumParam<Division>,

    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
}

impl Default for CodecPlugin {
    fn default() -> Self {
        let (edit_tx, edit_rx) = bounded::<ParamEdit>(1024);
        Self {
            params: Arc::new(CodecParams::default()),
            engine: None,
            sample_rate: 44100.0,
            ipc: None,
            shared: Arc::new(Shared {
                bpm: AtomicU32::new(0),
                active_grains: AtomicU32::new(0),
                ui_connected: AtomicBool::new(false),
                edit_tx,
            }),
            edit_rx,
            samples_since_last_send: 0,
        }
    }
}

impl Default for CodecParams {
    fn default() -> Self {
        Self {
            density: FloatParam::new(
                "Density",
                25.0,
                FloatRange::Skewed {
                    min: 0.5,
                    max: 150.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" /s")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            size: FloatParam::new(
                "Grain Size",
                80.0,
                FloatRange::Skewed {
                    min: 5.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            position: FloatParam::new("Position", 0.1, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            spray: FloatParam::new("Spray", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            pitch: FloatParam::new("Pitch", 0.0, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" st")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),

            pitch_spread: FloatParam::new(
                "Pitch Spread",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            pan_spread: FloatParam::new(
                "Pan Spread",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            feedback: FloatParam::new(
                "Feedback",
                0.0,
                FloatRange::Linear { min: 0.0, max: 0.95 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            sync: BoolParam::new("Sync", false),
            reverse: BoolParam::new("Reverse", false),
            division: EnumParam::new("Division", Division::Eighth),

            editor_state: EguiState::from_size(440, 380),
        }
    }
}

impl Plugin for CodecPlugin {
    const NAME: &'static str = "Codec Granular";
    const VENDOR: &'static str = "Commit451";
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
        self.sample_rate = buffer_config.sample_rate;
        self.engine = Some(GranularEngine::new(self.sample_rate, BUFFER_SECONDS));

        if self.ipc.is_none() {
            self.ipc = Some(IpcServer::start(self.shared.clone()));
        }

        true
    }

    fn reset(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let transport = context.transport();
        let bpm = transport.tempo.unwrap_or(0.0) as f32;
        self.shared.bpm.store(bpm.to_bits(), Ordering::Relaxed);

        // Block-rate parameter snapshot.
        let size_ms = self.params.size.smoothed.next();
        let size_samples = if self.params.sync.value() {
            if let Some(tempo) = transport.tempo {
                (self.params.division.value().beats() * 60.0 / tempo) as f32 * self.sample_rate
            } else {
                size_ms * 0.001 * self.sample_rate
            }
        } else {
            size_ms * 0.001 * self.sample_rate
        };
        let min_grain = self.sample_rate * 0.001;
        let max_grain = self.sample_rate * 1.0;

        let gp = GrainParams {
            density: self.params.density.smoothed.next(),
            size_samples: size_samples.clamp(min_grain, max_grain),
            position: self.params.position.smoothed.next(),
            spray: self.params.spray.smoothed.next(),
            pitch_semitones: self.params.pitch.smoothed.next(),
            pitch_spread: self.params.pitch_spread.smoothed.next(),
            pan_spread: self.params.pan_spread.smoothed.next(),
            feedback: self.params.feedback.smoothed.next(),
            mix: self.params.mix.smoothed.next(),
            reverse: self.params.reverse.value(),
        };

        let mut peak = 0.0_f32;
        if let Some(engine) = &mut self.engine {
            engine.set_params(gp);

            let out = buffer.as_slice();
            let stereo = out.len() > 1;
            let num_samples = out[0].len();
            for i in 0..num_samples {
                let in_l = out[0][i];
                let in_r = if stereo { out[1][i] } else { in_l };
                let (ol, or) = engine.process_sample(in_l, in_r);
                out[0][i] = ol;
                if stereo {
                    out[1][i] = or;
                }
                peak = peak.max(ol.abs()).max(or.abs());
            }

            self.shared
                .active_grains
                .store(engine.active_grains() as u32, Ordering::Relaxed);
        }

        // Send state to UI periodically (~30 Hz at 44100 sample rate).
        self.samples_since_last_send += buffer.samples() as u32;
        if self.samples_since_last_send >= 1470 {
            self.samples_since_last_send = 0;
            if let Some(ref ipc) = self.ipc {
                ipc.send_state(&self.params, bpm, peak.min(1.0));
            }
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        self.ipc = None;
    }
}

impl ClapPlugin for CodecPlugin {
    const CLAP_ID: &'static str = "com.commit451.codec";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Granular cloud FX with Compose Desktop UI");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Granular,
    ];
}

impl Vst3Plugin for CodecPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"CodecGranularv01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Generator];
}

nih_export_clap!(CodecPlugin);
nih_export_vst3!(CodecPlugin);
