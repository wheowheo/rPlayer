use ffmpeg_next as ffmpeg;
use ffmpeg::codec::context::Context;
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::{context as sws_context, flag};
use ffmpeg::util::frame::video::Video;

use crate::video::renderer::{FrameFormat, PlaneData, RawFrame};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecodeMode {
    Software,
    Hardware,
}

pub struct VideoDecoder {
    decoder: ffmpeg::decoder::Video,
    scaler: Option<sws_context::Context>,
    time_base: f64,
    mode: DecodeMode,
    hw_device_ctx: Option<*mut ffmpeg::ffi::AVBufferRef>,
    last_scaler_key: (u32, u32, i32),
}

unsafe impl Send for VideoDecoder {}

impl VideoDecoder {
    pub fn new_sw(input: &ffmpeg::format::context::Input, stream_index: usize) -> Result<Self, ffmpeg::Error> {
        let stream = input.streams().nth(stream_index).unwrap();
        let time_base = f64::from(stream.time_base());
        let context = Context::from_parameters(stream.parameters())?;
        let decoder = context.decoder().video()?;
        Ok(Self {
            decoder, scaler: None, time_base,
            mode: DecodeMode::Software,
            hw_device_ctx: None,
            last_scaler_key: (0, 0, 0),
        })
    }

    pub fn new_hw(input: &ffmpeg::format::context::Input, stream_index: usize) -> Result<Self, ffmpeg::Error> {
        let stream = input.streams().nth(stream_index).unwrap();
        let time_base = f64::from(stream.time_base());
        let mut context = Context::from_parameters(stream.parameters())?;

        let hw_type = Self::preferred_hw_type();
        let mut hw_device_ctx: *mut ffmpeg::ffi::AVBufferRef = std::ptr::null_mut();

        let hw_ok = unsafe {
            let ret = ffmpeg::ffi::av_hwdevice_ctx_create(
                &mut hw_device_ctx, hw_type,
                std::ptr::null(), std::ptr::null_mut(), 0,
            );
            if ret >= 0 {
                (*context.as_mut_ptr()).hw_device_ctx = ffmpeg::ffi::av_buffer_ref(hw_device_ctx);
                true
            } else {
                log::warn!("HW device creation failed ({})", ret);
                false
            }
        };

        let decoder = context.decoder().video()?;

        if hw_ok {
            log::info!("Hardware decoder: {}", Self::hw_type_name());
            Ok(Self {
                decoder, scaler: None, time_base,
                mode: DecodeMode::Hardware,
                hw_device_ctx: Some(hw_device_ctx),
                last_scaler_key: (0, 0, 0),
            })
        } else {
            Ok(Self {
                decoder, scaler: None, time_base,
                mode: DecodeMode::Software,
                hw_device_ctx: None,
                last_scaler_key: (0, 0, 0),
            })
        }
    }

