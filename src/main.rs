#[allow(dead_code)]
mod ai;
mod app;
mod audio;
#[allow(dead_code)]
mod camera;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod db;
mod decode;
#[allow(dead_code)]
mod error;
mod media;
mod subtitle;
#[allow(dead_code)]
mod ui;
mod video;

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use app::App;

struct RPlayer {
    app: Option<App>,
    pending_file: Option<String>,
}

impl RPlayer {
    fn new() -> Self {
        let file = std::env::args().nth(1);
        Self {
            app: None,
            pending_file: file,
        }
    }
}

impl ApplicationHandler for RPlayer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title(config::APP_NAME)
            .with_inner_size(winit::dpi::LogicalSize::new(
                config::DEFAULT_WIDTH,
                config::DEFAULT_HEIGHT,
            ));

        let window = Arc::new(event_loop.create_window(window_attrs).expect("Failed to create window"));

        let mut app = pollster::block_on(App::new(window.clone()))
            .expect("Failed to initialize app");

        // Open file from command line argument
        if let Some(path) = self.pending_file.take() {
            app.open_file(&path);
        }

        self.app = Some(app);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = self.app.as_mut() else { return };

        let response = app.egui_state.on_window_event(&app.window, &event);
        if response.consumed {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                if let Some(pipeline) = app.pipeline.take() {
                    pipeline.stop();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                app.resize(size.width, size.height);
                app.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyO) => {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Video", &["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "ts", "m4v"])
                                .pick_file()
                            {
                                app.open_file(&path.to_string_lossy());
                            }
                        }
                        PhysicalKey::Code(KeyCode::Space) => {
                            match app.ui_state.playback_state {
                                app::PlaybackState::Playing => {
                                    app.ui_state.playback_state = app::PlaybackState::Paused;
                                    if let Some(p) = &app.pipeline {
                                        let _ = p.cmd_tx.send(media::pipeline::PipelineCommand::Pause);
                                    }
                                    if let Some(ref audio) = app.audio_output {
                                        audio.set_paused(true);
                                    }
                                }
                                app::PlaybackState::Paused => {
                                    app.ui_state.playback_state = app::PlaybackState::Playing;
                                    if let Some(p) = &app.pipeline {
                                        let _ = p.cmd_tx.send(media::pipeline::PipelineCommand::Resume);
                                    }
                                    if let Some(ref audio) = app.audio_output {
                                        audio.set_paused(false);
                                    }
                                }
                                _ => {}
                            }
                        }
                        PhysicalKey::Code(KeyCode::Escape) => {
                            if app.ui_state.playback_state != app::PlaybackState::Empty {
                                if let Some(pipeline) = app.pipeline.take() {
                                    pipeline.stop();
                                }
                                app.audio_output = None;
                                app.ui_state.playback_state = app::PlaybackState::Stopped;
                                app.ui_state.current_time = 0.0;
                                app.window.set_title(config::APP_NAME);
                            }
                        }
                        PhysicalKey::Code(KeyCode::ArrowUp) => {
                            app.ui_state.volume = (app.ui_state.volume + config::VOLUME_STEP).min(config::MAX_VOLUME);
                            if let Some(ref audio) = app.audio_output {
                                audio.set_volume(app.ui_state.volume);
                            }
                        }
                        PhysicalKey::Code(KeyCode::ArrowDown) => {
                            app.ui_state.volume = (app.ui_state.volume - config::VOLUME_STEP).max(0.0);
                            if let Some(ref audio) = app.audio_output {
                                audio.set_volume(app.ui_state.volume);
                            }
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            let target = app.ui_state.current_time + config::SEEK_STEP_SECS;
                            app.seek(target);
                        }
                        PhysicalKey::Code(KeyCode::ArrowLeft) => {
                            let target = app.ui_state.current_time - config::SEEK_STEP_SECS;
                            app.seek(target);
                        }
                        PhysicalKey::Code(KeyCode::BracketRight) => {
                            app.ui_state.speed = (app.ui_state.speed + config::SPEED_STEP).min(config::MAX_SPEED);
                            if let Some(ref mut clock) = app.clock {
                                clock.set_speed(app.ui_state.speed);
                            }
                        }
                        PhysicalKey::Code(KeyCode::BracketLeft) => {
                            app.ui_state.speed = (app.ui_state.speed - config::SPEED_STEP).max(config::MIN_SPEED);
                            if let Some(ref mut clock) = app.clock {
                                clock.set_speed(app.ui_state.speed);
                            }
                        }
                        PhysicalKey::Code(KeyCode::KeyM) => {
                            app.ui_state.muted = !app.ui_state.muted;
                            if let Some(ref audio) = app.audio_output {
                                audio.set_muted(app.ui_state.muted);
                            }
                        }
                        PhysicalKey::Code(KeyCode::Tab) => {
                            app.ui_state.show_info_overlay = !app.ui_state.show_info_overlay;
                        }
                        PhysicalKey::Code(KeyCode::Equal) => {
                            // + key: subtitle sync forward
                            if let Some(ref mut sub) = app.subtitle {
                                sub.adjust_sync(0.5);
                            }
                        }
                        PhysicalKey::Code(KeyCode::Minus) => {
                            // - key: subtitle sync backward
                            if let Some(ref mut sub) = app.subtitle {
                                sub.adjust_sync(-0.5);
                            }
                        }
                        PhysicalKey::Code(KeyCode::KeyR) => {
                            app.toggle_decode_mode();
                        }
                        _ => {}
                    }
                }
                app.window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                app.open_file(&path.to_string_lossy());
            }
            WindowEvent::RedrawRequested => {
                app.update_frame();

                match app.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => app.resize(
                        app.surface_config.width,
                        app.surface_config.height,
                    ),
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => log::error!("Render error: {:?}", e),
                }

                if app.ui_state.playback_state == app::PlaybackState::Playing {
                    app.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut player = RPlayer::new();
    event_loop.run_app(&mut player).expect("Event loop error");
}
