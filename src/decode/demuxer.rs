use ffmpeg_next as ffmpeg;
use ffmpeg::format::context::Input;
use ffmpeg::media::Type;
use ffmpeg::Rational;

pub struct StreamInfo {
    pub index: usize,
    pub time_base: Rational,
    pub duration: Option<f64>,
    pub codec_name: String,
}

pub struct DemuxerInfo {
    pub video: Option<StreamInfo>,
    pub audio: Option<StreamInfo>,
    pub video_width: u32,
    pub video_height: u32,
    pub video_format: ffmpeg::format::Pixel,
    pub video_fps: f64,
    pub duration_secs: f64,
}

pub fn open_input(path: &str) -> Result<(Input, DemuxerInfo), ffmpeg::Error> {
    let ictx = ffmpeg::format::input(path)?;

    let mut info = DemuxerInfo {
        video: None,
        audio: None,
        video_width: 0,
        video_height: 0,
        video_format: ffmpeg::format::Pixel::None,
        video_fps: 0.0,
        duration_secs: 0.0,
    };

    // Container duration
    let format_duration = ictx.duration();
    if format_duration > 0 {
        info.duration_secs = format_duration as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE);
    }

    // Find best video stream
    if let Some(stream) = ictx.streams().best(Type::Video) {
        let tb = stream.time_base();
        let dur = if stream.duration() > 0 {
            Some(stream.duration() as f64 * f64::from(tb))
        } else {
            None
        };

        let codec_id = stream.parameters().id();
        let ctx = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
        let decoder = ctx.decoder().video()?;

        info.video_width = decoder.width();
        info.video_height = decoder.height();
        info.video_format = decoder.format();

        let rate = stream.avg_frame_rate();
        if rate.denominator() != 0 {
            info.video_fps = f64::from(rate);
        }

        let codec_name = ffmpeg::decoder::find(codec_id)
            .map(|c| c.name().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        info.video = Some(StreamInfo {
            index: stream.index(),
            time_base: tb,
            duration: dur,
            codec_name,
        });
    }

    // Find best audio stream
    if let Some(stream) = ictx.streams().best(Type::Audio) {
        let tb = stream.time_base();
        let dur = if stream.duration() > 0 {
            Some(stream.duration() as f64 * f64::from(tb))
        } else {
            None
        };

        let codec_id = stream.parameters().id();
        let codec_name = ffmpeg::decoder::find(codec_id)
            .map(|c| c.name().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        info.audio = Some(StreamInfo {
            index: stream.index(),
            time_base: tb,
            duration: dur,
            codec_name,
        });
    }

    Ok((ictx, info))
}

/// Seek to a position in seconds. Uses the video stream's time_base for precision.
pub fn seek(ictx: &mut Input, target_secs: f64, video_stream_index: usize) -> Result<(), ffmpeg::Error> {
    // Convert seconds to AV_TIME_BASE units
    let ts = (target_secs * f64::from(ffmpeg::ffi::AV_TIME_BASE)) as i64;
    unsafe {
        let ret = ffmpeg::ffi::avformat_seek_file(
            ictx.as_mut_ptr(),
            -1, // default stream
            i64::MIN,
            ts,
            ts,
            0,
        );
        if ret < 0 {
            Err(ffmpeg::Error::from(ret))
        } else {
            Ok(())
        }
    }
}
