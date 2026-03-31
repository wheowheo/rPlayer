use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::config;

/// Master clock for A/V synchronization.
/// Audio clock is primary; wall clock is fallback when audio stalls.
pub struct Clock {
    // Audio-driven PTS (set by audio callback via samples consumed)
    audio_pts: Arc<AtomicU64>,    // f64 bits
    audio_start_pts: f64,

    // Wall clock fallback
    wall_base: Option<Instant>,
    wall_pts_at_base: f64,

    // State
    last_audio_pts: f64,
    last_audio_update: Instant,
    using_wall_clock: bool,

    speed: f64,
}

impl Clock {
    pub fn new(audio_samples_played: Arc<AtomicU64>) -> Self {
        Self {
            audio_pts: audio_samples_played,
            audio_start_pts: 0.0,
            wall_base: None,
            wall_pts_at_base: 0.0,
            last_audio_pts: 0.0,
            last_audio_update: Instant::now(),
            using_wall_clock: false,
            speed: 1.0,
        }
    }

    /// Create a clock without audio (wall clock only)
    pub fn wall_only() -> Self {
        Self {
            audio_pts: Arc::new(AtomicU64::new(0)),
            audio_start_pts: 0.0,
            wall_base: Some(Instant::now()),
            wall_pts_at_base: 0.0,
            last_audio_pts: 0.0,
            last_audio_update: Instant::now(),
            using_wall_clock: true,
            speed: 1.0,
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        // Save current time before changing speed
        let now = self.time();
        self.speed = speed;
        self.wall_base = Some(Instant::now());
        self.wall_pts_at_base = now;
    }

    pub fn set_audio_start_pts(&mut self, pts: f64) {
        self.audio_start_pts = pts;
    }

    /// Get current playback time in seconds
    pub fn time(&mut self) -> f64 {
        if self.using_wall_clock {
            return self.wall_time();
        }

        let samples = self.audio_pts.load(Ordering::Relaxed);
        let audio_time = self.audio_start_pts
            + (samples as f64 / config::AUDIO_SAMPLE_RATE as f64);

        // Detect audio stall
        let now = Instant::now();
        if (audio_time - self.last_audio_pts).abs() < 0.001 {
            let stall_dur = now.duration_since(self.last_audio_update).as_secs_f64();
            if stall_dur > config::AUDIO_STALL_TIMEOUT_SECS {
                log::debug!("Audio stall detected, switching to wall clock");
                self.using_wall_clock = true;
                self.wall_base = Some(now);
                self.wall_pts_at_base = self.last_audio_pts;
                return self.wall_time();
            }
        } else {
            self.last_audio_pts = audio_time;
            self.last_audio_update = now;

            // If we were using wall clock and audio resumed, switch back
            if self.using_wall_clock {
                self.using_wall_clock = false;
            }
        }

        audio_time
    }

    fn wall_time(&self) -> f64 {
        if let Some(base) = self.wall_base {
            let elapsed = base.elapsed().as_secs_f64() * self.speed;
            self.wall_pts_at_base + elapsed
        } else {
            0.0
        }
    }

    pub fn reset(&mut self) {
        self.audio_start_pts = 0.0;
        self.wall_base = None;
        self.wall_pts_at_base = 0.0;
        self.last_audio_pts = 0.0;
        self.last_audio_update = Instant::now();
        self.using_wall_clock = false;
    }
}