    #[cfg(target_os = "macos")]
    fn preferred_hw_type() -> ffmpeg::ffi::AVHWDeviceType { ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX }
    #[cfg(target_os = "windows")]
    fn preferred_hw_type() -> ffmpeg::ffi::AVHWDeviceType { ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn preferred_hw_type() -> ffmpeg::ffi::AVHWDeviceType { ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI }

    #[cfg(target_os = "macos")]
    fn hw_type_name() -> &'static str { "VideoToolbox" }
    #[cfg(target_os = "windows")]
    fn hw_type_name() -> &'static str { "D3D11VA" }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn hw_type_name() -> &'static str { "VAAPI" }

    pub fn send_packet(&mut self, packet: &ffmpeg::Packet) -> Result<(), ffmpeg::Error> {
        self.decoder.send_packet(packet)
    }

    pub fn send_eof(&mut self) -> Result<(), ffmpeg::Error> {
        self.decoder.send_eof()
    }

    pub fn receive_frame(&mut self) -> Result<Option<RawFrame>, ffmpeg::Error> {
        let mut decoded = Video::empty();
        match self.decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                let pts = decoded.pts().unwrap_or(0);
                let pts_secs = pts as f64 * self.time_base;

                // HW frame → system memory
                let sw_frame = if self.mode == DecodeMode::Hardware {
                    self.transfer_hw_frame(&decoded)?
                } else {
                    None
                };
                let frame_ref = sw_frame.as_ref().unwrap_or(&decoded);
                let w = frame_ref.width();
                let h = frame_ref.height();
                let fmt = frame_ref.format();

                // Try zero-copy YUV path
                match fmt {
                    Pixel::YUV420P => {
                        Ok(Some(self.extract_yuv420p(frame_ref, w, h, pts_secs)))
                    }
                    Pixel::NV12 => {
                        Ok(Some(self.extract_nv12(frame_ref, w, h, pts_secs)))
                    }
                    _ => {
                        // Fallback: convert to YUV420P via swscale
                        Ok(Some(self.convert_to_yuv420p(frame_ref, w, h, pts_secs)?))
                    }
                }
            }
            Err(ffmpeg::Error::Other { errno: ffmpeg::ffi::EAGAIN }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn extract_yuv420p(&self, frame: &Video, w: u32, h: u32, pts_secs: f64) -> RawFrame {
        let copy_plane = |idx: usize, pw: u32, ph: u32| {
            let stride = frame.stride(idx);
            let data = frame.data(idx);
            let row_bytes = pw as usize;
            if stride == row_bytes {
                PlaneData { data: data[..row_bytes * ph as usize].to_vec(), stride, width: pw, height: ph }
            } else {
                let mut packed = Vec::with_capacity(row_bytes * ph as usize);
                for y in 0..ph as usize {
                    let s = y * stride;
                    packed.extend_from_slice(&data[s..s + row_bytes]);
                }
                PlaneData { data: packed, stride: row_bytes, width: pw, height: ph }
            }
        };

        let cw = w / 2;
        let ch = h / 2;
        RawFrame {
            format: FrameFormat::Yuv420p,
            width: w, height: h,
            planes: vec![
                copy_plane(0, w, h),
                copy_plane(1, cw, ch),
                copy_plane(2, cw, ch),
            ],
            pts_secs,
        }
    }

    fn extract_nv12(&self, frame: &Video, w: u32, h: u32, pts_secs: f64) -> RawFrame {
        let copy_plane = |idx: usize, pw: u32, ph: u32, bpp: usize| {
            let stride = frame.stride(idx);
            let data = frame.data(idx);
            let row_bytes = pw as usize * bpp;
            if stride == row_bytes {
                PlaneData { data: data[..row_bytes * ph as usize].to_vec(), stride, width: pw, height: ph }
            } else {
                let mut packed = Vec::with_capacity(row_bytes * ph as usize);
                for y in 0..ph as usize {
                    let s = y * stride;
                    packed.extend_from_slice(&data[s..s + row_bytes]);
                }
                PlaneData { data: packed, stride: row_bytes, width: pw, height: ph }
            }
        };

        let cw = w / 2;
        let ch = h / 2;
        RawFrame {
            format: FrameFormat::Nv12,
            width: w, height: h,
            planes: vec![
                copy_plane(0, w, h, 1),
                copy_plane(1, cw, ch, 2),
            ],
            pts_secs,
        }
    }

    fn convert_to_yuv420p(&mut self, frame: &Video, w: u32, h: u32, pts_secs: f64) -> Result<RawFrame, ffmpeg::Error> {
        let fmt = frame.format();
        let key = (w, h, fmt as i32);
        if self.scaler.is_none() || self.last_scaler_key != key {
            self.scaler = Some(sws_context::Context::get(
                fmt, w, h,
                Pixel::YUV420P, w, h,
                flag::Flags::FAST_BILINEAR,
            )?);
            self.last_scaler_key = key;
        }
        let mut yuv = Video::empty();
        self.scaler.as_mut().unwrap().run(frame, &mut yuv)?;
        Ok(self.extract_yuv420p(&yuv, w, h, pts_secs))
    }

    fn transfer_hw_frame(&self, hw_frame: &Video) -> Result<Option<Video>, ffmpeg::Error> {
        unsafe {
            let hw_ptr = hw_frame.as_ptr();
            if (*hw_ptr).hw_frames_ctx.is_null() {
                return Ok(None);
            }
            let mut sw_frame = Video::empty();
            let ret = ffmpeg::ffi::av_hwframe_transfer_data(sw_frame.as_mut_ptr(), hw_ptr, 0);
            if ret < 0 {
                return Ok(None);
            }
            (*sw_frame.as_mut_ptr()).pts = (*hw_ptr).pts;
            Ok(Some(sw_frame))
        }
    }

    pub fn mode(&self) -> DecodeMode { self.mode }

    pub fn flush(&mut self) {
        self.decoder.flush();
        self.scaler = None;
        self.last_scaler_key = (0, 0, 0);
    }

    #[allow(dead_code)]
    pub fn width(&self) -> u32 { self.decoder.width() }
    #[allow(dead_code)]
    pub fn height(&self) -> u32 { self.decoder.height() }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        if let Some(ctx) = self.hw_device_ctx.take() {
            unsafe { ffmpeg::ffi::av_buffer_unref(&mut (ctx as *mut _)); }
        }
    }
}
