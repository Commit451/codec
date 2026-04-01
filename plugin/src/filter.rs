/// Biquad low-pass filter (transposed direct form II).
#[derive(Clone)]
pub struct BiquadLpf {
    // Coefficients
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // State
    s1: f32,
    s2: f32,
    sample_rate: f32,
    // Cached params for dirty check
    last_cutoff: f32,
    last_resonance: f32,
}

impl BiquadLpf {
    pub fn new(sample_rate: f32) -> Self {
        let mut f = Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: 0.0,
            s2: 0.0,
            sample_rate,
            last_cutoff: -1.0,
            last_resonance: -1.0,
        };
        f.set_params(1000.0, 0.5);
        f
    }

    /// Recalculate coefficients only if params actually changed.
    pub fn set_params(&mut self, cutoff_hz: f32, resonance: f32) {
        // Skip expensive trig if nothing changed
        if cutoff_hz == self.last_cutoff && resonance == self.last_resonance {
            return;
        }
        self.last_cutoff = cutoff_hz;
        self.last_resonance = resonance;

        let freq = cutoff_hz.clamp(20.0, self.sample_rate * 0.49);
        // Map resonance 0..1 to Q 0.5..10
        let q = 0.5 + resonance * 9.5;

        let w0 = 2.0 * std::f32::consts::PI * freq / self.sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        // Normalize
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Process a single sample (transposed direct form II).
    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.s1;
        self.s1 = self.b1 * input - self.a1 * output + self.s2;
        self.s2 = self.b2 * input - self.a2 * output;
        output
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        // Force recalc on next set_params
        self.last_cutoff = -1.0;
    }
}
