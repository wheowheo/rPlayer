use crate::config;
use crate::decode::video_decoder::DecodedFrame;

pub enum SyncAction {
    Display(DecodedFrame),
    Drop,
    Wait,
}

/// Decide what to do with the next video frame based on clock time
pub fn sync_video_frame(frame: &DecodedFrame, clock_time: f64, frame_duration: f64) -> SyncAction {
    let diff = frame.pts_secs - clock_time;

    if diff < -frame_duration {
        // Frame is too late — drop it
        SyncAction::Drop
    } else if diff > config::SYNC_THRESHOLD_SECS {
        // Frame is too early — wait
        SyncAction::Wait
    } else {
        // Within acceptable range — display
        SyncAction::Display(DecodedFrame {
            width: frame.width,
            height: frame.height,
            data: Vec::new(), // placeholder, actual data passed separately
            pts_secs: frame.pts_secs,
        })
    }
}
