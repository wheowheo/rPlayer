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
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use app::{App, UiAction};

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
            app.window.request_redraw();
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
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                // Right-click context menu
                if let Some(pos) = app.egui_ctx.input(|i| i.pointer.latest_pos()) {
                    app.ui_state.show_context_menu = true;
                    app.ui_state.context_menu_pos = pos;
                }
                app.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    let action = match event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyO) => UiAction::OpenFile,
                        PhysicalKey::Code(KeyCode::Space) => UiAction::PlayPause,
                        PhysicalKey::Code(KeyCode::Escape) => UiAction::Stop,
                        PhysicalKey::Code(KeyCode::ArrowRight) => UiAction::SeekForward,
                        PhysicalKey::Code(KeyCode::ArrowLeft) => UiAction::SeekBackward,
                        PhysicalKey::Code(KeyCode::ArrowUp) => UiAction::VolumeUp,
                        PhysicalKey::Code(KeyCode::ArrowDown) => UiAction::VolumeDown,
                        PhysicalKey::Code(KeyCode::BracketRight) => UiAction::SpeedUp,
                        PhysicalKey::Code(KeyCode::BracketLeft) => UiAction::SpeedDown,
                        PhysicalKey::Code(KeyCode::KeyM) => UiAction::MuteToggle,
                        PhysicalKey::Code(KeyCode::Tab) => UiAction::ToggleInfoOverlay,
                        PhysicalKey::Code(KeyCode::KeyR) => UiAction::ToggleDecoder,
                        PhysicalKey::Code(KeyCode::Equal) => {
                            if let Some(ref mut sub) = app.subtitle {
                                sub.adjust_sync(0.5);
                            }
                            UiAction::None
                        }
                        PhysicalKey::Code(KeyCode::Minus) => {
                            if let Some(ref mut sub) = app.subtitle {
                                sub.adjust_sync(-0.5);
                            }
                            UiAction::None
                        }
                        _ => UiAction::None,
                    };
                    if action != UiAction::None {
                        app.handle_action(&action);
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
