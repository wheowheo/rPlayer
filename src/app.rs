use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use winit::window::Window;

use crate::audio::output::AudioOutput;
use crate::config;
use crate::decode::video_decoder::{DecodeMode, DecodedFrame};
use crate::media::clock::Clock;
use crate::media::pipeline::{MediaPipeline, PipelineCommand};
use crate::subtitle::SubtitleTrack;
use crate::video::renderer::VideoRenderer;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    Empty,
    Playing,
    Paused,
    Stopped,
    #[allow(dead_code)]
    Buffering,
}

/// UI actions returned from draw_ui, processed by the app
#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    None,
    OpenFile,
    PlayPause,
    Stop,
    SeekForward,
    SeekBackward,
    VolumeUp,
    VolumeDown,
    MuteToggle,
    SpeedUp,
    SpeedDown,
    ToggleDecoder,
    ToggleInfoOverlay,
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
    pub decode_mode: String,
    pub show_context_menu: bool,
    pub context_menu_pos: egui::Pos2,
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

fn draw_ui(ctx: &egui::Context, state: &mut UiState) -> Vec<UiAction> {
    let mut actions = Vec::new();

    // ========== Top menu bar ==========
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("파일", |ui| {
                if ui.button("  \u{1F4C2}  열기...  (O)").clicked() {
                    actions.push(UiAction::OpenFile);
                    ui.close_menu();
                }
            });
            ui.menu_button("재생", |ui| {
                let play_label = match state.playback_state {
                    PlaybackState::Playing => "  \u{23F8}  일시정지  (Space)",
                    _ => "  \u{25B6}  재생  (Space)",
                };
                if ui.button(play_label).clicked() {
                    actions.push(UiAction::PlayPause);
                    ui.close_menu();
                }
                if ui.button("  \u{23F9}  정지  (Esc)").clicked() {
                    actions.push(UiAction::Stop);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("  \u{23EA}  5초 뒤로  (\u{2190})").clicked() {
                    actions.push(UiAction::SeekBackward);
                    ui.close_menu();
                }
                if ui.button("  \u{23E9}  5초 앞으로  (\u{2192})").clicked() {
                    actions.push(UiAction::SeekForward);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(format!("  \u{23F2}  배속 감소  ([)  {:.2}x", state.speed)).clicked() {
                    actions.push(UiAction::SpeedDown);
                    ui.close_menu();
                }
                if ui.button(format!("  \u{23F1}  배속 증가  (])  {:.2}x", state.speed)).clicked() {
                    actions.push(UiAction::SpeedUp);
                    ui.close_menu();
                }
            });
            ui.menu_button("오디오", |ui| {
                if ui.button("  \u{1F50A}  볼륨 증가  (\u{2191})").clicked() {
                    actions.push(UiAction::VolumeUp);
                    ui.close_menu();
                }
                if ui.button("  \u{1F509}  볼륨 감소  (\u{2193})").clicked() {
                    actions.push(UiAction::VolumeDown);
                    ui.close_menu();
                }
                let mute_label = if state.muted {
                    "  \u{1F507}  음소거 해제  (M)"
                } else {
                    "  \u{1F508}  음소거  (M)"
                };
                if ui.button(mute_label).clicked() {
                    actions.push(UiAction::MuteToggle);
                    ui.close_menu();
                }
            });
            ui.menu_button("보기", |ui| {
                let info_label = if state.show_info_overlay {
                    "  \u{2139}  정보 숨기기  (Tab)"
                } else {
                    "  \u{2139}  정보 보기  (Tab)"
                };
                if ui.button(info_label).clicked() {
                    actions.push(UiAction::ToggleInfoOverlay);
                    ui.close_menu();
                }
                ui.separator();
                let dec_label = format!("  \u{1F3AC}  디코더 전환  (R)  [{}]", state.decode_mode);
                if ui.button(dec_label).clicked() {
                    actions.push(UiAction::ToggleDecoder);
                    ui.close_menu();
                }
            });
        });
    });

    // ========== Bottom control bar ==========
    egui::TopBottomPanel::bottom("control_bar").show(ctx, |ui| {
        // Seek bar
        if state.duration > 0.0 {
            let mut seek_pos = state.current_time as f32;
            let response = ui.add(
                egui::Slider::new(&mut seek_pos, 0.0..=state.duration as f32)
                    .show_value(false)
                    .trailing_fill(true)
            );
            if response.changed() {
                state.current_time = seek_pos as f64;
                actions.push(UiAction::None); // seek handled below
            }
            if response.drag_stopped() {
                // will be handled by the caller via state.current_time
            }
        }

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // Play/Pause
            let play_icon = match state.playback_state {
                PlaybackState::Playing => "\u{23F8}",
                _ => "\u{25B6}\u{FE0F}",
            };
            if ui.button(egui::RichText::new(play_icon).size(18.0)).clicked() {
                actions.push(UiAction::PlayPause);
            }

            // Stop
            if ui.button(egui::RichText::new("\u{23F9}").size(18.0)).clicked() {
                actions.push(UiAction::Stop);
            }

            ui.add_space(4.0);

            // Seek backward
            if ui.button(egui::RichText::new("\u{23EA}").size(16.0)).on_hover_text("5초 뒤로").clicked() {
                actions.push(UiAction::SeekBackward);
            }

            // Seek forward
            if ui.button(egui::RichText::new("\u{23E9}").size(16.0)).on_hover_text("5초 앞으로").clicked() {
                actions.push(UiAction::SeekForward);
            }

            ui.add_space(8.0);

            // Time
            if state.duration > 0.0 {
                ui.label(format!(
                    "{} / {}",
                    format_time(state.current_time),
                    format_time(state.duration)
                ));
            }

            // Right-aligned: volume + speed + decoder
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Decoder mode
                ui.label(egui::RichText::new(&state.decode_mode).small());
                ui.separator();

                // Speed
                ui.label(format!("{:.2}x", state.speed));
                if ui.button(egui::RichText::new("\u{23F1}").size(14.0)).on_hover_text("배속 증가 (])").clicked() {
                    actions.push(UiAction::SpeedUp);
                }
                if ui.button(egui::RichText::new("\u{23F2}").size(14.0)).on_hover_text("배속 감소 ([)").clicked() {
                    actions.push(UiAction::SpeedDown);
                }
                ui.separator();

                // Volume
                let vol_icon = if state.muted {
                    "\u{1F507}"
                } else if state.volume < 0.3 {
                    "\u{1F508}"
                } else if state.volume < 0.7 {
                    "\u{1F509}"
                } else {
                    "\u{1F50A}"
                };
                if ui.button(egui::RichText::new(vol_icon).size(16.0)).on_hover_text("음소거 (M)").clicked() {
                    actions.push(UiAction::MuteToggle);
                }

                let mut vol = state.volume as f32;
                let vol_resp = ui.add(
                    egui::Slider::new(&mut vol, 0.0..=2.0)
                        .show_value(false)
                        .fixed_decimals(0)
                );
                if vol_resp.changed() {
                    state.volume = vol as f64;
                    actions.push(UiAction::VolumeUp); // triggers set_volume
                }
                ui.label(format!("{:.0}%", state.volume * 100.0));
            });
        });
    });

    // ========== Context menu (right-click) ==========
    if state.show_context_menu {
        let pos = state.context_menu_pos;
        egui::Area::new(egui::Id::new("context_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);

                    if ui.button("  \u{1F4C2}  파일 열기...").clicked() {
                        actions.push(UiAction::OpenFile);
                        state.show_context_menu = false;
                    }
                    ui.separator();

                    let play_label = match state.playback_state {
                        PlaybackState::Playing => "  \u{23F8}  일시정지",
                        _ => "  \u{25B6}\u{FE0F}  재생",
                    };
                    if ui.button(play_label).clicked() {
                        actions.push(UiAction::PlayPause);
                        state.show_context_menu = false;
                    }
                    if ui.button("  \u{23F9}  정지").clicked() {
                        actions.push(UiAction::Stop);
                        state.show_context_menu = false;
                    }
                    ui.separator();

                    if ui.button("  \u{23EA}  5초 뒤로").clicked() {
                        actions.push(UiAction::SeekBackward);
                        state.show_context_menu = false;
                    }
                    if ui.button("  \u{23E9}  5초 앞으로").clicked() {
                        actions.push(UiAction::SeekForward);
                        state.show_context_menu = false;
                    }
                    ui.separator();

                    let vol_text = if state.muted {
                        "  \u{1F507}  음소거 해제".to_string()
                    } else {
                        format!("  \u{1F50A}  음소거  (볼륨 {:.0}%)", state.volume * 100.0)
                    };
                    if ui.button(vol_text).clicked() {
                        actions.push(UiAction::MuteToggle);
                        state.show_context_menu = false;
                    }
                    ui.separator();

                    let dec_text = format!("  \u{1F3AC}  디코더: {}", state.decode_mode);
                    if ui.button(dec_text).clicked() {
                        actions.push(UiAction::ToggleDecoder);
                        state.show_context_menu = false;
                    }

                    let info_text = if state.show_info_overlay {
                        "  \u{2139}  정보 숨기기"
                    } else {
                        "  \u{2139}  정보 보기"
                    };
                    if ui.button(info_text).clicked() {
                        actions.push(UiAction::ToggleInfoOverlay);
                        state.show_context_menu = false;
                    }
                });
            });

        // Close on click outside
        if ctx.input(|i| i.pointer.any_pressed()) {
            let ptr = ctx.input(|i| i.pointer.interact_pos().unwrap_or_default());
            // Check if click is outside the menu area (rough)
            let menu_rect = egui::Rect::from_min_size(pos, egui::vec2(200.0, 300.0));
            if !menu_rect.contains(ptr) {
                state.show_context_menu = false;
            }
        }
    }

    // ========== Info overlay (Tab) ==========
    if state.show_info_overlay {
        egui::Area::new(egui::Id::new("info_overlay"))
            .fixed_pos(egui::pos2(10.0, 36.0))
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
                    ui.label(egui::RichText::new(
                        format!("디코더: {} (R로 전환)", state.decode_mode)
                    ).monospace().size(14.0));
                });
            });
    }

    // ========== Subtitle overlay ==========
    if !state.subtitle_text.is_empty() {
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("subtitle"))
            .fixed_pos(egui::pos2(screen.center().x, screen.max.y - 100.0))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 180))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&state.subtitle_text)
                                .color(egui::Color32::WHITE)
                                .size(20.0)
                        );
                    });
            });
    }

    actions
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
                decode_mode: "SW".to_string(),
                show_context_menu: false,
                context_menu_pos: egui::Pos2::ZERO,
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
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.stop();
        }
        self.audio_output = None;

        match MediaPipeline::open(path, true) {
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

                if info.video_width > 0 && info.video_height > 0 {
                    let scale = (config::DEFAULT_HEIGHT as f64) / (info.video_height as f64);
                    let w = (info.video_width as f64 * scale) as u32;
                    let h = config::DEFAULT_HEIGHT;
                    let _ = self.window.request_inner_size(winit::dpi::LogicalSize::new(w, h));
                }

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

        self.update_decode_mode_display();

        let Some(pipeline) = &self.pipeline else { return };
        let Some(clock) = &mut self.clock else { return };

        let clock_time = clock.time();
        self.ui_state.current_time = clock_time;

        let frame_duration = if self.video_fps > 0.0 {
            1.0 / self.video_fps
        } else {
            1.0 / 30.0
        };

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

            if frame.pts_secs < 0.0 {
                continue;
            }

            let diff = frame.pts_secs - clock_time;

            if diff < -frame_duration * 2.0 {
                frames_dropped += 1;
                if frames_dropped > 30 {
                    self.video_renderer.upload_rgba_frame(
                        &self.device, &self.queue,
                        frame.width, frame.height, &frame.data,
                    );
                    break;
                }
                continue;
            } else if diff > config::SYNC_THRESHOLD_SECS {
                self.pending_frame = Some(frame);
                break;
            } else {
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

        if let Some(ref subtitle) = self.subtitle {
            if let Some(text) = subtitle.current_text(clock_time) {
                self.ui_state.subtitle_text = text.to_string();
            } else {
                self.ui_state.subtitle_text.clear();
            }
        }
    }

    /// Process a UI action
    pub fn handle_action(&mut self, action: &UiAction) {
        match action {
            UiAction::None => {}
            UiAction::OpenFile => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "ts", "m4v"])
                    .pick_file()
                {
                    self.open_file(&path.to_string_lossy());
                }
            }
            UiAction::PlayPause => {
                match self.ui_state.playback_state {
                    PlaybackState::Playing => {
                        self.ui_state.playback_state = PlaybackState::Paused;
                        if let Some(p) = &self.pipeline {
                            let _ = p.cmd_tx.send(PipelineCommand::Pause);
                        }
                        if let Some(ref audio) = self.audio_output {
                            audio.set_paused(true);
                        }
                    }
                    PlaybackState::Paused => {
                        self.ui_state.playback_state = PlaybackState::Playing;
                        if let Some(p) = &self.pipeline {
                            let _ = p.cmd_tx.send(PipelineCommand::Resume);
                        }
                        if let Some(ref audio) = self.audio_output {
                            audio.set_paused(false);
                        }
                    }
                    _ => {}
                }
            }
            UiAction::Stop => {
                if self.ui_state.playback_state != PlaybackState::Empty {
                    if let Some(pipeline) = self.pipeline.take() {
                        pipeline.stop();
                    }
                    self.audio_output = None;
                    self.ui_state.playback_state = PlaybackState::Stopped;
                    self.ui_state.current_time = 0.0;
                    self.window.set_title(config::APP_NAME);
                }
            }
            UiAction::SeekForward => {
                self.seek(self.ui_state.current_time + config::SEEK_STEP_SECS);
            }
            UiAction::SeekBackward => {
                self.seek(self.ui_state.current_time - config::SEEK_STEP_SECS);
            }
            UiAction::VolumeUp => {
                self.ui_state.volume = (self.ui_state.volume + config::VOLUME_STEP).min(config::MAX_VOLUME);
                if let Some(ref audio) = self.audio_output {
                    audio.set_volume(self.ui_state.volume);
                }
            }
            UiAction::VolumeDown => {
                self.ui_state.volume = (self.ui_state.volume - config::VOLUME_STEP).max(0.0);
                if let Some(ref audio) = self.audio_output {
                    audio.set_volume(self.ui_state.volume);
                }
            }
            UiAction::MuteToggle => {
                self.ui_state.muted = !self.ui_state.muted;
                if let Some(ref audio) = self.audio_output {
                    audio.set_muted(self.ui_state.muted);
                }
            }
            UiAction::SpeedUp => {
                self.ui_state.speed = (self.ui_state.speed + config::SPEED_STEP).min(config::MAX_SPEED);
                if let Some(ref mut clock) = self.clock {
                    clock.set_speed(self.ui_state.speed);
                }
            }
            UiAction::SpeedDown => {
                self.ui_state.speed = (self.ui_state.speed - config::SPEED_STEP).max(config::MIN_SPEED);
                if let Some(ref mut clock) = self.clock {
                    clock.set_speed(self.ui_state.speed);
                }
            }
            UiAction::ToggleDecoder => {
                self.toggle_decode_mode();
            }
            UiAction::ToggleInfoOverlay => {
                self.ui_state.show_info_overlay = !self.ui_state.show_info_overlay;
            }
        }
    }

    pub fn toggle_decode_mode(&mut self) {
        if let Some(p) = &self.pipeline {
            let current = p.current_decode_mode();
            let new_mode = match current {
                DecodeMode::Software => DecodeMode::Hardware,
                DecodeMode::Hardware => DecodeMode::Software,
            };
            let _ = p.cmd_tx.send(PipelineCommand::SetDecodeMode(new_mode));
        }
    }

    pub fn update_decode_mode_display(&mut self) {
        if let Some(p) = &self.pipeline {
            let mode = p.current_decode_mode();
            self.ui_state.decode_mode = match mode {
                DecodeMode::Hardware => "HW".to_string(),
                DecodeMode::Software => "SW".to_string(),
            };
        }
    }

    pub fn seek(&mut self, target: f64) {
        let target = target.clamp(0.0, self.ui_state.duration);
        if let Some(p) = &self.pipeline {
            let _ = p.cmd_tx.send(PipelineCommand::Seek(target));
        }
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
            let actions = draw_ui(ctx, &mut self.ui_state);
            // Store actions for processing after render
            // We use ctx.memory to stash them
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("ui_actions"), actions));
        });

        // Process UI actions
        let actions: Vec<UiAction> = self.egui_ctx.data_mut(|d| {
            d.get_temp(egui::Id::new("ui_actions")).unwrap_or_default()
        });
        for action in &actions {
            self.handle_action(action);
        }
        // Sync volume slider back to audio
        if let Some(ref audio) = self.audio_output {
            audio.set_volume(self.ui_state.volume);
        }

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
