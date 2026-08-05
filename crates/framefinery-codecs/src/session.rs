use std::io::{self, Cursor, Read, Write};

use framefinery_core::{
    frame_psnr, CodecId, EncodedVideoChunk, Frame, FrameEncodeMetrics, MediaError,
    RawVideoFrameSource, ReconstructionMode, Result, VideoChunkKind, VideoEncodeFrameMetrics,
    VideoEncodeFrameMetricsCallback, VideoEncodeOutput, VideoEncodeSourceRequest,
    VideoEncoderConfig, VideoEncoderManifest, VideoEncoderSession, VideoRateControl,
};

pub(crate) type VideoEncodeStreamFn =
    for<'request, 'callback> fn(
        &mut dyn Read,
        &mut dyn Write,
        Option<&mut dyn Write>,
        VideoEncodeStreamRequest<'request>,
        Option<VideoEncodeFrameMetricsCallback<'callback>>,
    ) -> std::result::Result<(), String>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct StreamEncoderManifest {
    pub public: VideoEncoderManifest,
    pub encode_stream: VideoEncodeStreamFn,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VideoEncodeStreamRequest<'a> {
    pub frames: usize,
    pub width: usize,
    pub height: usize,
    pub format: framefinery_core::PixelFormat,
    pub lossless: bool,
    pub settings: &'a [String],
}

pub(crate) fn buffered_stream_session(
    manifest: StreamEncoderManifest,
    config: VideoEncoderConfig,
) -> Result<Box<dyn VideoEncoderSession>> {
    manifest.public.validate_config(&config)?;

    Ok(Box::new(BufferedStreamEncoderSession {
        manifest,
        config,
        frames: Vec::new(),
        flushed: false,
    }))
}

struct BufferedStreamEncoderSession {
    manifest: StreamEncoderManifest,
    config: VideoEncoderConfig,
    frames: Vec<Frame>,
    flushed: bool,
}

impl VideoEncoderSession for BufferedStreamEncoderSession {
    fn codec(&self) -> &CodecId {
        &self.config.codec
    }

    fn config(&self) -> &VideoEncoderConfig {
        &self.config
    }

    fn encode_frame(&mut self, frame: Frame) -> Result<VideoEncodeOutput> {
        if self.flushed {
            return Err(MediaError::Message(
                "cannot encode a frame after encoder flush".to_string(),
            ));
        }
        if frame.info() != self.config.input {
            return Err(MediaError::IncompatibleFormat {
                format: frame.info().format.name(),
                reason: format!(
                    "expected {}x{}:{}, got {}x{}:{}",
                    self.config.input.width,
                    self.config.input.height,
                    self.config.input.format,
                    frame.info().width,
                    frame.info().height,
                    frame.info().format
                ),
            });
        }
        if self
            .config
            .frame_limit
            .is_some_and(|limit| self.frames.len() >= limit)
        {
            return Err(MediaError::Message(format!(
                "encoder frame limit {} was exceeded",
                self.config.frame_limit.unwrap()
            )));
        }
        self.frames.push(frame);
        Ok(VideoEncodeOutput::default())
    }

    fn flush(&mut self) -> Result<VideoEncodeOutput> {
        if self.flushed {
            return Ok(VideoEncodeOutput::default());
        }
        self.flushed = true;

        let frame_count = self.frames.len();
        let input_len = self
            .config
            .input
            .expected_len()
            .checked_mul(frame_count)
            .ok_or(MediaError::LengthOverflow)?;
        let mut input = Vec::with_capacity(input_len);
        for frame in &self.frames {
            input.extend_from_slice(frame.data());
        }

        let mut bitstream = Vec::new();
        let mut recon = Vec::new();
        let settings = self.config.setting_specs();
        let request = VideoEncodeStreamRequest {
            frames: frame_count,
            width: self.config.input.width,
            height: self.config.input.height,
            format: self.config.input.format,
            lossless: matches!(self.config.rate_control, VideoRateControl::Lossless),
            settings: &settings,
        };
        let mut metrics = Vec::new();
        let mut metrics_callback = |frame: VideoEncodeFrameMetrics<'_>| {
            metrics.push(FrameEncodeMetrics {
                frame_index: frame.frame_idx,
                frame_count: Some(frame.frame_count),
                encoded_bytes: frame.bitstream_bytes,
                psnr: frame_psnr(self.config.input, frame.source, frame.reconstruction)
                    .map(|psnr| psnr.all),
            });
        };
        let should_collect_metrics = self.config.reconstruction != ReconstructionMode::None;
        let metrics_callback = if should_collect_metrics {
            Some(
                &mut metrics_callback
                    as &mut dyn for<'frame> FnMut(VideoEncodeFrameMetrics<'frame>),
            )
        } else {
            None
        };
        let recon_writer = match self.config.reconstruction {
            ReconstructionMode::Frames => Some(&mut recon as &mut dyn Write),
            ReconstructionMode::None | ReconstructionMode::MetricsOnly => None,
        };

        (self.manifest.encode_stream)(
            &mut Cursor::new(input),
            &mut bitstream,
            recon_writer,
            request,
            metrics_callback,
        )
        .map_err(MediaError::Message)?;

        let mut chunk =
            EncodedVideoChunk::new(self.config.codec.clone(), VideoChunkKind::Stream, bitstream);
        chunk.keyframe = true;

        let reconstructions = if self.config.reconstruction == ReconstructionMode::Frames {
            split_reconstructions(&self.config, recon)?
        } else {
            Vec::new()
        };

        Ok(VideoEncodeOutput {
            chunks: vec![chunk],
            reconstructions,
            metrics,
        })
    }
}

