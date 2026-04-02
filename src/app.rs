use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use winit::window::Window;

use crate::audio::output::AudioOutput;
use crate::config;
use crate::decode::video_decoder::DecodeMode;
use crate::media::clock::Clock;
use crate::media::pipeline::{MediaPipeline, PipelineCommand};
use crate::subtitle::SubtitleTrack;
use crate::video::renderer::{RawFrame, VideoRenderer};

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
    SeekTo(f64),
    FrameStep,
    ToggleLibraryInfo,
}

pub struct UiState {
    pub playback_state: PlaybackState,
    pub render_fps: f64,
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
    pub show_library_info: bool,
    pub context_menu_pos: egui::Pos2,
    pub frames_dropped: u64,
    pub frames_displayed: u64,
    pub recent_drop_rate: f64, // rolling 300-frame window
    // Audio DSP
    pub eq_bass: f32,
    pub eq_mid: f32,
    pub eq_treble: f32,
    pub compressor_enabled: bool,
    // Audio visualization
    pub audio_peak_l: f32,
    pub audio_peak_r: f32,
    pub audio_waveform: Vec<f32>,
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
    pub pending_frame: Option<RawFrame>,
    video_fps: f64,
    fps_counter: u32,
    fps_last_time: std::time::Instant,
    frame_history: std::collections::VecDeque<bool>, // true=displayed, false=dropped (last 300)

    // Subtitle
    pub subtitle: Option<SubtitleTrack>,

    // Window
    pub window: Arc<Window>,
    pub video_size: Option<(u32, u32)>,
}

/// Draw a clickable icon button with custom vector graphics
fn icon_button(
    ui: &mut egui::Ui,
    draw: impl FnOnce(&egui::Painter, egui::Rect, egui::Color32),
) -> egui::Response {
    let size = egui::vec2(24.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let color = if resp.hovered() {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_gray(200)
    };
    // Button background on hover
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_gray(60));
    }
    draw(ui.painter(), rect, color);
    resp
}

