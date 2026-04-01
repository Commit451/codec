//! Standalone test harness: generates a test tone, runs it through the biquad LPF,
//! and outputs to system audio. The Compose UI connects via TCP on localhost:9847
//! exactly as it would with the DAW-hosted plugin.
//!
//! Usage:
//!   cargo run --features standalone --bin compose-vst-standalone
//!   cargo run --features standalone --bin compose-vst-standalone -- --tone noise
//!   cargo run --features standalone --bin compose-vst-standalone -- --freq 440 --tone sine

#[cfg(not(feature = "standalone"))]
compile_error!("Build with --features standalone");

// Include shared modules directly (can't link against cdylib)
#[path = "filter.rs"]
mod filter;
#[path = "ipc_standalone.rs"]
mod ipc_standalone;

#[cfg(feature = "standalone")]
fn main() {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use filter::BiquadLpf;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;

    struct SharedState {
        cutoff_bits: AtomicU32,
        resonance_bits: AtomicU32,
        running: AtomicBool,
    }

    impl SharedState {
        fn new() -> Self {
            Self {
                cutoff_bits: AtomicU32::new(1000.0_f32.to_bits()),
                resonance_bits: AtomicU32::new(0.0_f32.to_bits()),
                running: AtomicBool::new(true),
            }
        }
        fn cutoff(&self) -> f32 {
            f32::from_bits(self.cutoff_bits.load(Ordering::Relaxed))
        }
        fn resonance(&self) -> f32 {
            f32::from_bits(self.resonance_bits.load(Ordering::Relaxed))
        }
        fn set_cutoff(&self, v: f32) {
            self.cutoff_bits.store(v.to_bits(), Ordering::Relaxed);
        }
        fn set_resonance(&self, v: f32) {
            self.resonance_bits.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    fn parse_arg_str(args: &[String], flag: &str) -> Option<String> {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
    }
    fn parse_arg_f32(args: &[String], flag: &str) -> Option<f32> {
        parse_arg_str(args, flag).and_then(|s| s.parse().ok())
    }

    let args: Vec<String> = std::env::args().collect();
    let tone_freq = parse_arg_f32(&args, "--freq").unwrap_or(440.0);
    let use_noise = matches!(
        parse_arg_str(&args, "--tone").as_deref(),
        Some("noise") | Some("white-noise")
    );
    let use_sweep = parse_arg_str(&args, "--tone").as_deref() == Some("sweep");

    println!("╔══════════════════════════════════════════╗");
    println!("║   Compose VST - Standalone Test Mode     ║");
    println!("╠══════════════════════════════════════════╣");
    if use_noise {
        println!("║  Tone: white noise                       ║");
    } else if use_sweep {
        println!("║  Tone: sweep 20Hz-20kHz                  ║");
    } else {
        println!("║  Tone: sine {:.0}Hz{}", tone_freq, " ".repeat(27 - format!("{:.0}", tone_freq).len()));
        println!("║                                          ║");
    }
    println!("║  IPC:  localhost:9847                    ║");
    println!("║  Press Ctrl+C to quit                    ║");
    println!("╚══════════════════════════════════════════╝");

    let state = Arc::new(SharedState::new());

    // Start IPC server
    let state_ipc = state.clone();
    let (ipc_tx, _ipc_handle) = ipc_standalone::start_standalone_ipc(move |name: &str, value: f32| {
        match name {
            "cutoff" => state_ipc.set_cutoff(value),
            "resonance" => state_ipc.set_resonance(value),
            _ => {}
        }
    });

    // Setup audio output
    let host = cpal::default_host();
    let device = host.default_output_device().expect("No output audio device found");
    println!("Audio device: {}", device.name().unwrap_or_default());

    let config = device.default_output_config().expect("No default output config");
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;
    println!("Sample rate: {}Hz, Channels: {}", sample_rate, channels);

    let state_audio = state.clone();
    let mut phase: f64 = 0.0;
    let mut sweep_freq: f64 = 20.0;
    let mut filters: Vec<BiquadLpf> = (0..channels).map(|_| BiquadLpf::new(sample_rate)).collect();
    let mut rng_state: u32 = 12345;
    let mut sample_count: u64 = 0;
    let send_interval = (sample_rate as u64 / 30).max(1);

    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let cutoff = state_audio.cutoff();
            let resonance = state_audio.resonance();

            for f in &mut filters {
                f.set_params(cutoff, resonance);
            }

            for frame in data.chunks_mut(channels) {
                let raw = if use_noise {
                    rng_state ^= rng_state << 13;
                    rng_state ^= rng_state >> 17;
                    rng_state ^= rng_state << 5;
                    (rng_state as f32 / u32::MAX as f32 * 2.0 - 1.0) * 0.3
                } else if use_sweep {
                    let s = (phase * 2.0 * std::f64::consts::PI).sin() as f32 * 0.3;
                    phase += sweep_freq / sample_rate as f64;
                    if phase >= 1.0 { phase -= 1.0; }
                    sweep_freq = 20.0 * (20000.0_f64 / 20.0).powf(
                        (sample_count as f64 % (sample_rate as f64 * 5.0)) / (sample_rate as f64 * 5.0)
                    );
                    s
                } else {
                    let s = (phase * 2.0 * std::f64::consts::PI).sin() as f32 * 0.3;
                    phase += tone_freq as f64 / sample_rate as f64;
                    if phase >= 1.0 { phase -= 1.0; }
                    s
                };

                let num_filters = filters.len();
                for (ch, sample) in frame.iter_mut().enumerate() {
                    *sample = filters[ch % num_filters].process(raw);
                }
                sample_count += 1;
            }

            // Send state to UI ~30 times/sec
            if sample_count % send_interval < (data.len() / channels) as u64 {
                let _ = ipc_tx.try_send((cutoff, resonance));
            }
        },
        |err| eprintln!("Audio stream error: {err}"),
        None,
    ).expect("Failed to build audio stream");

    stream.play().expect("Failed to start audio stream");
    println!("\n🎵 Audio streaming. Launch the Compose UI to connect.\n");

    // Wait for Ctrl+C
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::Relaxed);
    }).expect("Error setting Ctrl-C handler");

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("\nShutting down...");
    state.running.store(false, Ordering::Relaxed);
    drop(stream);
}
