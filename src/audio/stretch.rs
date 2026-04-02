use rubato::{FftFixedIn, Resampler};

/// Pitch-preserving time stretcher using rubato FFT resampler.
/// speed > 1.0 = faster playback (fewer output samples per input).
/// speed < 1.0 = slower playback (more output samples per input).
pub struct TimeStretcher {
    resampler: Option<FftFixedIn<f32>>,
    channels: usize,
    sample_rate: usize,
    current_speed: f64,
    input_buf: Vec<Vec<f32>>,  // per-channel deinterleaved
    output_buf: Vec<f32>,      // interleaved output
}

impl TimeStretcher {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            resampler: None,
            channels: channels as usize,
            sample_rate: sample_rate as usize,
            current_speed: 1.0,
            input_buf: Vec::new(),
            output_buf: Vec::new(),
        }
    }

    /// Update speed. Recreates resampler if speed changed.
    pub fn set_speed(&mut self, speed: f64) {
        let speed = speed.clamp(0.25, 4.0);
        if (speed - self.current_speed).abs() < 0.001 {
            return;
        }
        self.current_speed = speed;
        self.resampler = None; // Force recreation on next process
    }

    fn ensure_resampler(&mut self) {
        if self.resampler.is_some() {
            return;
        }
        if (self.current_speed - 1.0).abs() < 0.01 {
            return; // No stretching needed at 1x
        }

        // rubato ratio = output_rate / input_rate
        // For speed 2x: we want half the output samples → ratio = 1/2
        let ratio = 1.0 / self.current_speed;
        let chunk_size = 1024;

        match FftFixedIn::<f32>::new(
            self.sample_rate,
            (self.sample_rate as f64 * ratio) as usize,
            chunk_size,
            2, // sub-chunks for quality
            self.channels,
        ) {
            Ok(r) => {
                self.input_buf = vec![Vec::with_capacity(chunk_size * 2); self.channels];
                self.resampler = Some(r);
            }
            Err(e) => {
                log::error!("TimeStretcher init failed: {}", e);
            }
        }
    }

    /// Process interleaved stereo samples. Returns stretched interleaved output.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if (self.current_speed - 1.0).abs() < 0.01 {
            return input.to_vec(); // Passthrough at 1x
        }

        self.ensure_resampler();

        let Some(ref mut resampler) = self.resampler else {
            return input.to_vec();
        };

        // Deinterleave input into per-channel buffers
        let frames = input.len() / self.channels;
        for ch_buf in &mut self.input_buf {
            ch_buf.clear();
        }
        for f in 0..frames {
            for ch in 0..self.channels {
                self.input_buf[ch].push(input[f * self.channels + ch]);
            }
        }

        // Process through rubato
        let needed = resampler.input_frames_next();
        if self.input_buf[0].len() < needed {
            return Vec::new();
        }

        // Feed exactly what rubato needs
        let feed: Vec<&[f32]> = self.input_buf.iter()
            .map(|ch| &ch[..needed])
            .collect();

        match resampler.process(&feed, None) {
            Ok(output_channels) => {
                // Drain consumed input
                for ch_buf in &mut self.input_buf {
                    ch_buf.drain(..needed);
                }

                // Interleave output
                let out_frames = output_channels.get(0).map(|c| c.len()).unwrap_or(0);
                self.output_buf.clear();
                self.output_buf.reserve(out_frames * self.channels);
                for f in 0..out_frames {
                    for ch in 0..self.channels {
                        self.output_buf.push(
                            output_channels.get(ch)
                                .and_then(|c| c.get(f))
                                .copied()
                                .unwrap_or(0.0)
                        );
                    }
                }
                self.output_buf.clone()
            }
            Err(e) => {
                log::debug!("TimeStretcher process error: {}", e);
                Vec::new()
            }
        }
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.resampler = None;
        for ch in &mut self.input_buf {
            ch.clear();
        }
        self.output_buf.clear();
    }
}