fn draw_ui(ctx: &egui::Context, state: &mut UiState) -> Vec<UiAction> {
    let mut actions = Vec::new();

    // Continuously repaint while menus/context are active
    if state.show_context_menu {
        ctx.request_repaint();
    }

    // ========== Top menu bar ==========
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("파일", |ui| {
                if ui.button("열기...        O").clicked() {
                    actions.push(UiAction::OpenFile);
                    ui.close_menu();
                }
            });
            ui.menu_button("재생", |ui| {
                let play_label = match state.playback_state {
                    PlaybackState::Playing => "|| 일시정지    Space",
                    _ =>                      "|> 재생        Space",
                };
                if ui.button(play_label).clicked() {
                    actions.push(UiAction::PlayPause);
                    ui.close_menu();
                }
                if ui.button("[] 정지        Esc").clicked() {
                    actions.push(UiAction::Stop);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("<< 5초 뒤로").clicked() {
                    actions.push(UiAction::SeekBackward);
                    ui.close_menu();
                }
                if ui.button(">> 5초 앞으로").clicked() {
                    actions.push(UiAction::SeekForward);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(format!("느리게  [     {:.2}x", state.speed)).clicked() {
                    actions.push(UiAction::SpeedDown);
                    ui.close_menu();
                }
                if ui.button(format!("빠르게  ]     {:.2}x", state.speed)).clicked() {
                    actions.push(UiAction::SpeedUp);
                    ui.close_menu();
                }
            });
            ui.menu_button("오디오", |ui| {
                if ui.button("볼륨 +5%").clicked() {
                    actions.push(UiAction::VolumeUp);
                    ui.close_menu();
                }
                if ui.button("볼륨 -5%").clicked() {
                    actions.push(UiAction::VolumeDown);
                    ui.close_menu();
                }
                let mute_label = if state.muted { "음소거 해제  M" } else { "음소거      M" };
                if ui.button(mute_label).clicked() {
                    actions.push(UiAction::MuteToggle);
                    ui.close_menu();
                }
                ui.separator();
                ui.label("이퀄라이저");
                ui.horizontal(|ui| {
                    ui.label("Bass");
                    ui.add(egui::DragValue::new(&mut state.eq_bass).range(-12.0..=12.0).speed(0.5).suffix("dB"));
                });
                ui.horizontal(|ui| {
                    ui.label("Mid ");
                    ui.add(egui::DragValue::new(&mut state.eq_mid).range(-12.0..=12.0).speed(0.5).suffix("dB"));
                });
                ui.horizontal(|ui| {
                    ui.label("Treble");
                    ui.add(egui::DragValue::new(&mut state.eq_treble).range(-12.0..=12.0).speed(0.5).suffix("dB"));
                });
                ui.separator();
                ui.checkbox(&mut state.compressor_enabled, "컴프레서");
            });
            ui.menu_button("보기", |ui| {
                let info_label = if state.show_info_overlay { "정보 숨기기  Tab" } else { "정보 보기    Tab" };
                if ui.button(info_label).clicked() {
                    actions.push(UiAction::ToggleInfoOverlay);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(format!("디코더 전환  R  [{}]", state.decode_mode)).clicked() {
                    actions.push(UiAction::ToggleDecoder);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("라이브러리 정보...").clicked() {
                    actions.push(UiAction::ToggleLibraryInfo);
                    ui.close_menu();
                }
            });
        });
    });

    // ========== Bottom control bar ==========
    egui::TopBottomPanel::bottom("control_bar").show(ctx, |ui| {
        // Seek bar — full-width custom bar
        if state.duration > 0.0 {
            let progress = (state.current_time / state.duration).clamp(0.0, 1.0) as f32;
            let bar_height = 12.0;
            let available = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(available, bar_height),
                egui::Sense::click_and_drag(),
            );

            // Draw background
            ui.painter().rect_filled(
                rect,
                2.0,
                egui::Color32::from_gray(60),
            );
            // Draw filled portion
            let filled_rect = egui::Rect::from_min_max(
                rect.min,
                egui::pos2(rect.min.x + rect.width() * progress, rect.max.y),
            );
            ui.painter().rect_filled(
                filled_rect,
                2.0,
                egui::Color32::from_rgb(80, 160, 255),
            );

            // Handle click / drag
            if resp.dragged() || resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let ratio = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                    state.current_time = ratio as f64 * state.duration;
                }
            }
            if resp.drag_stopped() || resp.clicked() {
                actions.push(UiAction::SeekTo(state.current_time));
            }

            // Hover tooltip with time
            if resp.hovered() {
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let ratio = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                    let hover_time = ratio as f64 * state.duration;
                    resp.on_hover_text(format_time(hover_time));
                }
            }
        }

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // Play/Pause icon button
            if icon_button(ui, |p, r, c| {
                if state.playback_state == PlaybackState::Playing {
                    // Pause: two vertical bars
                    let w = r.width() * 0.25;
                    let pad = r.width() * 0.2;
                    p.rect_filled(egui::Rect::from_min_size(
                        egui::pos2(r.min.x + pad, r.min.y + pad), egui::vec2(w, r.height() - pad * 2.0)
                    ), 0.0, c);
                    p.rect_filled(egui::Rect::from_min_size(
                        egui::pos2(r.max.x - pad - w, r.min.y + pad), egui::vec2(w, r.height() - pad * 2.0)
                    ), 0.0, c);
                } else {
                    // Play: right-pointing triangle
                    let pad = r.width() * 0.2;
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(r.min.x + pad, r.min.y + pad),
                            egui::pos2(r.max.x - pad * 0.5, r.center().y),
                            egui::pos2(r.min.x + pad, r.max.y - pad),
                        ],
                        c, egui::Stroke::NONE,
                    ));
                }
            }).clicked() {
                actions.push(UiAction::PlayPause);
            }

            // Stop icon button: filled square
            if icon_button(ui, |p, r, c| {
                let pad = r.width() * 0.25;
                p.rect_filled(egui::Rect::from_min_max(
                    egui::pos2(r.min.x + pad, r.min.y + pad),
                    egui::pos2(r.max.x - pad, r.max.y - pad),
                ), 1.0, c);
            }).clicked() {
                actions.push(UiAction::Stop);
            }

            ui.add_space(2.0);

            // Rewind icon: two left triangles
            if icon_button(ui, |p, r, c| {
                let pad = r.width() * 0.15;
                let mid = r.center().x;
                for offset in [0.0, r.width() * 0.3] {
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(mid - offset, r.center().y),
                            egui::pos2(mid - offset + r.width() * 0.35, r.min.y + pad),
                            egui::pos2(mid - offset + r.width() * 0.35, r.max.y - pad),
                        ],
                        c, egui::Stroke::NONE,
                    ));
                }
            }).on_hover_text("5초 뒤로").clicked() {
                actions.push(UiAction::SeekBackward);
            }

            // Forward icon: two right triangles
            if icon_button(ui, |p, r, c| {
                let pad = r.width() * 0.15;
                let mid = r.center().x;
                for offset in [0.0, r.width() * 0.3] {
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(mid + offset, r.center().y),
                            egui::pos2(mid + offset - r.width() * 0.35, r.min.y + pad),
                            egui::pos2(mid + offset - r.width() * 0.35, r.max.y - pad),
                        ],
                        c, egui::Stroke::NONE,
                    ));
                }
            }).on_hover_text("5초 앞으로").clicked() {
                actions.push(UiAction::SeekForward);
            }

            ui.add_space(4.0);

            if state.duration > 0.0 {
                ui.label(format!(
                    "{} / {}",
                    format_time(state.current_time),
                    format_time(state.duration)
                ));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(&state.decode_mode).small());
                ui.separator();

                ui.label(format!("{:.2}x", state.speed));
                if ui.button("+").on_hover_text("배속 증가 ]").clicked() {
                    actions.push(UiAction::SpeedUp);
                }
                if ui.button("-").on_hover_text("배속 감소 [").clicked() {
                    actions.push(UiAction::SpeedDown);
                }
                ui.separator();

                // Volume icon: speaker shape
                if icon_button(ui, |p, r, c| {
                    if state.muted {
                        // X mark
                        let pad = r.width() * 0.25;
                        let s = egui::Stroke::new(2.0, c);
                        p.line_segment([egui::pos2(r.min.x + pad, r.min.y + pad), egui::pos2(r.max.x - pad, r.max.y - pad)], s);
                        p.line_segment([egui::pos2(r.max.x - pad, r.min.y + pad), egui::pos2(r.min.x + pad, r.max.y - pad)], s);
                    } else {
                        // Speaker: rectangle + triangle horn
                        let cx = r.center().x - r.width() * 0.1;
                        let cy = r.center().y;
                        let h = r.height() * 0.25;
                        let w = r.width() * 0.15;
                        p.rect_filled(egui::Rect::from_center_size(
                            egui::pos2(cx - w, cy), egui::vec2(w, h * 2.0)
                        ), 0.0, c);
                        p.add(egui::Shape::convex_polygon(
                            vec![
                                egui::pos2(cx - w * 0.5, cy - h),
                                egui::pos2(cx + w * 2.0, cy - h * 2.0),
                                egui::pos2(cx + w * 2.0, cy + h * 2.0),
                                egui::pos2(cx - w * 0.5, cy + h),
                            ],
                            c, egui::Stroke::NONE,
                        ));
                        // Sound waves (arcs)
                        if state.volume > 0.3 {
                            let s = egui::Stroke::new(1.5, c);
                            p.line_segment([egui::pos2(cx + w * 3.0, cy - h), egui::pos2(cx + w * 3.5, cy)], s);
                            p.line_segment([egui::pos2(cx + w * 3.5, cy), egui::pos2(cx + w * 3.0, cy + h)], s);
                        }
                    }
                }).on_hover_text("음소거 M").clicked() {
                    actions.push(UiAction::MuteToggle);
                }

                let mut vol = state.volume as f32;
                let vol_resp = ui.add(
                    egui::Slider::new(&mut vol, 0.0..=2.0)
                        .show_value(false)
                );
                if vol_resp.changed() {
                    state.volume = vol as f64;
                    actions.push(UiAction::VolumeUp);
                }
                ui.label(format!("{:.0}%", state.volume * 100.0));
            });
        });
    });

    // ========== Context menu (right-click / two-finger tap) ==========
    if state.show_context_menu {
        let pos = state.context_menu_pos;
        egui::Area::new(egui::Id::new("context_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(160.0);

                    if ui.button("파일 열기...").clicked() {
                        actions.push(UiAction::OpenFile);
                        state.show_context_menu = false;
                    }
                    ui.separator();

                    let play_label = match state.playback_state {
                        PlaybackState::Playing => "일시정지",
                        _ => "재생",
                    };
                    if ui.button(play_label).clicked() {
                        actions.push(UiAction::PlayPause);
                        state.show_context_menu = false;
                    }
                    if ui.button("정지").clicked() {
                        actions.push(UiAction::Stop);
                        state.show_context_menu = false;
                    }
                    ui.separator();

                    if ui.button("5초 뒤로").clicked() {
                        actions.push(UiAction::SeekBackward);
                        state.show_context_menu = false;
                    }
                    if ui.button("5초 앞으로").clicked() {
                        actions.push(UiAction::SeekForward);
                        state.show_context_menu = false;
                    }
                    ui.separator();

                    let vol_text = if state.muted {
                        "음소거 해제".to_string()
                    } else {
                        format!("음소거 ({}%)", (state.volume * 100.0) as i32)
                    };
                    if ui.button(vol_text).clicked() {
                        actions.push(UiAction::MuteToggle);
                        state.show_context_menu = false;
                    }
                    ui.separator();

                    if ui.button(format!("디코더: {}", state.decode_mode)).clicked() {
                        actions.push(UiAction::ToggleDecoder);
                        state.show_context_menu = false;
                    }
                    let info_text = if state.show_info_overlay { "정보 숨기기" } else { "정보 보기" };
                    if ui.button(info_text).clicked() {
                        actions.push(UiAction::ToggleInfoOverlay);
                        state.show_context_menu = false;
                    }
                });
            });

        // Close on left-click outside
        if ctx.input(|i| i.pointer.primary_pressed()) {
            state.show_context_menu = false;
        }
    }

    // ========== Info overlay (Tab) ==========
    if state.show_info_overlay {
        egui::Area::new(egui::Id::new("info_overlay"))
            .fixed_pos(egui::pos2(10.0, 36.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new(
                        format!("{} | render {:.0}fps", state.video_info, state.render_fps)
                    ).monospace().size(14.0));
                    ui.label(egui::RichText::new(
                        format!("{} / {}", format_time(state.current_time), format_time(state.duration))
                    ).monospace().size(14.0));
                    ui.label(egui::RichText::new(
                        format!("{:.2}x | {:.0}%{}",
                            state.speed,
                            state.volume * 100.0,
                            if state.muted { " (mute)" } else { "" }
                        )
                    ).monospace().size(14.0));
                    ui.label(egui::RichText::new(
                        format!("decoder: {}", state.decode_mode)
                    ).monospace().size(14.0));
                    let drop_rate = state.recent_drop_rate;
                    let drop_color = if drop_rate > 5.0 {
                        egui::Color32::from_rgb(255, 80, 80)
                    } else if drop_rate > 1.0 {
                        egui::Color32::from_rgb(255, 200, 80)
                    } else {
                        egui::Color32::from_rgb(80, 255, 80)
                    };
                    ui.label(egui::RichText::new(
                        format!("frames: {} ok / {} drop ({:.1}%)",
                            state.frames_displayed, state.frames_dropped, drop_rate)
                    ).monospace().size(14.0).color(drop_color));
                });
            });

        // Audio visualizer — top right
        let screen = ctx.screen_rect();
        let vis_w = 220.0;
        egui::Area::new(egui::Id::new("audio_vis"))
            .fixed_pos(egui::pos2(screen.max.x - vis_w - 10.0, 36.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(vis_w);

                    // Level meters
                    ui.label(egui::RichText::new("Audio").monospace().size(12.0));

                    let meter_h = 10.0;
                    let meter_w = vis_w - 30.0;

                    // L channel
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("L").monospace().size(11.0));
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(meter_w, meter_h), egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_gray(40));
                        let level = state.audio_peak_l.clamp(0.0, 1.0);
                        let fill = egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(rect.min.x + rect.width() * level, rect.max.y),
                        );
                        let color = if level > 0.9 {
                            egui::Color32::from_rgb(255, 60, 60)
                        } else if level > 0.6 {
                            egui::Color32::from_rgb(255, 200, 60)
                        } else {
                            egui::Color32::from_rgb(60, 200, 60)
                        };
                        ui.painter().rect_filled(fill, 2.0, color);
                    });

                    // R channel
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("R").monospace().size(11.0));
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(meter_w, meter_h), egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_gray(40));
                        let level = state.audio_peak_r.clamp(0.0, 1.0);
                        let fill = egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(rect.min.x + rect.width() * level, rect.max.y),
                        );
                        let color = if level > 0.9 {
                            egui::Color32::from_rgb(255, 60, 60)
                        } else if level > 0.6 {
                            egui::Color32::from_rgb(255, 200, 60)
                        } else {
                            egui::Color32::from_rgb(60, 200, 60)
                        };
                        ui.painter().rect_filled(fill, 2.0, color);
                    });

                    ui.add_space(4.0);

                    // PCM Waveform oscilloscope
                    ui.label(egui::RichText::new("Waveform").monospace().size(12.0));
                    let wave_h = 60.0;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(vis_w, wave_h), egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, 2.0, egui::Color32::from_gray(20));

                    let waveform = &state.audio_waveform;
                    if waveform.len() > 2 {
                        let n = waveform.len().min(512);
                        let start = waveform.len().saturating_sub(n);
                        let samples = &waveform[start..];
                        let step = samples.len() as f32 / rect.width();
                        let mid_y = rect.center().y;

                        let points: Vec<egui::Pos2> = (0..rect.width() as usize).map(|x| {
                            let idx = (x as f32 * step) as usize;
                            let val = samples.get(idx).copied().unwrap_or(0.0);
                            let y = mid_y - val * (wave_h * 0.45);
                            egui::pos2(rect.min.x + x as f32, y)
                        }).collect();

                        if points.len() > 1 {
                            ui.painter().add(egui::Shape::line(
                                points,
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 200, 255)),
                            ));
                        }

                        // Center line
                        ui.painter().line_segment(
                            [egui::pos2(rect.min.x, mid_y), egui::pos2(rect.max.x, mid_y)],
                            egui::Stroke::new(0.5, egui::Color32::from_gray(60)),
                        );
                    }
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

    // ========== Library info window ==========
    if state.show_library_info {
        let mut open = state.show_library_info;
        egui::Window::new("라이브러리 정보")
            .open(&mut open)
            .resizable(true)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(format!(
                    "rPlayer v{}", env!("CARGO_PKG_VERSION")
                )).strong().size(16.0));
                ui.label("Rust 크로스플랫폼 비디오 플레이어");
                ui.separator();

                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    ui.label(egui::RichText::new("핵심 라이브러리").strong());
                    draw_lib_table(ui, &[
                        ("ffmpeg-next", "8.1.0", "FFmpeg 바인딩 — 비디오/오디오 디코딩"),
                        ("wgpu", "24.0.5", "WebGPU 렌더링 — Metal/DX12/Vulkan"),
                        ("winit", "0.30.13", "크로스플랫폼 윈도우 관리"),
                        ("egui", "0.31.1", "즉시 모드 UI 프레임워크"),
                        ("egui-wgpu", "0.31.1", "egui + wgpu 통합 렌더러"),
                        ("egui-winit", "0.31.1", "egui + winit 입력 통합"),
                        ("cpal", "0.15.3", "크로스플랫폼 오디오 출력"),
                    ]);

                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("미디어 처리").strong());
                    draw_lib_table(ui, &[
                        ("rubato", "0.16.2", "오디오 리샘플링 (피치 보존)"),
                        ("rusqlite", "0.32.1", "SQLite 데이터베이스 (bundled)"),
                        ("rfd", "0.15.4", "네이티브 파일 대화상자"),
                        ("sysinfo", "0.33.1", "시스템 리소스 모니터링"),
                    ]);

                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("유틸리티").strong());
                    draw_lib_table(ui, &[
                        ("crossbeam-channel", "0.5.15", "스레드 간 채널 통신"),
                        ("parking_lot", "0.12.5", "고성능 뮤텍스"),
                        ("bytemuck", "1.25.0", "안전한 바이트 캐스팅"),
                        ("anyhow", "1.0.102", "에러 처리"),
                        ("thiserror", "2.0.18", "에러 타입 정의"),
                        ("log", "0.4.29", "로깅 인터페이스"),
                        ("env_logger", "0.11.10", "환경변수 로그 설정"),
                        ("pollster", "0.4.0", "async 블로킹 실행"),
                        ("ringbuf", "0.4.8", "링 버퍼"),
                    ]);

                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("시스템 프레임워크").strong().size(13.0));
                    #[cfg(target_os = "macos")]
                    ui.label("VideoToolbox, CoreAudio, Metal, AppKit, CoreML, Vision, AVFoundation, SceneKit");
                    #[cfg(target_os = "windows")]
                    ui.label("D3D11VA, WASAPI, Direct3D 12, Win32");
                    #[cfg(target_os = "linux")]
                    ui.label("VAAPI, ALSA, Vulkan, X11/Wayland");
                });
            });
        state.show_library_info = open;
    }

    actions
}

