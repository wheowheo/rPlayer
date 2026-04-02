use ffmpeg_next as ffmpeg;
use ffmpeg::codec::context::Context;
use ffmpeg::software::resampling;
use ffmpeg::util::frame::audio::Audio;
use ffmpeg::format::Sample;
use ffmpeg::ChannelLayout;

use crate::config;

#[allow(dead_code)]
pub struct DecodedAudio {
    pub data: Vec<f32>,
    pub pts_secs: f64,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct AudioDecoder {
    decoder: ffmpeg::decoder::Audio,
    resampler: Option<resampling::Context>,
    time_base: f64,
    output_rate: u32,
    output_channels: u16,
}

impl AudioDecoder {
    pub fn new(input: &ffmpeg::format::context::Input, stream_index: usize) -> Result<Self, ffmpeg::Error> {
        let stream = input.streams().nth(stream_index).unwrap();
        let time_base = f64::from(stream.time_base());

        let context = Context::from_parameters(stream.parameters())?;
        let decoder = context.decoder().audio()?;

        Ok(Self {
            decoder,
            resampler: None,
            time_base,
            output_rate: config::AUDIO_SAMPLE_RATE,
            output_channels: config::AUDIO_CHANNELS,
        })
    }

    fn ensure_resampler(&mut self) -> Result<(), ffmpeg::Error> {
        if self.resampler.is_some() {
            return Ok(());
        }

        let out_layout = if self.output_channels == 1 {
            ChannelLayout::MONO
        } else {
            ChannelLayout::STEREO
        };

        self.resampler = Some(resampling::Context::get(
            self.decoder.format(),
            self.decoder.channel_layout(),
            self.decoder.rate(),
            Sample::F32(ffmpeg::format::sample::Type::Packed),
            out_layout,
            self.output_rate,
        )?);

        Ok(())
    }

    pub fn send_packet(&mut self, packet: &ffmpeg::Packet) -> Result<(), ffmpeg::Error> {
        self.decoder.send_packet(packet)
    }

    pub fn send_eof(&mut self) -> Result<(), ffmpeg::Error> {
        self.decoder.send_eof()
    }

    pub fn receive_frame(&mut self) -> Result<Option<DecodedAudio>, ffmpeg::Error> {
        let mut decoded = Audio::empty();
        match self.decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                self.ensure_resampler()?;

                let mut resampled = Audio::empty();
                if let Some(ref mut resampler) = self.resampler {
                    resampler.run(&decoded, &mut resampled)?;
                } else {
                    return Ok(None);
                }

                let pts = decoded.pts().unwrap_or(0);
                let pts_secs = pts as f64 * self.time_base;

                let samples = resampled.samples();
                let channels = self.output_channels as usize;
                let total_floats = samples * channels;

                let byte_data = resampled.data(0);
                let usable_bytes = byte_data.len() & !3; // align to 4-byte boundary
                let float_count = total_floats.min(usable_bytes / 4);
                if float_count == 0 {
                    return Ok(None);
                }
                let float_data: &[f32] = unsafe {
                    std::slice::from_raw_parts(
                        byte_data.as_ptr() as *const f32,
                        float_count,
                    )
                };

                Ok(Some(DecodedAudio {
                    data: float_data.to_vec(),
                    pts_secs,
                    sample_rate: self.output_rate,
                    channels: self.output_channels,
                }))
            }
            Err(ffmpeg::Error::Other { errno: ffmpeg::ffi::EAGAIN }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn flush(&mut self) {
        self.decoder.flush();
    }
}
