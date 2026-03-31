mod ai;
mod app;
mod audio;
mod camera;
mod config;
mod db;
mod decode;
mod error;
mod media;
mod subtitle;
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
}

impl RPlayer {
    fn new() -> Self {
        Self { app: None }
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

        let app = pollster::block_on(App::new(window.clone()))
            .expect("Failed to initialize app");

        self.app = Some(app);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = self.app.as_mut() else { return };

        // Let egui handle the event first
        let response = app.egui_state.on_window_event(&app.window, &event);
        if response.consumed {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
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
                            log::info!("Open file dialog (TODO)");
                        }
                        PhysicalKey::Code(KeyCode::Space) => {
                            match app.ui_state.playback_state {
                                app::PlaybackState::Playing => {
                                    app.ui_state.playback_state = app::PlaybackState::Paused;
                                }
                                app::PlaybackState::Paused => {
                                    app.ui_state.playback_state = app::PlaybackState::Playing;
                                }
                                _ => {}
                            }
                        }
                        PhysicalKey::Code(KeyCode::Escape) => {
                            if app.ui_state.playback_state != app::PlaybackState::Empty {
                                app.ui_state.playback_state = app::PlaybackState::Stopped;
                            }
                        }
                        PhysicalKey::Code(KeyCode::ArrowUp) => {
                            app.ui_state.volume = (app.ui_state.volume + config::VOLUME_STEP).min(config::MAX_VOLUME);
                        }
                        PhysicalKey::Code(KeyCode::ArrowDown) => {
                            app.ui_state.volume = (app.ui_state.volume - config::VOLUME_STEP).max(0.0);
                        }
                        PhysicalKey::Code(KeyCode::BracketRight) => {
                            app.ui_state.speed = (app.ui_state.speed + config::SPEED_STEP).min(config::MAX_SPEED);
                        }
                        PhysicalKey::Code(KeyCode::BracketLeft) => {
                            app.ui_state.speed = (app.ui_state.speed - config::SPEED_STEP).max(config::MIN_SPEED);
                        }
                        PhysicalKey::Code(KeyCode::KeyM) => {
                            app.ui_state.muted = !app.ui_state.muted;
                        }
                        _ => {}
                    }
                }
                app.window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                log::info!("File dropped: {:?}", path);
                // TODO: open file
            }
            WindowEvent::RedrawRequested => {
                match app.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => app.resize(
                        app.surface_config.width,
                        app.surface_config.height,
                    ),
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => log::error!("Render error: {:?}", e),
                }

                // Keep requesting redraws when playing
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
