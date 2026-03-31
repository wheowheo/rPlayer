use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;
use winit::window::Window;

use crate::audio::output::AudioOutput;
use crate::config;
use crate::decode::video_decoder::DecodedFrame;
use crate::media::clock::Clock;
use crate::media::pipeline::MediaPipeline;
use crate::subtitle::SubtitleTrack;
use crate::video::renderer::VideoRenderer;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    Empty,
    Playing,
    Paused,
    Stopped,
    Buffering,
}

pub struct UiState {
    pub playback_state: PlaybackState,
    pub volume: f64,
    pub speed: f64,
    pub muted: bool,
    pub current_time: f64,
    pub duration: f64,
    pub video_info: String,
    pub show_info_overlay: bool,
    pub subtitle_text: String,
}

pub struct App {
    // GPU
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,

    // Renderers
    pub video_renderer: VideoRenderer,

    // egui
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,

    // State
    pub ui_state: UiState,

    // Media
    pub pipeline: Option<MediaPipeline>,
    pub audio_output: Option<AudioOutput>,
    pub clock: Option<Clock>,
    pub pending_frame: Option<DecodedFrame>,
    video_fps: f64,

    // Subtitle
    pub subtitle: Option<SubtitleTrack>,

    // Window
    pub window: Arc<Window>,
    pub video_size: Option<(u32, u32)>,
}

fn draw_ui(ctx: &egui::Context, state: &UiState) {
    // Status bar at bottom
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            let state_text = match state.playback_state {
                PlaybackState::Empty => "파일을 열어주세요 (O)",
                PlaybackState::Playing => "재생 중",
                PlaybackState::Paused => "일시정지",
                PlaybackState::Stopped => "정지",
                PlaybackState::Buffering => "버퍼링...",
            };
            ui.label(state_text);
            ui.separator();

            if state.duration > 0.0 {
                let cur = format_time(state.current_time);
                let dur = format_time(state.duration);
                ui.label(format!("{cur} / {dur}"));
                ui.separator();
            }

            let vol_text = if state.muted {
                "음소거".to_string()
            } else {
                format!("볼륨: {:.0}%", state.volume * 100.0)
            };
            ui.label(vol_text);
            ui.separator();
            ui.label(format!("배속: {:.2}x", state.speed));

            if !state.video_info.is_empty() {
                ui.separator();
                ui.label(&state.video_info);
            }
        });
    });

    // Info overlay (Tab)
    if state.show_info_overlay {
        egui::Area::new(egui::Id::new("info_overlay"))
            .fixed_pos(egui::pos2(10.0, 10.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new(&state.video_info).monospace().size(14.0));
                    ui.label(egui::RichText::new(
                        format!("시간: {} / {}", format_time(state.current_time), format_time(state.duration))
                    ).monospace().size(14.0));
                    ui.label(egui::RichText::new(
                        format!("배속: {:.2}x | 볼륨: {:.0}%{}",
                            state.speed,
                            state.volume * 100.0,
                            if state.muted { " (음소거)" } else { "" }
                        )
                    ).monospace().size(14.0));
                });
            });
    }

    // Subtitle overlay
    if !state.subtitle_text.is_empty() {
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("subtitle"))
            .fixed_pos(egui::pos2(screen.center().x, screen.max.y - 80.0))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 180))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .rounding(4.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&state.subtitle_text)
                                .color(egui::Color32::WHITE)
                                .size(20.0)
                        );
                    });
            });
    }
}

