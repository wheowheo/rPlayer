#[allow(dead_code)]
use crate::config;
#[allow(dead_code)]
use crate::decode::video_decoder::DecodedFrame;

#[allow(dead_code)]
pub enum SyncAction {
    Display(DecodedFrame),
    Drop,
    Wait,
}

#[allow(dead_code)]
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