pub(crate) fn encode_stream_from_source(
    manifest: StreamEncoderManifest,
    source: &mut dyn RawVideoFrameSource,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: VideoEncodeSourceRequest<'_>,
    frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
) -> Result<()> {
    let config = request.config;
    manifest.public.validate_config(config)?;
    let frame_count = config.frame_limit.ok_or_else(|| {
        MediaError::Message(
            "source-driven encoding requires VideoEncoderConfig::frame_limit".to_string(),
        )
    })?;
    let settings = config.setting_specs();
    let request = VideoEncodeStreamRequest {
        frames: frame_count,
        width: config.input.width,
        height: config.input.height,
        format: config.input.format,
        lossless: matches!(config.rate_control, VideoRateControl::Lossless),
        settings: &settings,
    };
    let mut input = SourceFrameReader::new(source, config.input.expected_len(), frame_count);
    (manifest.encode_stream)(&mut input, output, recon, request, frame_metrics)
        .map_err(MediaError::Message)
}

struct SourceFrameReader<'a> {
    source: &'a mut dyn RawVideoFrameSource,
    frame: Vec<u8>,
    frame_offset: usize,
    frames_read: usize,
    frame_limit: usize,
}

impl<'a> SourceFrameReader<'a> {
    fn new(source: &'a mut dyn RawVideoFrameSource, frame_len: usize, frame_limit: usize) -> Self {
        Self {
            source,
            frame: vec![0; frame_len],
            frame_offset: frame_len,
            frames_read: 0,
            frame_limit,
        }
    }

    fn load_frame(&mut self) -> io::Result<bool> {
        if self.frames_read >= self.frame_limit {
            return Ok(false);
        }
        let has_frame = self
            .source
            .read_frame(&mut self.frame)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
        if !has_frame {
            return Ok(false);
        }
        self.frames_read += 1;
        self.frame_offset = 0;
        Ok(true)
    }
}

impl Read for SourceFrameReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }

        let mut written = 0usize;
        while written < output.len() {
            if self.frame_offset == self.frame.len() && !self.load_frame()? {
                break;
            }
            let remaining_frame = self.frame.len() - self.frame_offset;
            let count = remaining_frame.min(output.len() - written);
            output[written..written + count]
                .copy_from_slice(&self.frame[self.frame_offset..self.frame_offset + count]);
            self.frame_offset += count;
            written += count;
        }
        Ok(written)
    }
}

