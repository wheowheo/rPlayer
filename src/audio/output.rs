use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;
use parking_lot::Mutex;

use crate::config;
use crate::decode::audio_decoder::DecodedAudio;
use super::dsp::{Equalizer, Compressor};
use super::stretch::TimeStretcher;

pub struct AudioVis {
    pub peak_l: f32,
    pub peak_r: f32,
    pub waveform: Vec<f32>,
}

impl Default for AudioVis {
    fn default() -> Self {
        Self { peak_l: 0.0, peak_r: 0.0, waveform: Vec::new() }
    }
}

/// Shared DSP parameters (set from UI thread, read by feed thread)
pub struct DspParams {
    pub speed: f64,
    pub eq_bass: f32,   // dB
    pub eq_mid: f32,
    pub eq_treble: f32,
    pub compressor_enabled: bool,
    pub compressor_threshold: f32, // dB
    pub compressor_ratio: f32,
}

impl Default for DspParams {
    fn default() -> Self {
        Self {
            speed: 1.0,
            eq_bass: 0.0, eq_mid: 0.0, eq_treble: 0.0,
            compressor_enabled: false,
            compressor_threshold: -10.0,
            compressor_ratio: 4.0,
        }
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
    pub dsp: Arc<Mutex<DspParams>>,
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

        let stream_config = cpal::StreamConfig {
            channels: config::AUDIO_CHANNELS,
            sample_rate: cpal::SampleRate(config::AUDIO_SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        let volume = Arc::new(AtomicU64::new(f64::to_bits(1.0)));
        let muted = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(AtomicBool::new(false));
        let vis = Arc::new(Mutex::new(AudioVis::default()));
        let dsp = Arc::new(Mutex::new(DspParams::default()));

        let volume_clone = volume.clone();
        let muted_clone = muted.clone();
        let samples_played_clone = samples_played.clone();
        let paused_clone = paused.clone();
        let vis_clone = vis.clone();
        let dsp_clone = dsp.clone();

        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(
            VecDeque::with_capacity(config::AUDIO_RING_BUFFER_SIZE),
        ));
        let buffer_feed = buffer.clone();
        let buffer_callback = buffer.clone();

        let sr = config::AUDIO_SAMPLE_RATE as f32;
        let ch = config::AUDIO_CHANNELS;

        // Feed thread: decode → stretch → EQ → compress → buffer
        std::thread::Builder::new()
            .name("audio-feed".to_string())
            .spawn(move || {
                let mut stretcher = TimeStretcher::new(config::AUDIO_SAMPLE_RATE, ch);
                let mut eq = Equalizer::new(sr);
                let mut compressor = Compressor::new(sr);
                let mut last_speed = 1.0_f64;
                let mut last_eq = (0.0_f32, 0.0_f32, 0.0_f32);
                let mut last_comp = (false, -10.0_f32, 4.0_f32);

                while let Ok(audio) = audio_rx.recv() {
                    // Read DSP params (try_lock to avoid blocking decode thread)
                    if let Some(params) = dsp_clone.try_lock() {
                        if (params.speed - last_speed).abs() > 0.001 {
                            stretcher.set_speed(params.speed);
                            last_speed = params.speed;
                        }
                        let eq_key = (params.eq_bass, params.eq_mid, params.eq_treble);
                        if eq_key != last_eq {
                            eq.set_bands(params.eq_bass, params.eq_mid, params.eq_treble, sr);
                            last_eq = eq_key;
                        }
                        let comp_key = (params.compressor_enabled, params.compressor_threshold, params.compressor_ratio);
                        if comp_key != last_comp {
                            compressor.enabled = params.compressor_enabled;
                            compressor.threshold = params.compressor_threshold;
                            compressor.ratio = params.compressor_ratio;
                            last_comp = comp_key;
                        }
                    }

                    // 1. Time-stretch
                    let mut processed = stretcher.process(&audio.data);
                    if processed.is_empty() {
                        continue;
                    }

                    // 2. EQ + Compress
                    eq.process_stereo(&mut processed);
                    compressor.process_stereo(&mut processed);

                    // 3. Feed to output buffer
                    let mut buf = buffer_feed.lock();
                    if buf.len() < config::AUDIO_RING_BUFFER_SIZE * 2 {
                        buf.extend(processed.iter());
                    }
                }
            })?;

        let channels = config::AUDIO_CHANNELS as usize;

        let stream = device.build_output_stream(
            &stream_config,
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

                let mut peak_l: f32 = 0.0;
                let mut peak_r: f32 = 0.0;

                for i in 0..available {
                    let sample = buf[i] * gain;
                    data[i] = sample;
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

                if let Some(mut vis) = vis_clone.try_lock() {
                    vis.peak_l = vis.peak_l * 0.8 + peak_l * 0.2;
                    vis.peak_r = vis.peak_r * 0.8 + peak_r * 0.2;
                    if peak_l > vis.peak_l { vis.peak_l = peak_l; }
                    if peak_r > vis.peak_r { vis.peak_r = peak_r; }

                    let mono_samples = available / channels;
                    for i in 0..mono_samples {
                        let idx = i * channels;
                        if idx < available {
                            let mono = if channels == 2 && idx + 1 < available {
                                (data[idx] + data[idx + 1]) * 0.5
                            } else { data[idx] };
                            vis.waveform.push(mono);
                        }
                    }
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
            buffer, paused, vis, dsp,
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

    pub fn set_speed(&self, speed: f64) {
        if let Some(mut params) = self.dsp.try_lock() {
            params.speed = speed;
        }
    }

    pub fn set_eq(&self, bass: f32, mid: f32, treble: f32) {
        if let Some(mut params) = self.dsp.try_lock() {
            params.eq_bass = bass;
            params.eq_mid = mid;
            params.eq_treble = treble;
        }
    }

    pub fn set_compressor(&self, enabled: bool, threshold: f32, ratio: f32) {
        if let Some(mut params) = self.dsp.try_lock() {
            params.compressor_enabled = enabled;
            params.compressor_threshold = threshold;
            params.compressor_ratio = ratio;
        }
    }

    pub fn flush(&self) {
        self.paused.store(true, Ordering::SeqCst);
        { self.buffer.lock().clear(); }
        self.samples_played.store(0, Ordering::SeqCst);
        {
            let mut vis = self.vis.lock();
            vis.peak_l = 0.0; vis.peak_r = 0.0; vis.waveform.clear();
        }
    }

    #[allow(dead_code)]
    pub fn samples_played(&self) -> u64 { self.samples_played.load(Ordering::Relaxed) }

    #[allow(dead_code)]
    pub fn playback_time_secs(&self) -> f64 { self.samples_played() as f64 / self.sample_rate as f64 }
}
