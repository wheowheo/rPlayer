use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{bounded, Receiver, Sender};
use ffmpeg_next as ffmpeg;
use ffmpeg::codec::packet::Packet;

use crate::decode::audio_decoder::{AudioDecoder, DecodedAudio};
use crate::decode::demuxer::{self, DemuxerInfo};
use crate::decode::video_decoder::{DecodeMode, DecodedFrame, VideoDecoder};

pub enum PipelineCommand {
    Stop,
    Pause,
    Resume,
    Seek(f64),
    SetDecodeMode(DecodeMode),
}

/// 0 = SW, 1 = HW
const MODE_SW: u8 = 0;
const MODE_HW: u8 = 1;

pub struct MediaPipeline {
    pub info: DemuxerInfo,
    pub frame_rx: Receiver<DecodedFrame>,
    pub audio_rx: Option<Receiver<DecodedAudio>>,
    pub cmd_tx: Sender<PipelineCommand>,
    running: Arc<AtomicBool>,
    decode_mode: Arc<AtomicU8>,
}

impl MediaPipeline {
    pub fn open(path: &str, hw: bool) -> anyhow::Result<Self> {
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
        let decode_mode = Arc::new(AtomicU8::new(if hw { MODE_HW } else { MODE_SW }));
        let decode_mode_clone = decode_mode.clone();

        let path_owned = path.to_string();
        let has_audio = audio_stream_index.is_some();

        thread::Builder::new()
            .name("demux-decode".to_string())
            .spawn(move || {
                if let Err(e) = decode_thread(
                    &path_owned, video_stream_index, audio_stream_index,
                    hw, frame_tx, audio_tx, cmd_rx, running_clone, decode_mode_clone,
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
            decode_mode,
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.cmd_tx.send(PipelineCommand::Stop);
    }

    pub fn current_decode_mode(&self) -> DecodeMode {
        if self.decode_mode.load(Ordering::Relaxed) == MODE_HW {
            DecodeMode::Hardware
        } else {
            DecodeMode::Software
        }
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
    initial_hw: bool,
    frame_tx: Sender<DecodedFrame>,
    audio_tx: Sender<DecodedAudio>,
    cmd_rx: Receiver<PipelineCommand>,
    running: Arc<AtomicBool>,
    decode_mode: Arc<AtomicU8>,
) -> anyhow::Result<()> {
    let (mut ictx, _) = demuxer::open_input(path)?;

    let mut video_decoder = if initial_hw {
        match VideoDecoder::new_hw(&ictx, video_stream_index) {
            Ok(d) => {
                decode_mode.store(
                    if d.mode() == DecodeMode::Hardware { MODE_HW } else { MODE_SW },
                    Ordering::Relaxed,
                );
                d
            }
            Err(e) => {
                log::warn!("HW decoder init failed: {}, using SW", e);
                decode_mode.store(MODE_SW, Ordering::Relaxed);
                VideoDecoder::new_sw(&ictx, video_stream_index)?
            }
        }
    } else {
        VideoDecoder::new_sw(&ictx, video_stream_index)?
    };

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
                    if let Err(e) = demuxer::seek(&mut ictx, target, video_stream_index) {
                        log::error!("Seek error: {}", e);
                    } else {
                        video_decoder.flush();
                        if let Some(ref mut adec) = audio_decoder {
                            adec.flush();
                        }
                        seek_target_pts = Some(target);
                        while frame_tx.try_send(DecodedFrame {
                            width: 1, height: 1, data: vec![0; 4], pts_secs: -1.0,
                        }).is_ok() {}
                        while audio_tx.try_send(DecodedAudio {
                            data: Vec::new(), pts_secs: -1.0,
                            sample_rate: 0, channels: 0,
                        }).is_ok() {}
                    }
                    if paused { paused = false; }
                    break;
                }
                Some(PipelineCommand::SetDecodeMode(new_mode)) => {
                    let want_hw = new_mode == DecodeMode::Hardware;
                    let is_hw = video_decoder.mode() == DecodeMode::Hardware;
                    if want_hw != is_hw {
                        // Reopen input and recreate decoder
                        log::info!("Switching to {:?} decoder", new_mode);
                        // Remember current position from last decoded frame PTS
                        let current_pos = seek_target_pts.unwrap_or(0.0);

                        drop(video_decoder);
                        let (new_ictx, _) = demuxer::open_input(path)?;
                        ictx = new_ictx;

                        video_decoder = if want_hw {
                            match VideoDecoder::new_hw(&ictx, video_stream_index) {
                                Ok(d) => d,
                                Err(e) => {
                                    log::warn!("HW decoder switch failed: {}", e);
                                    VideoDecoder::new_sw(&ictx, video_stream_index)?
                                }
                            }
                        } else {
                            VideoDecoder::new_sw(&ictx, video_stream_index)?
                        };

                        audio_decoder = audio_stream_index.map(|idx| {
                            AudioDecoder::new(&ictx, idx)
                        }).transpose()?;

                        decode_mode.store(
                            if video_decoder.mode() == DecodeMode::Hardware { MODE_HW } else { MODE_SW },
                            Ordering::Relaxed,
                        );

                        // Seek back to position
                        if current_pos > 0.5 {
                            let _ = demuxer::seek(&mut ictx, current_pos, video_stream_index);
                            seek_target_pts = Some(current_pos);
                        }
                    }
                    break;
                }
                None => {
                    if paused { continue; }
                    break;
                }
            }
        }

        if !running.load(Ordering::Relaxed) {
            return Ok(());
        }

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
                        if let Some(target) = seek_target_pts {
                            if frame.pts_secs < target - 0.1 {
                                continue;
                            }
                            seek_target_pts = None;
                        }
                        match frame_tx.try_send(frame) {
                            Ok(()) => {}
                            Err(crossbeam_channel::TrySendError::Full(_)) => {}
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

    // Flush
    let _ = video_decoder.send_eof();
    loop {
        match video_decoder.receive_frame() {
            Ok(Some(frame)) => { let _ = frame_tx.try_send(frame); }
            _ => break,
        }
    }
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
