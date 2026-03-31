pub const APP_NAME: &str = "rPlayer";
pub const DEFAULT_WIDTH: u32 = 1280;
pub const DEFAULT_HEIGHT: u32 = 720;

// Audio
pub const AUDIO_SAMPLE_RATE: u32 = 48000;
pub const AUDIO_CHANNELS: u16 = 2;
pub const AUDIO_RING_BUFFER_SIZE: usize = 48000 * 2 * 4; // ~4 sec stereo f32

// Playback
pub const MIN_SPEED: f64 = 0.25;
pub const MAX_SPEED: f64 = 4.0;
pub const SPEED_STEP: f64 = 0.25;
pub const SEEK_STEP_SECS: f64 = 5.0;
pub const VOLUME_STEP: f64 = 0.05;
pub const MAX_VOLUME: f64 = 2.0;

// Sync
pub const SYNC_THRESHOLD_SECS: f64 = 0.04; // 40ms
pub const AUDIO_STALL_TIMEOUT_SECS: f64 = 0.2;

// Queues
pub const PACKET_QUEUE_SIZE: usize = 256;
pub const VIDEO_FRAME_QUEUE_SIZE: usize = 8;
pub const AUDIO_FRAME_QUEUE_SIZE: usize = 32;

// Speed optimization
pub const SKIP_SPEED_THRESHOLD: f64 = 2.0;
pub const KEYFRAME_ONLY_SPEED: f64 = 3.0;
