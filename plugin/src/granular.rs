//! Real-time granular "cloud" engine.
//!
//! Incoming audio is written into a stereo circular buffer. A scheduler sprays
//! short, overlapping, windowed *grains* that read back from that buffer with
//! per-grain position, pitch, direction, and pan variation. The summed grains
//! are blended with the dry signal, and (optionally) fed back into the buffer.
//!
//! Real-time safe: the grain pool and buffers are allocated once in [`GranularEngine::new`]
//! (called from `initialize()`), and `process_sample` performs no allocation.

use std::f32::consts::PI;

/// Maximum simultaneously-active grains (voice pool size).
const MAX_GRAINS: usize = 128;
/// Hann window lookup table size.
const WINDOW_SIZE: usize = 2048;
/// Pitch spread maps 0..1 to ± this many semitones.
const PITCH_SPREAD_SEMIS: f32 = 12.0;

/// Per-block parameter snapshot pushed from the plugin before processing.
#[derive(Clone, Copy)]
pub struct GrainParams {
    /// Grains spawned per second.
    pub density: f32,
    /// Grain length in samples.
    pub size_samples: f32,
    /// 0..1 — how far back from the write head grains read (0 = newest).
    pub position: f32,
    /// 0..1 — random spread of the read position.
    pub spray: f32,
    /// Base transpose in semitones.
    pub pitch_semitones: f32,
    /// 0..1 — random per-grain pitch spread.
    pub pitch_spread: f32,
    /// 0..1 — random per-grain pan spread (0 = mono/center).
    pub pan_spread: f32,
    /// 0..0.95 — wet signal fed back into the capture buffer.
    pub feedback: f32,
    /// 0..1 — dry/wet blend.
    pub mix: f32,
    /// Grains play backwards through the buffer.
    pub reverse: bool,
}

