use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::config;

pub struct Clock {
    audio_samples_played: Arc<AtomicU64>,
    audio_sample_rate: u32,

    wall_base: Option<Instant>,
    wall_pts_at_base: f64,

    last_audio_time: f64,
    last_audio_update: Instant,
    using_wall_clock: bool,

    speed: f64,
    base_offset: f64,

    /// After seek, clock is frozen until unfreeze() is called (first video frame displayed)
    frozen: bool,
    frozen_time: f64,
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
            frozen: false,
            frozen_time: 0.0,
        }
    }

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
            frozen: false,
            frozen_time: 0.0,
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        let now = self.time();
        self.speed = speed;
        self.wall_base = Some(Instant::now());
        self.wall_pts_at_base = now;
        if speed != 1.0 {
            self.using_wall_clock = true;
        }
    }

    pub fn time(&mut self) -> f64 {
        if self.frozen {
            return self.frozen_time;
        }

        if self.using_wall_clock {
            return self.wall_time();
        }

        let samples = self.audio_samples_played.load(Ordering::Relaxed);
        let audio_time = self.base_offset
            + (samples as f64 / self.audio_sample_rate as f64);

        let now = Instant::now();
        let time_since_last_update = now.duration_since(self.last_audio_update).as_secs_f64();

        if samples == 0 && time_since_last_update < 1.0 {
            return self.base_offset;
        }

        if (audio_time - self.last_audio_time).abs() < 0.001 && samples > 0 {
            if time_since_last_update > config::AUDIO_STALL_TIMEOUT_SECS {
                log::debug!("Audio stall ({:.3}s), wall clock fallback", time_since_last_update);
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

    /// After seek: freeze clock at target until first video frame arrives
    pub fn reset_for_seek(&mut self, target_secs: f64) {
        self.audio_samples_played.store(0, Ordering::Relaxed);
        self.base_offset = target_secs;
        self.last_audio_time = target_secs;
        self.last_audio_update = Instant::now();
        self.frozen = true;
        self.frozen_time = target_secs;
        // Wall clock / audio clock will be set up on unfreeze
    }

    /// Called when first video frame after seek is displayed — resume clock
    pub fn unfreeze(&mut self) {
        if !self.frozen {
            return;
        }
        self.frozen = false;
        self.audio_samples_played.store(0, Ordering::Relaxed);
        self.last_audio_update = Instant::now();
        if self.speed == 1.0 {
            self.using_wall_clock = false;
        } else {
            self.wall_base = Some(Instant::now());
            self.wall_pts_at_base = self.frozen_time;
            self.using_wall_clock = true;
        }
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    #[allow(dead_code)]
    pub fn speed(&self) -> f64 {
        self.speed
    }
}