fn format_time(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

impl App {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to find GPU adapter"))?;

        log::info!("GPU adapter: {:?}", adapter.get_info().name);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rplayer_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            }, None)
            .await?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let video_renderer = VideoRenderer::new(&device, surface_format);

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(&device, surface_format, None, 1, false);

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            video_renderer,
            egui_ctx,
            egui_state,
            egui_renderer,
            ui_state: UiState {
                playback_state: PlaybackState::Empty,
                volume: 1.0,
                speed: 1.0,
                muted: false,
                current_time: 0.0,
                duration: 0.0,
                video_info: String::new(),
                show_info_overlay: false,
                subtitle_text: String::new(),
            },
            pipeline: None,
            audio_output: None,
            clock: None,
            pending_frame: None,
            video_fps: 0.0,
            subtitle: None,
            window,
            video_size: None,
        })
    }

    pub fn open_file(&mut self, path: &str) {
        // Stop existing pipeline
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.stop();
        }
        self.audio_output = None;

        match MediaPipeline::open(path) {
            Ok(mut pipeline) => {
                let info = &pipeline.info;
                self.ui_state.duration = info.duration_secs;
                self.video_size = Some((info.video_width, info.video_height));

                let codec = info.video.as_ref()
                    .map(|v| v.codec_name.as_str())
                    .unwrap_or("?");
                self.ui_state.video_info = format!(
                    "{}x{} {} {:.1}fps",
                    info.video_width, info.video_height, codec, info.video_fps
                );

                // Resize window to video aspect ratio
                if info.video_width > 0 && info.video_height > 0 {
                    let scale = (config::DEFAULT_HEIGHT as f64) / (info.video_height as f64);
                    let w = (info.video_width as f64 * scale) as u32;
                    let h = config::DEFAULT_HEIGHT;
                    let _ = self.window.request_inner_size(winit::dpi::LogicalSize::new(w, h));
                }

                // Start audio output and clock
                let samples_played = Arc::new(AtomicU64::new(0));
                if let Some(audio_rx) = pipeline.audio_rx.take() {
                    match AudioOutput::new(audio_rx, samples_played.clone()) {
                        Ok(audio) => {
                            audio.set_volume(self.ui_state.volume);
                            audio.set_muted(self.ui_state.muted);
                            self.audio_output = Some(audio);
                            self.clock = Some(Clock::new(samples_played, crate::config::AUDIO_SAMPLE_RATE));
                            log::info!("Audio output started");
                        }
                        Err(e) => {
                            log::error!("Failed to start audio: {}", e);
                            self.clock = Some(Clock::wall_only());
                        }
                    }
                } else {
                    self.clock = Some(Clock::wall_only());
                }

                self.video_fps = info.video_fps;
                self.pipeline = Some(pipeline);
                self.ui_state.playback_state = PlaybackState::Playing;
                self.ui_state.current_time = 0.0;
                self.ui_state.subtitle_text.clear();
                self.pending_frame = None;

                // Auto-detect subtitle file (same name, .srt or .smi)
                let path_base = std::path::Path::new(path);
                let stem = path_base.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let dir = path_base.parent().unwrap_or(std::path::Path::new("."));
                self.subtitle = None;
                for ext in &["srt", "smi", "sami"] {
                    let sub_path = dir.join(format!("{}.{}", stem, ext));
                    if sub_path.exists() {
                        if let Some(track) = SubtitleTrack::load_file(&sub_path.to_string_lossy()) {
                            log::info!("Subtitle loaded: {:?}", sub_path);
                            self.subtitle = Some(track);
                            break;
                        }
                    }
                }

                let file_name = path_base
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                self.window.set_title(&format!("{} - {}", file_name, config::APP_NAME));

                log::info!("Opened: {} ({})", path, self.ui_state.video_info);
            }
            Err(e) => {
                log::error!("Failed to open {}: {}", path, e);
            }
        }
    }

    pub fn update_frame(&mut self) {
        if self.ui_state.playback_state != PlaybackState::Playing {
            return;
        }

        let Some(pipeline) = &self.pipeline else { return };
        let Some(clock) = &mut self.clock else { return };

        let clock_time = clock.time();
        self.ui_state.current_time = clock_time;

        let frame_duration = if self.video_fps > 0.0 {
            1.0 / self.video_fps
        } else {
            1.0 / 30.0
        };

        // Try to get a frame to display
        let mut frames_dropped = 0u32;
        loop {
            let frame = if self.pending_frame.is_some() {
                self.pending_frame.take().unwrap()
            } else {
                match pipeline.frame_rx.try_recv() {
                    Ok(f) => f,
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        self.ui_state.playback_state = PlaybackState::Stopped;
                        log::info!("Playback finished");
                        return;
                    }
                }
            };

            // Skip sentinel frames (from seek drain)
            if frame.pts_secs < 0.0 {
                continue;
            }

            let diff = frame.pts_secs - clock_time;

            if diff < -frame_duration * 2.0 {
                // Frame is late — drop it and try next
                frames_dropped += 1;
                if frames_dropped > 30 {
                    // Too many drops — just show something
                    self.video_renderer.upload_rgba_frame(
                        &self.device, &self.queue,
                        frame.width, frame.height, &frame.data,
                    );
                    break;
                }
                continue;
            } else if diff > config::SYNC_THRESHOLD_SECS {
                // Frame is early — save for next render cycle
                self.pending_frame = Some(frame);
                break;
            } else {
                // Display this frame
                self.video_renderer.upload_rgba_frame(
                    &self.device, &self.queue,
                    frame.width, frame.height, &frame.data,
                );
                break;
            }
        }
        if frames_dropped > 0 {
            log::debug!("Dropped {} late frames (clock={:.3})", frames_dropped, clock_time);
        }

        // Update subtitle
        if let Some(ref subtitle) = self.subtitle {
            if let Some(text) = subtitle.current_text(clock_time) {
                self.ui_state.subtitle_text = text.to_string();
            } else {
                self.ui_state.subtitle_text.clear();
            }
        }
    }

    pub fn seek(&mut self, target: f64) {
        let target = target.clamp(0.0, self.ui_state.duration);
        if let Some(p) = &self.pipeline {
            let _ = p.cmd_tx.send(crate::media::pipeline::PipelineCommand::Seek(target));
        }
        // Flush audio buffer and reset clock
        if let Some(ref audio) = self.audio_output {
            audio.flush();
        }
        if let Some(ref mut clock) = self.clock {
            clock.reset_for_seek(target);
        }
        self.pending_frame = None;
        self.ui_state.current_time = target;
        log::debug!("Seek to {:.2}s", target);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });

        // Video pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("video_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05, g: 0.05, b: 0.05, a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            self.video_renderer.render(&mut render_pass);
        }

        // egui pass
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            draw_ui(ctx, &self.ui_state);
        });

        self.egui_state.handle_platform_output(&self.window, full_output.platform_output);

        let tris = self.egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
        }
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &tris,
            &screen_descriptor,
        );

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            let mut render_pass = render_pass.forget_lifetime();
            self.egui_renderer.render(&mut render_pass, &tris, &screen_descriptor);
        }

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
