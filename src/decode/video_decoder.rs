use ffmpeg_next as ffmpeg;
use ffmpeg::codec::context::Context;
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::{context as sws_context, flag};
use ffmpeg::util::frame::video::Video;

pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
    pub pts_secs: f64,
}

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
    last_scaler_key: (u32, u32, i32), // (w, h, format)
}

// Safety: The hw_device_ctx pointer is managed by FFmpeg's refcount system
// and is only accessed from the decode thread.
unsafe impl Send for VideoDecoder {}

impl VideoDecoder {
    pub fn new_sw(input: &ffmpeg::format::context::Input, stream_index: usize) -> Result<Self, ffmpeg::Error> {
        let stream = input.streams().nth(stream_index).unwrap();
        let time_base = f64::from(stream.time_base());

        let context = Context::from_parameters(stream.parameters())?;
        let decoder = context.decoder().video()?;

        Ok(Self {
            decoder,
            scaler: None,
            time_base,
            mode: DecodeMode::Software,
            hw_device_ctx: None,
            last_scaler_key: (0, 0, 0),
        })
    }

    pub fn new_hw(input: &ffmpeg::format::context::Input, stream_index: usize) -> Result<Self, ffmpeg::Error> {
        let stream = input.streams().nth(stream_index).unwrap();
        let time_base = f64::from(stream.time_base());

        let mut context = Context::from_parameters(stream.parameters())?;

        // Try to create HW device context
        let hw_type = Self::preferred_hw_type();
        let mut hw_device_ctx: *mut ffmpeg::ffi::AVBufferRef = std::ptr::null_mut();

        let hw_ok = unsafe {
            let ret = ffmpeg::ffi::av_hwdevice_ctx_create(
                &mut hw_device_ctx,
                hw_type,
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            );
            if ret >= 0 {
                (*context.as_mut_ptr()).hw_device_ctx =
                    ffmpeg::ffi::av_buffer_ref(hw_device_ctx);
                true
            } else {
                log::warn!("HW device creation failed (code {}), falling back to SW", ret);
                false
            }
        };

        let decoder = context.decoder().video()?;

        if hw_ok {
            log::info!("Hardware decoder initialized ({:?})", Self::hw_type_name());
            Ok(Self {
                decoder,
                scaler: None,
                time_base,
                mode: DecodeMode::Hardware,
                hw_device_ctx: Some(hw_device_ctx),
                last_scaler_key: (0, 0, 0),
            })
        } else {
            Ok(Self {
                decoder,
                scaler: None,
                time_base,
                mode: DecodeMode::Software,
                hw_device_ctx: None,
                last_scaler_key: (0, 0, 0),
            })
        }
    }

    #[cfg(target_os = "macos")]
    fn preferred_hw_type() -> ffmpeg::ffi::AVHWDeviceType {
        ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX
    }

    #[cfg(target_os = "windows")]
    fn preferred_hw_type() -> ffmpeg::ffi::AVHWDeviceType {
        ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn preferred_hw_type() -> ffmpeg::ffi::AVHWDeviceType {
        ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI
    }

    #[cfg(target_os = "macos")]
    fn hw_type_name() -> &'static str { "VideoToolbox" }
    #[cfg(target_os = "windows")]
    fn hw_type_name() -> &'static str { "D3D11VA" }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn hw_type_name() -> &'static str { "VAAPI" }

    fn ensure_scaler(&mut self, src_fmt: Pixel, w: u32, h: u32) -> Result<(), ffmpeg::Error> {
        let key = (w, h, src_fmt as i32);
        if self.scaler.is_none() || self.last_scaler_key != key {
            self.scaler = Some(sws_context::Context::get(
                src_fmt, w, h,
                Pixel::RGBA, w, h,
                flag::Flags::BILINEAR,
            )?);
            self.last_scaler_key = key;
        }
        Ok(())
    }

    pub fn send_packet(&mut self, packet: &ffmpeg::Packet) -> Result<(), ffmpeg::Error> {
        self.decoder.send_packet(packet)
    }

    pub fn send_eof(&mut self) -> Result<(), ffmpeg::Error> {
        self.decoder.send_eof()
    }

    pub fn receive_frame(&mut self) -> Result<Option<DecodedFrame>, ffmpeg::Error> {
        let mut decoded = Video::empty();
        match self.decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                let pts = decoded.pts().unwrap_or(0);
                let pts_secs = pts as f64 * self.time_base;

                // If HW decoded, transfer to system memory
                let sw_frame = if self.mode == DecodeMode::Hardware {
                    self.transfer_hw_frame(&decoded)?
                } else {
                    None
                };

                let frame_ref = sw_frame.as_ref().unwrap_or(&decoded);
                let w = frame_ref.width();
                let h = frame_ref.height();
                let fmt = frame_ref.format();

                self.ensure_scaler(fmt, w, h)?;
                let mut rgba_frame = Video::empty();
                self.scaler.as_mut().unwrap().run(frame_ref, &mut rgba_frame)?;

                let stride = rgba_frame.stride(0);
                let plane_data = rgba_frame.data(0);
                let row_bytes = (w * 4) as usize;
                let mut data = Vec::with_capacity((w * h * 4) as usize);
                for y in 0..h as usize {
                    let start = y * stride;
                    let end = start + row_bytes;
                    data.extend_from_slice(&plane_data[start..end]);
                }

                Ok(Some(DecodedFrame {
                    width: w,
                    height: h,
                    data,
                    pts_secs,
                }))
            }
            Err(ffmpeg::Error::Other { errno: ffmpeg::ffi::EAGAIN }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Transfer HW surface to system memory frame
    fn transfer_hw_frame(&self, hw_frame: &Video) -> Result<Option<Video>, ffmpeg::Error> {
        unsafe {
            let hw_ptr = hw_frame.as_ptr();
            // Check if this is actually a HW frame
            if (*hw_ptr).hw_frames_ctx.is_null() {
                return Ok(None); // Already a SW frame
            }

            let mut sw_frame = Video::empty();
            let ret = ffmpeg::ffi::av_hwframe_transfer_data(
                sw_frame.as_mut_ptr(),
                hw_ptr,
                0,
            );
            if ret < 0 {
                log::warn!("HW frame transfer failed ({}), using original", ret);
                return Ok(None);
            }
            // Copy PTS
            (*sw_frame.as_mut_ptr()).pts = (*hw_ptr).pts;
            Ok(Some(sw_frame))
        }
    }

    pub fn mode(&self) -> DecodeMode {
        self.mode
    }

    pub fn flush(&mut self) {
        self.decoder.flush();
        self.scaler = None;
        self.last_scaler_key = (0, 0, 0);
    }

    #[allow(dead_code)]
    pub fn width(&self) -> u32 {
        self.decoder.width()
    }

    #[allow(dead_code)]
    pub fn height(&self) -> u32 {
        self.decoder.height()
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        if let Some(ctx) = self.hw_device_ctx.take() {
            unsafe {
                ffmpeg::ffi::av_buffer_unref(&mut (ctx as *mut _));
            }
        }
    }
}
