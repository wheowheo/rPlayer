use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;
use parking_lot::Mutex;

use crate::config;
use crate::decode::audio_decoder::DecodedAudio;

pub struct AudioOutput {
    _stream: cpal::Stream,
    volume: Arc<AtomicU64>,
    muted: Arc<AtomicU64>,
    samples_played: Arc<AtomicU64>,
    buffer: Arc<Mutex<Vec<f32>>>,
    paused: Arc<AtomicBool>,
    sample_rate: u32,
}

impl AudioOutput {
    pub fn new(audio_rx: Receiver<DecodedAudio>, samples_played: Arc<AtomicU64>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No audio output device found"))?;

        log::info!("Audio device: {:?}", device.name());

        let config = cpal::StreamConfig {
            channels: config::AUDIO_CHANNELS,
            sample_rate: cpal::SampleRate(config::AUDIO_SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        let volume = Arc::new(AtomicU64::new(f64::to_bits(1.0)));
        let muted = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(AtomicBool::new(false));

        let volume_clone = volume.clone();
        let muted_clone = muted.clone();
        let samples_played_clone = samples_played.clone();
        let paused_clone = paused.clone();

        let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(
            config::AUDIO_RING_BUFFER_SIZE,
        )));
        let buffer_clone = buffer.clone();
        let buffer_callback = buffer.clone();

        // Feed thread: moves decoded audio from channel to buffer
        std::thread::Builder::new()
            .name("audio-feed".to_string())
            .spawn(move || {
                while let Ok(audio) = audio_rx.recv() {
                    let mut buf = buffer_clone.lock();
                    // Cap buffer at a reasonable size to prevent unbounded growth
                    // But DON'T drain old data — let backpressure handle it
                    if buf.len() < config::AUDIO_RING_BUFFER_SIZE * 2 {
                        buf.extend_from_slice(&audio.data);
                    }
                    // If buffer is full, drop this packet (preferable to dropping played data)
                }
            })?;

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let is_paused = paused_clone.load(Ordering::Relaxed);
                if is_paused {
                    for sample in data.iter_mut() {
                        *sample = 0.0;
                    }
                    return;
                }

                let vol = f64::from_bits(volume_clone.load(Ordering::Relaxed));
                let is_muted = muted_clone.load(Ordering::Relaxed) != 0;

                let mut buf = buffer_callback.lock();
                let available = buf.len().min(data.len());

                if available > 0 {
                    let gain = if is_muted { 0.0 } else { vol as f32 };
                    for (out, &inp) in data[..available].iter_mut().zip(buf[..available].iter()) {
                        *out = inp * gain;
                    }
                    buf.drain(..available);

                    // Track samples played for clock
                    let frames = available as u64 / config::AUDIO_CHANNELS as u64;
                    samples_played_clone.fetch_add(frames, Ordering::Relaxed);
                }

                // Fill remainder with silence
                for sample in &mut data[available..] {
                    *sample = 0.0;
                }
            },
            move |err| {
                log::error!("Audio stream error: {:?}", err);
            },
            None,
        )?;

        stream.play()?;

        Ok(Self {
            _stream: stream,
            volume,
            muted,
            samples_played,
            buffer,
            paused,
            sample_rate: config::AUDIO_SAMPLE_RATE,
        })
    }

    pub fn set_volume(&self, vol: f64) {
        self.volume.store(f64::to_bits(vol.clamp(0.0, config::MAX_VOLUME)), Ordering::Relaxed);
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(if muted { 1 } else { 0 }, Ordering::Relaxed);
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// Clear buffer and reset samples counter (for seek)
    pub fn flush(&self) {
        self.paused.store(true, Ordering::Relaxed);
        {
            let mut buf = self.buffer.lock();
            buf.clear();
        }
        self.samples_played.store(0, Ordering::Relaxed);
        self.paused.store(false, Ordering::Relaxed);
    }

    pub fn samples_played(&self) -> u64 {
        self.samples_played.load(Ordering::Relaxed)
    }

    pub fn playback_time_secs(&self) -> f64 {
        self.samples_played() as f64 / self.sample_rate as f64
    }
}
