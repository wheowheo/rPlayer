use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlayerError {
    #[error("FFmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),

    #[error("Window error: {0}")]
    Window(String),

    #[error("GPU error: {0}")]
    Gpu(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("No video stream found")]
    NoVideoStream,

    #[error("No audio stream found")]
    NoAudioStream,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, PlayerError>;
