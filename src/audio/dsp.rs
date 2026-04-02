/// Biquad IIR filter — one instance per channel per band
#[derive(Clone)]
pub struct Biquad {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    x1: f32, x2: f32,
    y1: f32, y2: f32,
}

impl Biquad {
    pub fn new() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    /// Peaking EQ: boost/cut at center frequency
    /// gain_db: -12..+12, freq: center Hz, q: 0.5..4.0, sample_rate: Hz
    pub fn set_peaking(&mut self, freq: f32, gain_db: f32, q: f32, sample_rate: f32) {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * w0.cos();
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * w0.cos();
        let a2 = 1.0 - alpha / a;

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Low-shelf filter
    pub fn set_low_shelf(&mut self, freq: f32, gain_db: f32, sample_rate: f32) {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let alpha = w0.sin() / 2.0 * (2.0_f32).sqrt();

        let a0 = (a + 1.0) + (a - 1.0) * w0.cos() + 2.0 * a.sqrt() * alpha;
        self.b0 = (a * ((a + 1.0) - (a - 1.0) * w0.cos() + 2.0 * a.sqrt() * alpha)) / a0;
        self.b1 = (2.0 * a * ((a - 1.0) - (a + 1.0) * w0.cos())) / a0;
        self.b2 = (a * ((a + 1.0) - (a - 1.0) * w0.cos() - 2.0 * a.sqrt() * alpha)) / a0;
        self.a1 = (-2.0 * ((a - 1.0) + (a + 1.0) * w0.cos())) / a0;
        self.a2 = ((a + 1.0) + (a - 1.0) * w0.cos() - 2.0 * a.sqrt() * alpha) / a0;
    }

    /// High-shelf filter
    pub fn set_high_shelf(&mut self, freq: f32, gain_db: f32, sample_rate: f32) {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let alpha = w0.sin() / 2.0 * (2.0_f32).sqrt();

        let a0 = (a + 1.0) - (a - 1.0) * w0.cos() + 2.0 * a.sqrt() * alpha;
        self.b0 = (a * ((a + 1.0) + (a - 1.0) * w0.cos() + 2.0 * a.sqrt() * alpha)) / a0;
        self.b1 = (-2.0 * a * ((a - 1.0) + (a + 1.0) * w0.cos())) / a0;
        self.b2 = (a * ((a + 1.0) + (a - 1.0) * w0.cos() - 2.0 * a.sqrt() * alpha)) / a0;
        self.a1 = (2.0 * ((a - 1.0) - (a + 1.0) * w0.cos())) / a0;
        self.a2 = ((a + 1.0) - (a - 1.0) * w0.cos() - 2.0 * a.sqrt() * alpha) / a0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
              - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.x1 = 0.0; self.x2 = 0.0;
        self.y1 = 0.0; self.y2 = 0.0;
    }
}

/// 3-band parametric EQ: Bass (low-shelf), Mid (peaking), Treble (high-shelf)
pub struct Equalizer {
    // [L, R] per band
    bass: [Biquad; 2],
    mid: [Biquad; 2],
    treble: [Biquad; 2],
    pub enabled: bool,
}

impl Equalizer {
    pub fn new(sample_rate: f32) -> Self {
        let mut eq = Self {
            bass: [Biquad::new(), Biquad::new()],
            mid: [Biquad::new(), Biquad::new()],
            treble: [Biquad::new(), Biquad::new()],
            enabled: false,
        };
        eq.set_bands(0.0, 0.0, 0.0, sample_rate);
        eq
    }

    /// Set EQ gains in dB: bass_db, mid_db, treble_db (-12..+12)
    pub fn set_bands(&mut self, bass_db: f32, mid_db: f32, treble_db: f32, sample_rate: f32) {
        for b in &mut self.bass { b.set_low_shelf(200.0, bass_db, sample_rate); }
        for b in &mut self.mid { b.set_peaking(1000.0, mid_db, 1.0, sample_rate); }
        for b in &mut self.treble { b.set_high_shelf(4000.0, treble_db, sample_rate); }
        self.enabled = bass_db.abs() > 0.1 || mid_db.abs() > 0.1 || treble_db.abs() > 0.1;
    }

    /// Process interleaved stereo samples in-place
    pub fn process_stereo(&mut self, data: &mut [f32]) {
        if !self.enabled { return; }
        let mut i = 0;
        while i + 1 < data.len() {
            data[i] = self.bass[0].process(data[i]);
            data[i] = self.mid[0].process(data[i]);
            data[i] = self.treble[0].process(data[i]);

            data[i + 1] = self.bass[1].process(data[i + 1]);
            data[i + 1] = self.mid[1].process(data[i + 1]);
            data[i + 1] = self.treble[1].process(data[i + 1]);
            i += 2;
        }
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        for b in &mut self.bass { b.reset(); }
        for b in &mut self.mid { b.reset(); }
        for b in &mut self.treble { b.reset(); }
    }
}

/// Simple compressor/limiter
pub struct Compressor {
    pub enabled: bool,
    pub threshold: f32,  // dB, e.g. -10.0
    pub ratio: f32,      // e.g. 4.0 means 4:1
    pub attack: f32,     // seconds
    pub release: f32,    // seconds
    envelope: f32,       // current envelope level (linear)
    sample_rate: f32,
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false,
            threshold: -10.0,
            ratio: 4.0,
            attack: 0.005,
            release: 0.05,
            envelope: 0.0,
            sample_rate,
        }
    }

    /// Process interleaved stereo samples in-place
    pub fn process_stereo(&mut self, data: &mut [f32]) {
        if !self.enabled { return; }

        let attack_coeff = (-1.0 / (self.attack * self.sample_rate)).exp();
        let release_coeff = (-1.0 / (self.release * self.sample_rate)).exp();
        let threshold_lin = 10.0_f32.powf(self.threshold / 20.0);

        let mut i = 0;
        while i + 1 < data.len() {
            // Detect peak of stereo pair
            let peak = data[i].abs().max(data[i + 1].abs());

            // Envelope follower
            let coeff = if peak > self.envelope { attack_coeff } else { release_coeff };
            self.envelope = coeff * self.envelope + (1.0 - coeff) * peak;

            // Gain computation
            let gain = if self.envelope > threshold_lin {
                let over_db = 20.0 * (self.envelope / threshold_lin).log10();
                let compressed_db = over_db / self.ratio;
                let reduction_db = over_db - compressed_db;
                10.0_f32.powf(-reduction_db / 20.0)
            } else {
                1.0
            };

            data[i] *= gain;
            data[i + 1] *= gain;
            i += 2;
        }
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.envelope = 0.0;
    }
}