fn split_reconstructions(config: &VideoEncoderConfig, recon: Vec<u8>) -> Result<Vec<Frame>> {
    let frame_len = config.input.expected_len();
    if recon.is_empty() {
        return Ok(Vec::new());
    }
    if recon.len() % frame_len != 0 {
        return Err(MediaError::BufferLength {
            expected: frame_len * (recon.len().div_ceil(frame_len)),
            actual: recon.len(),
        });
    }
    recon
        .chunks_exact(frame_len)
        .map(|frame| Frame::new(config.input, frame.to_vec()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use framefinery_core::{FrameInfo, PixelFormat, VideoEncoderSetting};

    const TEST_CODEC: VideoEncoderManifest = VideoEncoderManifest {
        name: "test",
        feature: "test-codec",
        summary: "test stream bridge codec",
        settings: &[],
        accepts_format: test_accepts_format,
        supports_lossless_format: test_accepts_format,
        create_session: test_create_session,
        encode_source: test_encode_source,
    };

    const TEST_STREAM_ENCODER: StreamEncoderManifest = StreamEncoderManifest {
        public: TEST_CODEC,
        encode_stream: test_encode_stream,
    };

    fn test_accepts_format(format: PixelFormat) -> bool {
        format == PixelFormat::Rgb24
    }

    fn test_create_session(config: VideoEncoderConfig) -> Result<Box<dyn VideoEncoderSession>> {
        buffered_stream_session(TEST_STREAM_ENCODER, config)
    }

    fn test_encode_source(
        source: &mut dyn RawVideoFrameSource,
        output: &mut dyn Write,
        recon: Option<&mut dyn Write>,
        request: VideoEncodeSourceRequest<'_>,
        frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
    ) -> Result<()> {
        encode_stream_from_source(
            TEST_STREAM_ENCODER,
            source,
            output,
            recon,
            request,
            frame_metrics,
        )
    }

    fn test_encode_stream(
        input: &mut dyn Read,
        output: &mut dyn Write,
        mut recon: Option<&mut dyn Write>,
        request: VideoEncodeStreamRequest<'_>,
        mut frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
    ) -> std::result::Result<(), String> {
        let frame_len = request
            .format
            .frame_len(request.width, request.height)
            .ok_or_else(|| "frame length overflow".to_string())?;
        let mut frame = vec![0; frame_len];
        for frame_idx in 0..request.frames {
            input
                .read_exact(&mut frame)
                .map_err(|err| format!("read frame: {err}"))?;
            output.write_all(&frame).map_err(|err| err.to_string())?;
            if let Some(writer) = recon.as_mut() {
                writer.write_all(&frame).map_err(|err| err.to_string())?;
            }
            if let Some(callback) = frame_metrics.as_mut() {
                callback(VideoEncodeFrameMetrics {
                    frame_idx,
                    frame_count: request.frames,
                    bitstream_bytes: frame_len,
                    source: &frame,
                    reconstruction: &frame,
                });
            }
        }
        Ok(())
    }

    fn config(info: FrameInfo) -> VideoEncoderConfig {
        VideoEncoderConfig::new(CodecId::new("test").unwrap(), info).with_frame_limit(2)
    }

    #[test]
    fn source_bridge_pulls_one_frame_at_a_time() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let frames = [
            vec![1u8; info.expected_len()],
            vec![2u8; info.expected_len()],
        ];
        let mut index = 0usize;
        let mut source = |frame: &mut [u8]| {
            let Some(next) = frames.get(index) else {
                return Ok(false);
            };
            frame.copy_from_slice(next);
            index += 1;
            Ok(true)
        };
        let mut output = Vec::new();

        TEST_CODEC
            .encode_source(&mut source, &mut output, None, &config(info), None)
            .expect("source bridge encode");

        assert_eq!(index, 2);
        assert_eq!(
            output,
            [frames[0].as_slice(), frames[1].as_slice()].concat()
        );
    }

    #[test]
    fn buffered_session_flush_is_idempotent_and_then_closed() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let mut encoder = TEST_CODEC
            .create_encoder(config(info))
            .expect("create test encoder session");

        encoder
            .encode_frame(Frame::new(info, vec![1u8; info.expected_len()]).unwrap())
            .expect("first frame");
        encoder
            .encode_frame(Frame::new(info, vec![2u8; info.expected_len()]).unwrap())
            .expect("second frame");
        let first = encoder.flush().expect("first flush");
        let second = encoder.flush().expect("second flush");

        assert_eq!(first.chunks.len(), 1);
        assert!(second.chunks.is_empty());
        assert!(encoder
            .encode_frame(Frame::new(info, vec![3u8; info.expected_len()]).unwrap())
            .unwrap_err()
            .to_string()
            .contains("after encoder flush"));
    }

    #[test]
    fn buffered_session_rejects_wrong_frame_info() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let other = FrameInfo::new(4, 2, PixelFormat::Rgb24).unwrap();
        let mut encoder = TEST_CODEC
            .create_encoder(config(info))
            .expect("create test encoder session");

        let err = encoder
            .encode_frame(Frame::new(other, vec![0; other.expected_len()]).unwrap())
            .unwrap_err();
        assert!(err.to_string().contains("expected 2x2:rgb24"));
    }

    #[test]
    fn source_encode_requires_frame_limit() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("test").unwrap(), info);
        let mut source = |_frame: &mut [u8]| Ok(false);
        let mut output = Vec::new();

        let err = TEST_CODEC
            .encode_source(&mut source, &mut output, None, &config, None)
            .unwrap_err();
        assert!(err.to_string().contains("frame_limit"));
    }

    #[test]
    fn buffered_session_validates_config_before_creation() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let config =
            config(info).with_setting(VideoEncoderSetting::boolean("missing", true).unwrap());

        let err = match TEST_CODEC.create_encoder(config) {
            Ok(_) => panic!("unknown setting should reject encoder creation"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("unknown setting"));
    }
}
