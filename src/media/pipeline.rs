use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{bounded, Receiver, Sender};
use ffmpeg_next as ffmpeg;
use ffmpeg::codec::packet::Packet;

use crate::decode::audio_decoder::{AudioDecoder, DecodedAudio};
use crate::decode::demuxer::{self, DemuxerInfo};
use crate::decode::video_decoder::{DecodedFrame, VideoDecoder};

pub enum PipelineCommand {
    Stop,
    Pause,
    Resume,
    Seek(f64),
}

pub struct MediaPipeline {
    pub info: DemuxerInfo,
    pub frame_rx: Receiver<DecodedFrame>,
    pub audio_rx: Option<Receiver<DecodedAudio>>,
    pub cmd_tx: Sender<PipelineCommand>,
    running: Arc<AtomicBool>,
}

impl MediaPipeline {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        ffmpeg::init()?;

        let (_, info) = demuxer::open_input(path)?;

        let video_stream_index = info.video.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No video stream found"))?
            .index;
        let audio_stream_index = info.audio.as_ref().map(|a| a.index);

        let (frame_tx, frame_rx) = bounded::<DecodedFrame>(8);
        let (audio_tx, audio_rx) = bounded::<DecodedAudio>(32);
        let (cmd_tx, cmd_rx) = bounded::<PipelineCommand>(16);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let path_owned = path.to_string();
        let has_audio = audio_stream_index.is_some();

        thread::Builder::new()
            .name("demux-decode".to_string())
            .spawn(move || {
                if let Err(e) = decode_thread(
                    &path_owned, video_stream_index, audio_stream_index,
                    frame_tx, audio_tx, cmd_rx, running_clone,
                ) {
                    log::error!("Decode thread error: {}", e);
                }
            })?;

        Ok(Self {
            info,
            frame_rx,
            audio_rx: if has_audio { Some(audio_rx) } else { None },
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
    audio_stream_index: Option<usize>,
    frame_tx: Sender<DecodedFrame>,
    audio_tx: Sender<DecodedAudio>,
    cmd_rx: Receiver<PipelineCommand>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let (mut ictx, _) = demuxer::open_input(path)?;
    let mut video_decoder = VideoDecoder::new(&ictx, video_stream_index)?;
    let mut audio_decoder = audio_stream_index.map(|idx| {
        AudioDecoder::new(&ictx, idx)
    }).transpose()?;

    let mut paused = false;
    let mut seek_target_pts: Option<f64> = None;

    loop {
        // Check commands
        loop {
            let cmd = if paused {
                cmd_rx.recv_timeout(std::time::Duration::from_millis(50)).ok()
            } else {
                cmd_rx.try_recv().ok()
            };

            match cmd {
                Some(PipelineCommand::Stop) => return Ok(()),
                Some(PipelineCommand::Pause) => paused = true,
                Some(PipelineCommand::Resume) => { paused = false; break; }
                Some(PipelineCommand::Seek(target)) => {
                    // Perform seek
                    if let Err(e) = demuxer::seek(&mut ictx, target, video_stream_index) {
                        log::error!("Seek error: {}", e);
                    } else {
                        video_decoder.flush();
                        if let Some(ref mut adec) = audio_decoder {
                            adec.flush();
                        }
                        seek_target_pts = Some(target);
                        // Drain frame channel (consume and discard existing frames)
                        while frame_tx.try_send(DecodedFrame {
                            width: 1, height: 1, data: vec![0; 4], pts_secs: -1.0,
                        }).is_ok() {}
                        // Drain audio channel
                        while audio_tx.try_send(DecodedAudio {
                            data: Vec::new(), pts_secs: -1.0,
                            sample_rate: 0, channels: 0,
                        }).is_ok() {}
                    }
                    if paused {
                        paused = false;
                    }
                    break;
                }
                None => {
                    if paused {
                        continue; // Keep waiting
                    }
                    break;
                }
            }
        }

        if !running.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Read next packet
        let mut packet = Packet::empty();
        match packet.read(&mut ictx) {
            Ok(()) => {}
            Err(ffmpeg::Error::Eof) => break,
            Err(_) => break,
        }

        let stream_index = packet.stream();

        if stream_index == video_stream_index {
            if video_decoder.send_packet(&packet).is_err() {
                continue;
            }
            loop {
                match video_decoder.receive_frame() {
                    Ok(Some(frame)) => {
                        // Skip frames before seek target
                        if let Some(target) = seek_target_pts {
                            if frame.pts_secs < target - 0.1 {
                                continue;
                            }
                            seek_target_pts = None;
                        }
                        // Non-blocking send: drop frame if queue full (prefer audio continuity)
                        match frame_tx.try_send(frame) {
                            Ok(()) => {}
                            Err(crossbeam_channel::TrySendError::Full(_)) => {
                                // Video queue full — drop this frame to keep audio flowing
                            }
                            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                                return Ok(());
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        } else if Some(stream_index) == audio_stream_index {
            if let Some(ref mut adec) = audio_decoder {
                if adec.send_packet(&packet).is_err() {
                    continue;
                }
                loop {
                    match adec.receive_frame() {
                        Ok(Some(audio)) => {
                            // Skip audio before seek target
                            if let Some(target) = seek_target_pts {
                                if audio.pts_secs < target - 0.1 {
                                    continue;
                                }
                            }
                            if audio_tx.send(audio).is_err() { return Ok(()); }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
        }
    }

    // Flush video
    let _ = video_decoder.send_eof();
    loop {
        match video_decoder.receive_frame() {
            Ok(Some(frame)) => {
                let _ = frame_tx.try_send(frame);
            }
            _ => break,
        }
    }

    // Flush audio
    if let Some(ref mut adec) = audio_decoder {
        let _ = adec.send_eof();
        loop {
            match adec.receive_frame() {
                Ok(Some(audio)) => {
                    if audio_tx.send(audio).is_err() { return Ok(()); }
                }
                _ => break,
            }
        }
    }

    log::info!("Decode loop finished");
    Ok(())
}