impl Default for GrainParams {
    fn default() -> Self {
        Self {
            density: 20.0,
            size_samples: 4000.0,
            position: 0.1,
            spray: 0.0,
            pitch_semitones: 0.0,
            pitch_spread: 0.0,
            pan_spread: 0.5,
            feedback: 0.0,
            mix: 1.0,
            reverse: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Grain {
    active: bool,
    /// Fractional read position into the circular buffer.
    pos: f32,
    /// Read step per output sample (pitch ratio); sign encodes direction.
    step: f32,
    /// Samples elapsed since the grain started.
    age: f32,
    /// Total grain length in samples.
    len: f32,
    gain_l: f32,
    gain_r: f32,
}

impl Grain {
    const fn inactive() -> Self {
        Self {
            active: false,
            pos: 0.0,
            step: 1.0,
            age: 0.0,
            len: 1.0,
            gain_l: 0.0,
            gain_r: 0.0,
        }
    }
}

pub struct GranularEngine {
    sample_rate: f32,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    buf_len: usize,
    write_pos: usize,
    grains: Vec<Grain>,
    /// Hann window, indexed by grain progress.
    window: Vec<f32>,
    /// Countdown (in samples) until the next grain is scheduled.
    samples_until_next: f32,
    /// Last wet output, for the feedback path.
    fb_l: f32,
    fb_r: f32,
    /// Fast xorshift RNG state.
    rng: u32,
    params: GrainParams,
}

impl GranularEngine {
    /// Allocate all buffers up front. `max_seconds` sets the capture buffer length.
    pub fn new(sample_rate: f32, max_seconds: f32) -> Self {
        let buf_len = ((sample_rate * max_seconds) as usize).max(1024);
        let window = (0..WINDOW_SIZE)
            .map(|i| {
                let phase = i as f32 / (WINDOW_SIZE - 1) as f32;
                0.5 - 0.5 * (2.0 * PI * phase).cos()
            })
            .collect();

        Self {
            sample_rate,
            buf_l: vec![0.0; buf_len],
            buf_r: vec![0.0; buf_len],
            buf_len,
            write_pos: 0,
            grains: vec![Grain::inactive(); MAX_GRAINS],
            window,
            samples_until_next: 0.0,
            fb_l: 0.0,
            fb_r: 0.0,
            rng: 0x9E3779B9,
            params: GrainParams::default(),
        }
    }

    pub fn set_params(&mut self, params: GrainParams) {
        self.params = params;
    }

    pub fn reset(&mut self) {
        for s in self.buf_l.iter_mut() {
            *s = 0.0;
        }
        for s in self.buf_r.iter_mut() {
            *s = 0.0;
        }
        for g in self.grains.iter_mut() {
            g.active = false;
        }
        self.write_pos = 0;
        self.samples_until_next = 0.0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
    }

    /// Number of currently-active grains (for the UI's activity meter).
    pub fn active_grains(&self) -> usize {
        self.grains.iter().filter(|g| g.active).count()
    }

    #[inline]
    fn next_rand(&mut self) -> f32 {
        // xorshift32 → 0..1
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Bipolar random in -1..1.
    #[inline]
    fn next_bipolar(&mut self) -> f32 {
        self.next_rand() * 2.0 - 1.0
    }

    #[inline]
    fn read_interp(buf: &[f32], pos: f32, buf_len: usize) -> f32 {
        let i0 = pos.floor() as isize;
        let frac = pos - i0 as f32;
        let a = buf[(i0.rem_euclid(buf_len as isize)) as usize];
        let b = buf[((i0 + 1).rem_euclid(buf_len as isize)) as usize];
        a + (b - a) * frac
    }

    fn spawn_grain(&mut self) {
        let slot = match self.grains.iter().position(|g| !g.active) {
            Some(i) => i,
            None => return, // pool exhausted — drop this grain
        };

        let p = self.params;
        let len = p.size_samples.max(1.0);

        // Read position: `position` sets the base distance behind the write head,
        // `spray` randomizes it. Leave headroom so a grain doesn't run into the
        // write head within its lifetime.
        let max_back = (self.buf_len as f32 - len * 4.0 - 2.0).max(1.0);
        let spray_samples = p.spray * self.sample_rate * 0.5; // up to 0.5 s of jitter
        let back = (p.position * max_back + self.next_rand() * spray_samples)
            .clamp(1.0, self.buf_len as f32 - 2.0);
        let start = (self.write_pos as f32 - back).rem_euclid(self.buf_len as f32);

        // Pitch: base transpose + bipolar spread.
        let semis = p.pitch_semitones + self.next_bipolar() * p.pitch_spread * PITCH_SPREAD_SEMIS;
        let rate = 2.0_f32.powf(semis / 12.0);
        let step = if p.reverse { -rate } else { rate };

        // Equal-power pan, centered with spread.
        let pan = (0.5 + self.next_bipolar() * p.pan_spread * 0.5).clamp(0.0, 1.0);

        self.grains[slot] = Grain {
            active: true,
            pos: start,
            step,
            age: 0.0,
            len,
            gain_l: (1.0 - pan).sqrt(),
            gain_r: pan.sqrt(),
        };
    }

    /// Process one stereo sample. Returns the wet/dry-mixed output.
    #[inline]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        // Capture input (+ feedback) into the buffer.
        let fb = self.params.feedback;
        self.buf_l[self.write_pos] = (in_l + fb * self.fb_l).clamp(-2.0, 2.0);
        self.buf_r[self.write_pos] = (in_r + fb * self.fb_r).clamp(-2.0, 2.0);

        // Schedule new grains based on density.
        self.samples_until_next -= 1.0;
        while self.samples_until_next <= 0.0 {
            self.spawn_grain();
            let interval = self.sample_rate / self.params.density.max(0.01);
            self.samples_until_next += interval.max(1.0);
        }

        // Sum active grains.
        let mut wet_l = 0.0;
        let mut wet_r = 0.0;
        for g in self.grains.iter_mut() {
            if !g.active {
                continue;
            }
            let win_idx = ((g.age / g.len) * (WINDOW_SIZE - 1) as f32) as usize;
            let env = self.window[win_idx.min(WINDOW_SIZE - 1)];
            let s_l = Self::read_interp(&self.buf_l, g.pos, self.buf_len);
            let s_r = Self::read_interp(&self.buf_r, g.pos, self.buf_len);
            wet_l += s_l * env * g.gain_l;
            wet_r += s_r * env * g.gain_r;

            g.pos = (g.pos + g.step).rem_euclid(self.buf_len as f32);
            g.age += 1.0;
            if g.age >= g.len {
                g.active = false;
            }
        }

        // Compensate for overlap so level stays roughly constant with density/size.
        let overlap = (self.params.density * (self.params.size_samples / self.sample_rate)).max(1.0);
        let norm = 1.0 / overlap.sqrt();
        wet_l *= norm;
        wet_r *= norm;

        self.fb_l = wet_l;
        self.fb_r = wet_r;

        self.write_pos = (self.write_pos + 1) % self.buf_len;

        let mix = self.params.mix;
        let out_l = in_l * (1.0 - mix) + wet_l * mix;
        let out_r = in_r * (1.0 - mix) + wet_r * mix;
        (out_l, out_r)
    }
}
