use ffmpeg_next as ffmpeg;
use ffmpeg::codec::context::Context;
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::{context as sws_context, flag};
use ffmpeg::util::frame::video::Video;

use crate::video::renderer::{ColorRange, ColorSpace, FrameFormat, PlaneData, RawFrame};

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
    // Reusable buffers to avoid per-frame allocation
    plane_bufs: [Vec<u8>; 3],
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
            plane_bufs: [Vec::new(), Vec::new(), Vec::new()],
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
                plane_bufs: [Vec::new(), Vec::new(), Vec::new()],
            })
        } else {
            Ok(Self {
                decoder, scaler: None, time_base,
                mode: DecodeMode::Software,
                hw_device_ctx: None,
                last_scaler_key: (0, 0, 0),
                plane_bufs: [Vec::new(), Vec::new(), Vec::new()],
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

                let sw_frame = if self.mode == DecodeMode::Hardware {
                    self.transfer_hw_frame(&decoded)?
                } else {
                    None
                };
                let frame_ref = sw_frame.as_ref().unwrap_or(&decoded);
                let w = frame_ref.width();
                let h = frame_ref.height();
                let fmt = frame_ref.format();

                // Extract color metadata from FFmpeg frame
                let cs = Self::detect_color_space(frame_ref);
                let cr = Self::detect_color_range(frame_ref);

                match fmt {
                    Pixel::YUV420P => Ok(Some(self.extract_yuv420p(frame_ref, w, h, pts_secs, cs, cr))),
                    Pixel::NV12 => Ok(Some(self.extract_nv12(frame_ref, w, h, pts_secs, cs, cr))),
                    _ => Ok(Some(self.convert_to_yuv420p(frame_ref, w, h, pts_secs, cs, cr)?)),
                }
            }
            Err(ffmpeg::Error::Other { errno: ffmpeg::ffi::EAGAIN }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn detect_color_space(frame: &Video) -> ColorSpace {
        match frame.color_space() {
            ffmpeg::color::Space::BT709 => ColorSpace::Bt709,
            ffmpeg::color::Space::BT2020NCL | ffmpeg::color::Space::BT2020CL => ColorSpace::Bt2020,
            ffmpeg::color::Space::BT470BG | ffmpeg::color::Space::SMPTE170M => ColorSpace::Bt601,
            _ => {
                // Auto-detect from resolution: HD+ → BT.709, SD → BT.601
                if frame.width() >= 1280 {
                    ColorSpace::Bt709
                } else {
                    ColorSpace::Bt601
                }
            }
        }
    }

    fn detect_color_range(frame: &Video) -> ColorRange {
        match frame.color_range() {
            ffmpeg::color::Range::JPEG => ColorRange::Full,
            ffmpeg::color::Range::MPEG => ColorRange::Limited,
            _ => ColorRange::Limited, // Default: TV range
        }
    }

    fn extract_yuv420p(&mut self, frame: &Video, w: u32, h: u32, pts_secs: f64, cs: ColorSpace, cr: ColorRange) -> RawFrame {
        let cw = (w / 2) as usize;
        let ch = (h / 2) as usize;
        let sizes = [(w as usize, h as usize), (cw, ch), (cw, ch)];

        let mut planes = Vec::with_capacity(3);
        for (i, &(pw, ph)) in sizes.iter().enumerate() {
            let stride = frame.stride(i);
            let src = frame.data(i);

            let buf = &mut self.plane_bufs[i];
            let needed = pw * ph;
            buf.clear();
            buf.reserve(needed);

            if stride == pw {
                buf.extend_from_slice(&src[..needed]);
            } else {
                for y in 0..ph {
                    let s = y * stride;
                    buf.extend_from_slice(&src[s..s + pw]);
                }
            }

            planes.push(PlaneData {
                data: std::mem::take(buf), // move out, will be returned via channel
                stride: pw,
                width: pw as u32,
                height: ph as u32,
            });
        }

        RawFrame { format: FrameFormat::Yuv420p, width: w, height: h, planes, pts_secs, color_space: cs, color_range: cr }
    }

    fn extract_nv12(&mut self, frame: &Video, w: u32, h: u32, pts_secs: f64, cs: ColorSpace, cr: ColorRange) -> RawFrame {
        let cw = (w / 2) as usize;
        let ch = (h / 2) as usize;
        // Y plane: w x h, 1 bpp.  UV plane: cw x ch, 2 bpp.
        let plane_specs: [(usize, usize, usize); 2] = [
            (w as usize, h as usize, 1),
            (cw, ch, 2),
        ];

        let mut planes = Vec::with_capacity(2);
        for (i, &(pw, ph, bpp)) in plane_specs.iter().enumerate() {
            let stride = frame.stride(i);
            let src = frame.data(i);
            let row_bytes = pw * bpp;

            let buf = &mut self.plane_bufs[i];
            let needed = row_bytes * ph;
            buf.clear();
            buf.reserve(needed);

            if stride == row_bytes {
                buf.extend_from_slice(&src[..needed]);
            } else {
                for y in 0..ph {
                    let s = y * stride;
                    buf.extend_from_slice(&src[s..s + row_bytes]);
                }
            }

            planes.push(PlaneData {
                data: std::mem::take(buf),
                stride: row_bytes,
                width: pw as u32,
                height: ph as u32,
            });
        }

        RawFrame { format: FrameFormat::Nv12, width: w, height: h, planes, pts_secs, color_space: cs, color_range: cr }
    }

    fn convert_to_yuv420p(&mut self, frame: &Video, w: u32, h: u32, pts_secs: f64, cs: ColorSpace, cr: ColorRange) -> Result<RawFrame, ffmpeg::Error> {
        let fmt = frame.format();
        let key = (w, h, fmt as i32);
        if self.scaler.is_none() || self.last_scaler_key != key {
            self.scaler = Some(sws_context::Context::get(
                fmt, w, h, Pixel::YUV420P, w, h, flag::Flags::FAST_BILINEAR,
            )?);
            self.last_scaler_key = key;
        }
        let mut yuv = Video::empty();
        self.scaler.as_mut().unwrap().run(frame, &mut yuv)?;
        Ok(self.extract_yuv420p(&yuv, w, h, pts_secs, cs, cr))
    }

    fn transfer_hw_frame(&self, hw_frame: &Video) -> Result<Option<Video>, ffmpeg::Error> {
        unsafe {
            let hw_ptr = hw_frame.as_ptr();
            if (*hw_ptr).hw_frames_ctx.is_null() {
                return Ok(None);
            }
            let mut sw_frame = Video::empty();
            let ret = ffmpeg::ffi::av_hwframe_transfer_data(sw_frame.as_mut_ptr(), hw_ptr, 0);
            if ret < 0 { return Ok(None); }
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
