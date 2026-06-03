//! Standalone test harness: generates a test tone or loops a WAV file,
//! runs it through the granular engine, and outputs to system audio.
//! The Compose UI connects via TCP on localhost:9847.
//!
//! Usage:
//!   cargo run --features standalone --bin codec-standalone
//!   cargo run --features standalone --bin codec-standalone -- --tone noise
//!   cargo run --features standalone --bin codec-standalone -- --wav sample.wav

#[cfg(not(feature = "standalone"))]
compile_error!("Build with --features standalone");

#[path = "granular.rs"]
mod granular;
#[path = "ipc_standalone.rs"]
mod ipc_standalone;

#[cfg(feature = "standalone")]
fn main() {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use granular::{GrainParams, GranularEngine};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;

    /// Granular parameters shared from the IPC thread into the audio callback.
    struct SharedState {
        density: AtomicU32,
        size_ms: AtomicU32,
        position: AtomicU32,
        spray: AtomicU32,
        pitch: AtomicU32,
        pitch_spread: AtomicU32,
        pan_spread: AtomicU32,
        feedback: AtomicU32,
        mix: AtomicU32,
        reverse: AtomicBool,
        running: AtomicBool,
    }

    impl SharedState {
        fn new() -> Self {
            Self {
                density: AtomicU32::new(25.0_f32.to_bits()),
                size_ms: AtomicU32::new(80.0_f32.to_bits()),
                position: AtomicU32::new(0.1_f32.to_bits()),
                spray: AtomicU32::new(0.0_f32.to_bits()),
                pitch: AtomicU32::new(0.0_f32.to_bits()),
                pitch_spread: AtomicU32::new(0.0_f32.to_bits()),
                pan_spread: AtomicU32::new(0.5_f32.to_bits()),
                feedback: AtomicU32::new(0.0_f32.to_bits()),
                mix: AtomicU32::new(1.0_f32.to_bits()),
                reverse: AtomicBool::new(false),
                running: AtomicBool::new(true),
            }
        }
        fn getf(a: &AtomicU32) -> f32 {
            f32::from_bits(a.load(Ordering::Relaxed))
        }
        fn setf(a: &AtomicU32, v: f32) {
            a.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    /// Load a WAV file into interleaved f32 samples, resampled to target_sr if needed.
    fn load_wav(path: &str, target_sr: f32) -> (Vec<f32>, usize) {
        let reader = hound::WavReader::open(path)
            .unwrap_or_else(|e| panic!("Failed to open WAV file '{}': {}", path, e));

        let spec = reader.spec();
        let wav_sr = spec.sample_rate as f32;
        let wav_channels = spec.channels as usize;

        println!(
            "  WAV: {}Hz, {} channels, {:?} {}bit",
            spec.sample_rate, spec.channels, spec.sample_format, spec.bits_per_sample
        );

        let raw_samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .into_samples::<i32>()
                    .map(|s| s.unwrap() as f32 / max_val)
                    .collect()
            }
            hound::SampleFormat::Float => {
                reader.into_samples::<f32>().map(|s| s.unwrap()).collect()
            }
        };

        let num_frames = raw_samples.len() / wav_channels;
        if (wav_sr - target_sr).abs() > 1.0 {
            println!("  Resampling {}Hz → {}Hz", wav_sr, target_sr);
            let ratio = wav_sr as f64 / target_sr as f64;
            let new_frames = (num_frames as f64 / ratio) as usize;
            let mut resampled = Vec::with_capacity(new_frames * wav_channels);
            for i in 0..new_frames {
                let src_frame = ((i as f64 * ratio) as usize).min(num_frames - 1);
                for ch in 0..wav_channels {
                    resampled.push(raw_samples[src_frame * wav_channels + ch]);
                }
            }
            (resampled, wav_channels)
        } else {
            (raw_samples, wav_channels)
        }
    }

    fn parse_arg_str(args: &[String], flag: &str) -> Option<String> {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1).cloned())
    }
    fn parse_arg_f32(args: &[String], flag: &str) -> Option<f32> {
        parse_arg_str(args, flag).and_then(|s| s.parse().ok())
    }

    let args: Vec<String> = std::env::args().collect();
    let tone_freq = parse_arg_f32(&args, "--freq").unwrap_or(220.0);
    let wav_path = parse_arg_str(&args, "--wav");
    let tone_str = parse_arg_str(&args, "--tone");
    let use_noise = matches!(tone_str.as_deref(), Some("noise") | Some("white-noise"));
    let use_sweep = tone_str.as_deref() == Some("sweep");
    let use_wav = wav_path.is_some();

    println!("╔══════════════════════════════════════════╗");
    println!("║   Codec — Standalone Granular Test       ║");
    println!("╠══════════════════════════════════════════╣");
    if use_wav {
        println!("║  Source: WAV file (looping)               ║");
    } else if use_noise {
        println!("║  Source: white noise                     ║");
    } else if use_sweep {
        println!("║  Source: sweep 20Hz-20kHz                ║");
    } else {
        println!("║  Source: sine {:.0}Hz", tone_freq);
    }
    println!("║  IPC:  localhost:9847                    ║");
    println!("║  Press Ctrl+C to quit                    ║");
    println!("╚══════════════════════════════════════════╝");

    let state = Arc::new(SharedState::new());

    // Start IPC server — map incoming param names to the shared atomics.
    let state_ipc = state.clone();
    let (ipc_tx, _ipc_handle) = ipc_standalone::start_standalone_ipc(move |name: &str, value: f32| {
        match name {
            "density" => SharedState::setf(&state_ipc.density, value),
            "size" => SharedState::setf(&state_ipc.size_ms, value),
            "position" => SharedState::setf(&state_ipc.position, value),
            "spray" => SharedState::setf(&state_ipc.spray, value),
            "pitch" => SharedState::setf(&state_ipc.pitch, value),
            "pitch_spread" => SharedState::setf(&state_ipc.pitch_spread, value),
            "pan_spread" => SharedState::setf(&state_ipc.pan_spread, value),
            "feedback" => SharedState::setf(&state_ipc.feedback, value),
            "mix" => SharedState::setf(&state_ipc.mix, value),
            "reverse" => state_ipc.reverse.store(value >= 0.5, Ordering::Relaxed),
            _ => {}
        }
    });

    // Audio output setup.
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("No output audio device found");
    println!("Audio device: {}", device.name().unwrap_or_default());

    let config = device.default_output_config().expect("No default output config");
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;
    println!("Sample rate: {}Hz, Channels: {}", sample_rate, channels);

    let wav_data: Option<(Vec<f32>, usize)> = wav_path.as_ref().map(|p| {
        println!("Loading WAV: {}", p);
        load_wav(p, sample_rate)
    });

    let state_audio = state.clone();
    let mut engine = GranularEngine::new(sample_rate, 4.0);
    let mut phase: f64 = 0.0;
    let mut sweep_freq: f64 = 20.0;
    let mut rng_state: u32 = 12345;
    let mut sample_count: u64 = 0;
    let send_interval = (sample_rate as u64 / 30).max(1);
    let mut level_peak: f32 = 0.0;

    let mut wav_pos: usize = 0;
    let wav_samples = wav_data.as_ref().map(|(s, _)| s.clone());
    let wav_channels = wav_data.as_ref().map(|(_, c)| *c).unwrap_or(1);
    let wav_total_frames = wav_samples.as_ref().map(|s| s.len() / wav_channels).unwrap_or(0);

    let stream = device
        .build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Snapshot params for this block.
                engine.set_params(GrainParams {
                    density: SharedState::getf(&state_audio.density),
                    size_samples: SharedState::getf(&state_audio.size_ms) * 0.001 * sample_rate,
                    position: SharedState::getf(&state_audio.position),
                    spray: SharedState::getf(&state_audio.spray),
                    pitch_semitones: SharedState::getf(&state_audio.pitch),
                    pitch_spread: SharedState::getf(&state_audio.pitch_spread),
                    pan_spread: SharedState::getf(&state_audio.pan_spread),
                    feedback: SharedState::getf(&state_audio.feedback),
                    mix: SharedState::getf(&state_audio.mix),
                    reverse: state_audio.reverse.load(Ordering::Relaxed),
                });

                for frame in data.chunks_mut(channels) {
                    // Source sample (stereo).
                    let (src_l, src_r) = if let Some(ref wav) = wav_samples {
                        let l = wav[wav_pos * wav_channels];
                        let r = if wav_channels > 1 {
                            wav[wav_pos * wav_channels + 1]
                        } else {
                            l
                        };
                        wav_pos += 1;
                        if wav_pos >= wav_total_frames {
                            wav_pos = 0;
                        }
                        (l, r)
                    } else {
                        let raw = if use_noise {
                            rng_state ^= rng_state << 13;
                            rng_state ^= rng_state >> 17;
                            rng_state ^= rng_state << 5;
                            (rng_state as f32 / u32::MAX as f32 * 2.0 - 1.0) * 0.3
                        } else if use_sweep {
                            let s = (phase * 2.0 * std::f64::consts::PI).sin() as f32 * 0.3;
                            phase += sweep_freq / sample_rate as f64;
                            if phase >= 1.0 {
                                phase -= 1.0;
                            }
                            sweep_freq = 20.0
                                * (20000.0_f64 / 20.0).powf(
                                    (sample_count as f64 % (sample_rate as f64 * 5.0))
                                        / (sample_rate as f64 * 5.0),
                                );
                            s
                        } else {
                            let s = (phase * 2.0 * std::f64::consts::PI).sin() as f32 * 0.3;
                            phase += tone_freq as f64 / sample_rate as f64;
                            if phase >= 1.0 {
                                phase -= 1.0;
                            }
                            s
                        };
                        (raw, raw)
                    };

                    let (out_l, out_r) = engine.process_sample(src_l, src_r);
                    level_peak = level_peak.max(out_l.abs()).max(out_r.abs());

                    for (ch, sample) in frame.iter_mut().enumerate() {
                        *sample = if ch % 2 == 0 { out_l } else { out_r };
                    }
                    sample_count += 1;
                }

                // Send state to UI ~30 times/sec.
                if sample_count % send_interval < (data.len() / channels.max(1)) as u64 {
                    let json = format!(
                        "{{\"type\":\"state\",\"density\":{:.4},\"size\":{:.4},\"position\":{:.4},\"spray\":{:.4},\"pitch\":{:.4},\"pitch_spread\":{:.4},\"pan_spread\":{:.4},\"feedback\":{:.4},\"mix\":{:.4},\"sync\":0,\"reverse\":{},\"division\":4,\"bpm\":0.0,\"level\":{:.4},\"grains\":{}}}",
                        SharedState::getf(&state_audio.density),
                        SharedState::getf(&state_audio.size_ms),
                        SharedState::getf(&state_audio.position),
                        SharedState::getf(&state_audio.spray),
                        SharedState::getf(&state_audio.pitch),
                        SharedState::getf(&state_audio.pitch_spread),
                        SharedState::getf(&state_audio.pan_spread),
                        SharedState::getf(&state_audio.feedback),
                        SharedState::getf(&state_audio.mix),
                        state_audio.reverse.load(Ordering::Relaxed) as i32,
                        level_peak.min(1.0),
                        engine.active_grains(),
                    );
                    let _ = ipc_tx.try_send(json);
                    level_peak = 0.0;
                }
            },
            |err| eprintln!("Audio stream error: {err}"),
            None,
        )
        .expect("Failed to build audio stream");

    stream.play().expect("Failed to start audio stream");
    println!("\n🎵 Audio streaming. Launch the Compose UI to connect.\n");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::Relaxed);
    })
    .expect("Error setting Ctrl-C handler");

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("\nShutting down...");
    state.running.store(false, Ordering::Relaxed);
    drop(stream);
}
