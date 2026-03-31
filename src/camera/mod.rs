/// Camera capture abstraction.
/// Implementation uses platform-specific capture APIs:
/// - macOS: AVFoundation (via nokhwa or direct bindings)
/// - Windows: MediaFoundation (via nokhwa or direct bindings)

pub struct CameraFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
}

pub trait CameraCapture {
    fn read_frame(&mut self) -> Option<CameraFrame>;
    fn set_mirror(&mut self, mirror: bool);
    fn close(&mut self);
}

// nokhwa-based implementation is gated behind the "camera" feature.
// For now, this is a stub — the trait defines the interface for Phase 7+.
