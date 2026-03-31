use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::config;

/// Master clock for A/V synchronization.
/// Audio clock is primary; wall clock is fallback when audio stalls.
pub struct Clock {
    // Audio-driven PTS (set by audio callback via samples consumed)
    audio_samples_played: Arc<AtomicU64>,
    audio_sample_rate: u32,

    // Wall clock fallback
    wall_base: Option<Instant>,
    wall_pts_at_base: f64,

    // State
    last_audio_time: f64,
    last_audio_update: Instant,
    using_wall_clock: bool,

    speed: f64,
    base_offset: f64, // added to computed time (for seek)
}

impl Clock {
    pub fn new(audio_samples_played: Arc<AtomicU64>, sample_rate: u32) -> Self {
        Self {
            audio_samples_played,
            audio_sample_rate: sample_rate,
            wall_base: None,
            wall_pts_at_base: 0.0,
            last_audio_time: 0.0,
            last_audio_update: Instant::now(),
            using_wall_clock: false,
            speed: 1.0,
            base_offset: 0.0,
        }
    }

    /// Create a clock without audio (wall clock only)
    pub fn wall_only() -> Self {
        Self {
            audio_samples_played: Arc::new(AtomicU64::new(0)),
            audio_sample_rate: config::AUDIO_SAMPLE_RATE,
            wall_base: Some(Instant::now()),
            wall_pts_at_base: 0.0,
            last_audio_time: 0.0,
            last_audio_update: Instant::now(),
            using_wall_clock: true,
            speed: 1.0,
            base_offset: 0.0,
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        // Snapshot current time before changing speed
        let now = self.time();
        self.speed = speed;
        // Switch to wall clock for speed != 1.0 (audio plays at 1x, video needs to run faster)
        self.wall_base = Some(Instant::now());
        self.wall_pts_at_base = now;
        if speed != 1.0 {
            self.using_wall_clock = true;
        }
    }

    /// Get current playback time in seconds
    pub fn time(&mut self) -> f64 {
        if self.using_wall_clock {
            return self.wall_time();
        }

        let samples = self.audio_samples_played.load(Ordering::Relaxed);
        let audio_time = self.base_offset
            + (samples as f64 / self.audio_sample_rate as f64);

        let now = Instant::now();

        // Give audio time to start (first 500ms: use wall clock, don't judge stall)
        let time_since_last_update = now.duration_since(self.last_audio_update).as_secs_f64();

        if samples == 0 && time_since_last_update < 1.0 {
            // Audio hasn't started yet — use wall clock temporarily
            return self.base_offset + time_since_last_update * self.speed;
        }

        // Detect audio stall (only after audio has started)
        if (audio_time - self.last_audio_time).abs() < 0.001 && samples > 0 {
            if time_since_last_update > config::AUDIO_STALL_TIMEOUT_SECS {
                log::debug!("Audio stall detected ({:.3}s), wall clock fallback", time_since_last_update);
                self.using_wall_clock = true;
                self.wall_base = Some(now);
                self.wall_pts_at_base = audio_time;
                return self.wall_time();
            }
        } else {
            self.last_audio_time = audio_time;
            self.last_audio_update = now;
        }

        audio_time
    }

    fn wall_time(&self) -> f64 {
        if let Some(base) = self.wall_base {
            let elapsed = base.elapsed().as_secs_f64() * self.speed;
            self.wall_pts_at_base + elapsed
        } else {
            self.base_offset
        }
    }

    /// Reset clock after seek. Must also reset the external samples_played atomic.
    pub fn reset_for_seek(&mut self, target_secs: f64) {
        self.audio_samples_played.store(0, Ordering::Relaxed);
        self.base_offset = target_secs;
        self.last_audio_time = target_secs;
        self.last_audio_update = Instant::now();
        if self.speed == 1.0 {
            self.using_wall_clock = false;
        } else {
            self.wall_base = Some(Instant::now());
            self.wall_pts_at_base = target_secs;
            self.using_wall_clock = true;
        }
    }

    #[allow(dead_code)]
    pub fn speed(&self) -> f64 {
        self.speed
    }
}
