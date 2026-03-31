use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{bounded, Receiver, Sender};
use ffmpeg_next as ffmpeg;

use crate::decode::demuxer::{self, DemuxerInfo};
use crate::decode::video_decoder::{DecodedFrame, VideoDecoder};

pub enum PipelineCommand {
    Stop,
    Pause,
    Resume,
}

pub struct MediaPipeline {
    pub info: DemuxerInfo,
    pub frame_rx: Receiver<DecodedFrame>,
    pub cmd_tx: Sender<PipelineCommand>,
    running: Arc<AtomicBool>,
}

impl MediaPipeline {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        ffmpeg::init()?;

        // Open once to get info
        let (_, info) = demuxer::open_input(path)?;

        let video_stream = info.video.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No video stream found"))?;
        let video_stream_index = video_stream.index;
        let audio_stream_index = info.audio.as_ref().map(|a| a.index);

        let (frame_tx, frame_rx) = bounded::<DecodedFrame>(8);
        let (cmd_tx, cmd_rx) = bounded::<PipelineCommand>(16);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let path_owned = path.to_string();

        // Spawn thread — all FFmpeg objects created inside (no Send issues)
        thread::Builder::new()
            .name("demux-decode".to_string())
            .spawn(move || {
                if let Err(e) = decode_thread(
                    &path_owned, video_stream_index, audio_stream_index,
                    frame_tx, cmd_rx, running_clone,
                ) {
                    log::error!("Decode thread error: {}", e);
                }
            })?;

        Ok(Self {
            info,
            frame_rx,
            cmd_tx,
            running,
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.cmd_tx.send(PipelineCommand::Stop);
    }
}

impl Drop for MediaPipeline {
    fn drop(&mut self) {
        self.stop();
    }
}

fn decode_thread(
    path: &str,
    video_stream_index: usize,
    _audio_stream_index: Option<usize>,
    frame_tx: Sender<DecodedFrame>,
    cmd_rx: Receiver<PipelineCommand>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let (mut ictx, _) = demuxer::open_input(path)?;
    let mut video_decoder = VideoDecoder::new(&ictx, video_stream_index)?;

    let mut paused = false;

    for (stream, packet) in ictx.packets() {
        // Check commands (non-blocking)
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                PipelineCommand::Stop => return Ok(()),
                PipelineCommand::Pause => paused = true,
                PipelineCommand::Resume => paused = false,
            }
        }

        if !running.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Wait while paused
        while paused && running.load(Ordering::Relaxed) {
            match cmd_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(PipelineCommand::Stop) => return Ok(()),
                Ok(PipelineCommand::Resume) => { paused = false; break; }
                Ok(PipelineCommand::Pause) => {}
                Err(_) => {}
            }
        }

        if stream.index() == video_stream_index {
            if video_decoder.send_packet(&packet).is_err() {
                continue;
            }

            loop {
                match video_decoder.receive_frame() {
                    Ok(Some(frame)) => {
                        if frame_tx.send(frame).is_err() {
                            return Ok(());
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    // Flush
    let _ = video_decoder.send_eof();
    loop {
        match video_decoder.receive_frame() {
            Ok(Some(frame)) => {
                if frame_tx.send(frame).is_err() {
                    return Ok(());
                }
            }
            _ => break,
        }
    }

    log::info!("Decode loop finished");
    Ok(())
}
