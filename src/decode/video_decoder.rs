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

pub struct VideoDecoder {
    decoder: ffmpeg::decoder::Video,
    scaler: Option<sws_context::Context>,
    time_base: f64,
}

impl VideoDecoder {
    pub fn new(input: &ffmpeg::format::context::Input, stream_index: usize) -> Result<Self, ffmpeg::Error> {
        let stream = input.streams().nth(stream_index).unwrap();
        let time_base = f64::from(stream.time_base());

        let context = Context::from_parameters(stream.parameters())?;
        let decoder = context.decoder().video()?;

        Ok(Self {
            decoder,
            scaler: None,
            time_base,
        })
    }

    fn ensure_scaler(&mut self) -> Result<(), ffmpeg::Error> {
        let w = self.decoder.width();
        let h = self.decoder.height();
        let fmt = self.decoder.format();

        if self.scaler.is_none() || self.needs_scaler_rebuild(w, h, fmt) {
            self.scaler = Some(sws_context::Context::get(
                fmt, w, h,
                Pixel::RGBA, w, h,
                flag::Flags::BILINEAR,
            )?);
        }
        Ok(())
    }

    fn needs_scaler_rebuild(&self, _w: u32, _h: u32, _fmt: Pixel) -> bool {
        // For now, rebuild if dimensions changed
        // TODO: track previous dimensions
        false
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
                self.ensure_scaler()?;
                let mut rgba_frame = Video::empty();
                self.scaler.as_mut().unwrap().run(&decoded, &mut rgba_frame)?;

                let pts = decoded.pts().unwrap_or(0);
                let pts_secs = pts as f64 * self.time_base;

                let w = rgba_frame.width();
                let h = rgba_frame.height();
                let stride = rgba_frame.stride(0);
                let plane_data = rgba_frame.data(0);

                // Copy RGBA data, handling stride != width*4
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

    pub fn flush(&mut self) {
        self.decoder.flush();
    }

    pub fn width(&self) -> u32 {
        self.decoder.width()
    }

    pub fn height(&self) -> u32 {
        self.decoder.height()
    }
}
