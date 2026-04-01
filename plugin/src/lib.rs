pub mod filter;
mod ipc;
pub mod ipc_standalone;

use filter::BiquadLpf;
use ipc::IpcServer;
use nih_plug::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Atomic storage for IPC-driven parameter overrides.
/// Uses separate dirty flags to avoid sentinel value collisions with valid f32 bits.
pub struct IpcParamOverrides {
    cutoff_bits: AtomicU32,
    cutoff_dirty: AtomicBool,
    resonance_bits: AtomicU32,
    resonance_dirty: AtomicBool,
}

impl IpcParamOverrides {
    fn new() -> Self {
        Self {
            cutoff_bits: AtomicU32::new(1000.0_f32.to_bits()),
            cutoff_dirty: AtomicBool::new(false),
            resonance_bits: AtomicU32::new(0.0_f32.to_bits()),
            resonance_dirty: AtomicBool::new(false),
        }
    }

    pub fn set_cutoff(&self, value: f32) {
        self.cutoff_bits.store(value.to_bits(), Ordering::Relaxed);
        self.cutoff_dirty.store(true, Ordering::Release);
    }

    pub fn set_resonance(&self, value: f32) {
        self.resonance_bits.store(value.to_bits(), Ordering::Relaxed);
        self.resonance_dirty.store(true, Ordering::Release);
    }

    fn take_cutoff(&self) -> Option<f32> {
        if self.cutoff_dirty.swap(false, Ordering::Acquire) {
            Some(f32::from_bits(self.cutoff_bits.load(Ordering::Relaxed)))
        } else {
            None
        }
    }

    fn take_resonance(&self) -> Option<f32> {
        if self.resonance_dirty.swap(false, Ordering::Acquire) {
            Some(f32::from_bits(self.resonance_bits.load(Ordering::Relaxed)))
        } else {
            None
        }
    }
}

struct ComposeVstPlugin {
    params: Arc<ComposeVstParams>,
    filters: Vec<BiquadLpf>,
    ipc: Option<IpcServer>,
    ipc_overrides: Arc<IpcParamOverrides>,
    /// Current effective values (from DAW params or IPC overrides)
    effective_cutoff: f32,
    effective_resonance: f32,
    /// Throttle state sends to ~30 Hz
    samples_since_last_send: u32,
}

#[derive(Params)]
struct ComposeVstParams {
    #[id = "cutoff"]
    cutoff: FloatParam,
    #[id = "resonance"]
    resonance: FloatParam,
}

impl Default for ComposeVstPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(ComposeVstParams::default()),
            filters: Vec::new(),
            ipc: None,
            ipc_overrides: Arc::new(IpcParamOverrides::new()),
            effective_cutoff: 1000.0,
            effective_resonance: 0.0,
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
                FloatRange::Linear {
                    min: 0.0,
                    max: 1.0,
                },
            )
            .with_unit("")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
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

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let sr = buffer_config.sample_rate;
        self.filters = vec![BiquadLpf::new(sr); 2]; // stereo

        // Start IPC server with shared overrides
        if self.ipc.is_none() {
            self.ipc = Some(IpcServer::start(self.ipc_overrides.clone()));
        }

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Read DAW params as baseline
        let mut cutoff = self.params.cutoff.smoothed.next();
        let mut resonance = self.params.resonance.smoothed.next();

        // Apply IPC overrides if present (UI takes priority)
        if let Some(c) = self.ipc_overrides.take_cutoff() {
            cutoff = c;
        }
        if let Some(r) = self.ipc_overrides.take_resonance() {
            resonance = r;
        }

        self.effective_cutoff = cutoff;
        self.effective_resonance = resonance;

        // Update filter coefficients (dirty-checked internally — skips trig if unchanged)
        for f in &mut self.filters {
            f.set_params(cutoff, resonance);
        }

        // Process audio
        for mut frame in buffer.iter_samples() {
            for (ch, sample) in frame.iter_mut().enumerate() {
                if ch < self.filters.len() {
                    *sample = self.filters[ch].process(*sample);
                }
            }
        }

        // Send state to UI periodically (~30 Hz at 44100 sample rate)
        self.samples_since_last_send += buffer.samples() as u32;
        if self.samples_since_last_send >= 1470 {
            self.samples_since_last_send = 0;
            if let Some(ref ipc) = self.ipc {
                ipc.send_state(self.effective_cutoff, self.effective_resonance);
            }
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        self.ipc = None;
    }
}

impl ClapPlugin for ComposeVstPlugin {
    const CLAP_ID: &'static str = "com.compose-vst.lowpass";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Low-pass filter with Compose Desktop UI");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Filter,
    ];
}

impl Vst3Plugin for ComposeVstPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"ComposeVSTv0001\0";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Filter,
    ];
}

nih_export_clap!(ComposeVstPlugin);
nih_export_vst3!(ComposeVstPlugin);
