use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;
use parking_lot::Mutex;

use crate::config;
use crate::decode::audio_decoder::DecodedAudio;

/// Shared audio visualization data (updated by callback, read by UI)
pub struct AudioVis {
    /// Per-channel peak level (0.0~1.0), updated every callback
    pub peak_l: f32,
    pub peak_r: f32,
    /// Recent PCM waveform samples for oscilloscope (mono mix, last ~2048 samples)
    pub waveform: Vec<f32>,
}

impl Default for AudioVis {
    fn default() -> Self {
        Self { peak_l: 0.0, peak_r: 0.0, waveform: Vec::new() }
    }
}

pub struct AudioOutput {
    _stream: cpal::Stream,
    volume: Arc<AtomicU64>,
    muted: Arc<AtomicU64>,
    samples_played: Arc<AtomicU64>,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    paused: Arc<AtomicBool>,
    pub vis: Arc<Mutex<AudioVis>>,
    #[allow(dead_code)]
    sample_rate: u32,
}

const WAVEFORM_SIZE: usize = 2048;

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
        let vis = Arc::new(Mutex::new(AudioVis::default()));

        let volume_clone = volume.clone();
        let muted_clone = muted.clone();
        let samples_played_clone = samples_played.clone();
        let paused_clone = paused.clone();
        let vis_clone = vis.clone();

        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(
            VecDeque::with_capacity(config::AUDIO_RING_BUFFER_SIZE),
        ));
        let buffer_feed = buffer.clone();
        let buffer_callback = buffer.clone();

        std::thread::Builder::new()
            .name("audio-feed".to_string())
            .spawn(move || {
                while let Ok(audio) = audio_rx.recv() {
                    let mut buf = buffer_feed.lock();
                    if buf.len() < config::AUDIO_RING_BUFFER_SIZE * 2 {
                        buf.extend(audio.data.iter());
                    }
                }
            })?;

        let channels = config::AUDIO_CHANNELS as usize;

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if paused_clone.load(Ordering::Relaxed) {
                    for s in data.iter_mut() { *s = 0.0; }
                    return;
                }

                let vol = f64::from_bits(volume_clone.load(Ordering::Relaxed));
                let is_muted = muted_clone.load(Ordering::Relaxed) != 0;
                let gain = if is_muted { 0.0 } else { vol as f32 };

                let mut buf = buffer_callback.lock();
                let available = buf.len().min(data.len());

                // Peak detection
                let mut peak_l: f32 = 0.0;
                let mut peak_r: f32 = 0.0;

                for i in 0..available {
                    let sample = buf[i] * gain;
                    data[i] = sample;
                    // Channel peak (interleaved stereo)
                    if channels == 2 {
                        if i % 2 == 0 { peak_l = peak_l.max(sample.abs()); }
                        else { peak_r = peak_r.max(sample.abs()); }
                    } else {
                        peak_l = peak_l.max(sample.abs());
                        peak_r = peak_l;
                    }
                }
                buf.drain(..available);

                for s in &mut data[available..] { *s = 0.0; }

                let frames = available as u64 / channels as u64;
                samples_played_clone.fetch_add(frames, Ordering::Relaxed);

                // Update visualization data (try_lock to avoid blocking audio thread)
                if let Some(mut vis) = vis_clone.try_lock() {
                    // Exponential decay for smooth meters
                    vis.peak_l = vis.peak_l * 0.8 + peak_l * 0.2;
                    vis.peak_r = vis.peak_r * 0.8 + peak_r * 0.2;
                    // But ensure peaks actually reach max
                    if peak_l > vis.peak_l { vis.peak_l = peak_l; }
                    if peak_r > vis.peak_r { vis.peak_r = peak_r; }

                    // Waveform: downsample to mono and append
                    let mono_samples: usize = available / channels;
                    for i in 0..mono_samples {
                        let idx = i * channels;
                        if idx < available {
                            let mono = if channels == 2 && idx + 1 < available {
                                (data[idx] + data[idx + 1]) * 0.5
                            } else {
                                data[idx]
                            };
                            vis.waveform.push(mono);
                        }
                    }
                    // Keep only last WAVEFORM_SIZE samples
                    if vis.waveform.len() > WAVEFORM_SIZE * 2 {
                        let drain = vis.waveform.len() - WAVEFORM_SIZE;
                        vis.waveform.drain(..drain);
                    }
                }
            },
            move |err| { log::error!("Audio stream error: {:?}", err); },
            None,
        )?;

        stream.play()?;

        Ok(Self {
            _stream: stream,
            volume, muted, samples_played,
            buffer, paused, vis,
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

    pub fn flush(&self) {
        self.paused.store(true, Ordering::SeqCst);
        {
            let mut buf = self.buffer.lock();
            buf.clear();
        }
        self.samples_played.store(0, Ordering::SeqCst);
        {
            let mut vis = self.vis.lock();
            vis.peak_l = 0.0;
            vis.peak_r = 0.0;
            vis.waveform.clear();
        }
        // Don't auto-resume — caller controls pause state
    }

    #[allow(dead_code)]
    pub fn samples_played(&self) -> u64 {
        self.samples_played.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn playback_time_secs(&self) -> f64 {
        self.samples_played() as f64 / self.sample_rate as f64
    }
}