fn draw_lib_table(ui: &mut egui::Ui, libs: &[(&str, &str, &str)]) {
    egui::Grid::new(ui.next_auto_id())
        .num_columns(3)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for &(name, ver, desc) in libs {
                ui.label(egui::RichText::new(name).monospace().strong().size(13.0));
                ui.label(egui::RichText::new(ver).monospace().size(12.0).color(egui::Color32::from_gray(160)));
                ui.label(egui::RichText::new(desc).size(12.0));
                ui.end_row();
            }
        });
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Try loading system Korean font
    let font_paths = [
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",           // macOS
        "/System/Library/Fonts/Supplemental/AppleGothic.ttf",   // macOS fallback
        "C:\\Windows\\Fonts\\malgun.ttf",                        // Windows
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];

    let mut loaded = false;
    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert(
                "korean".to_owned(),
                egui::FontData::from_owned(data).into(),
            );
            // Prepend to proportional and monospace families
            fonts.families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "korean".to_owned());
            fonts.families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("korean".to_owned());
            loaded = true;
            log::info!("Loaded Korean font: {}", path);
            break;
        }
    }

    if !loaded {
        log::warn!("Korean font not found, UI text may not render correctly");
    }

    ctx.set_fonts(fonts);
}

fn format_time(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 { return "0:00".to_string(); }
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
        // Prefer non-sRGB to avoid double gamma on YUV video
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| !f.is_srgb())
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
        configure_fonts(&egui_ctx);

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
                render_fps: 0.0,
                show_context_menu: false,
                show_library_info: false,
                context_menu_pos: egui::Pos2::ZERO,
                frames_dropped: 0,
                frames_displayed: 0,
                recent_drop_rate: 0.0,
                eq_bass: 0.0,
                eq_mid: 0.0,
                eq_treble: 0.0,
                compressor_enabled: false,
                audio_peak_l: 0.0,
                audio_peak_r: 0.0,
                audio_waveform: Vec::new(),
            },
            pipeline: None,
            audio_output: None,
            clock: None,
            pending_frame: None,
            video_fps: 0.0,
            fps_counter: 0,
            fps_last_time: std::time::Instant::now(),
            frame_history: std::collections::VecDeque::with_capacity(300),
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

        let is_frozen = clock.is_frozen();
        let clock_time = clock.time();
        self.ui_state.current_time = clock_time;

        let mut displayed = false;

        // Take exactly one frame per render — 1:1 mapping with display refresh
        let frame = match pipeline.frame_rx.try_recv() {
            Ok(f) => Some(f),
            Err(crossbeam_channel::TryRecvError::Empty) => None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.ui_state.playback_state = PlaybackState::Stopped;
                log::info!("Playback finished");
                return;
            }
        };

        if let Some(frame) = frame {
            if !frame.planes.is_empty() && frame.pts_secs >= 0.0 {
                self.video_renderer.upload_frame(&self.device, &self.queue, &frame);
                displayed = true;
                self.ui_state.frames_displayed += 1;
            }
        }

        // Unfreeze clock after first frame is displayed post-seek
        if is_frozen && displayed {
            if let Some(ref mut clock) = self.clock {
                clock.unfreeze();
            }
            // Resume audio now that video is ready
            if let Some(ref audio) = self.audio_output {
                audio.set_paused(false);
            }
        }

        if let Some(ref subtitle) = self.subtitle {
            if let Some(text) = subtitle.current_text(clock_time) {
                self.ui_state.subtitle_text = text.to_string();
            } else {
                self.ui_state.subtitle_text.clear();
            }
        }

        // Rolling frame drop rate (300-frame window)
        self.frame_history.push_back(displayed);
        if self.frame_history.len() > 300 {
            self.frame_history.pop_front();
        }
        if !self.frame_history.is_empty() {
            let drops = self.frame_history.iter().filter(|&&ok| !ok).count();
            self.ui_state.recent_drop_rate = drops as f64 / self.frame_history.len() as f64 * 100.0;
        }

        // Update audio visualization data
        if let Some(ref audio) = self.audio_output {
            if let Some(vis) = audio.vis.try_lock() {
                self.ui_state.audio_peak_l = vis.peak_l;
                self.ui_state.audio_peak_r = vis.peak_r;
                if self.ui_state.show_info_overlay {
                    // Swap instead of clone to avoid allocation
                    let mut waveform = std::mem::take(&mut self.ui_state.audio_waveform);
                    waveform.clear();
                    let n = vis.waveform.len().min(2048);
                    let start = vis.waveform.len().saturating_sub(n);
                    waveform.extend_from_slice(&vis.waveform[start..]);
                    self.ui_state.audio_waveform = waveform;
                }
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
            UiAction::SeekTo(target) => {
                self.seek(*target);
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
                if let Some(ref audio) = self.audio_output {
                    audio.set_speed(self.ui_state.speed);
                }
            }
            UiAction::SpeedDown => {
                self.ui_state.speed = (self.ui_state.speed - config::SPEED_STEP).max(config::MIN_SPEED);
                if let Some(ref mut clock) = self.clock {
                    clock.set_speed(self.ui_state.speed);
                }
                if let Some(ref audio) = self.audio_output {
                    audio.set_speed(self.ui_state.speed);
                }
            }
            UiAction::ToggleDecoder => {
                self.toggle_decode_mode();
            }
            UiAction::ToggleInfoOverlay => {
                self.ui_state.show_info_overlay = !self.ui_state.show_info_overlay;
            }
            UiAction::ToggleLibraryInfo => {
                self.ui_state.show_library_info = !self.ui_state.show_library_info;
            }
            UiAction::FrameStep => {
                // Pause + advance one frame
                if self.ui_state.playback_state == PlaybackState::Playing {
                    self.handle_action(&UiAction::PlayPause);
                }
                // Take one frame from queue and display
                if let Some(ref pipeline) = self.pipeline {
                    if let Ok(frame) = pipeline.frame_rx.try_recv() {
                        if !frame.planes.is_empty() && frame.pts_secs >= 0.0 {
                            self.ui_state.current_time = frame.pts_secs;
                            self.video_renderer.upload_frame(&self.device, &self.queue, &frame);
                            self.ui_state.frames_displayed += 1;
                        }
                    }
                }
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
        // Pause audio immediately to prevent sound before video
        if let Some(ref audio) = self.audio_output {
            audio.set_paused(true);
            audio.flush();
        }
        if let Some(p) = &self.pipeline {
            let _ = p.cmd_tx.send(PipelineCommand::Seek(target));
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
        // Sync UI values to audio DSP
        if let Some(ref audio) = self.audio_output {
            audio.set_volume(self.ui_state.volume);
            audio.set_eq(self.ui_state.eq_bass, self.ui_state.eq_mid, self.ui_state.eq_treble);
            audio.set_compressor(self.ui_state.compressor_enabled, -10.0, 4.0);
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

        // FPS counter
        self.fps_counter += 1;
        let elapsed = self.fps_last_time.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            self.ui_state.render_fps = self.fps_counter as f64 / elapsed;
            log::debug!("render {:.1} fps", self.ui_state.render_fps);
            self.fps_counter = 0;
            self.fps_last_time = std::time::Instant::now();
        }

        Ok(())
    }
}
