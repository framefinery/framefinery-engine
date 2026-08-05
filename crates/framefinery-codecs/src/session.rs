use std::io::{Cursor, Write};

use framefinery_core::{
    CodecId, EncodedVideoChunk, Frame, FrameEncodeMetrics, MediaError, ReconstructionMode, Result,
    VideoChunkKind, VideoEncodeFrameMetrics, VideoEncodeOutput, VideoEncodeStreamRequest,
    VideoEncoderConfig, VideoEncoderManifest, VideoEncoderSession, VideoRateControl,
};

pub(crate) fn buffered_stream_session(
    manifest: VideoEncoderManifest,
    config: VideoEncoderConfig,
) -> Result<Box<dyn VideoEncoderSession>> {
    if config.codec.as_str() != manifest.name {
        return Err(MediaError::Unsupported {
            feature: format!("codec '{}'", config.codec),
            reason: format!("manifest '{}' cannot create it", manifest.name),
        });
    }
    if !(manifest.accepts_format)(config.input.format) {
        return Err(MediaError::Unsupported {
            feature: format!("codec '{}'", manifest.name),
            reason: format!("does not accept {} input", config.input.format),
        });
    }
    if matches!(config.rate_control, VideoRateControl::Lossless)
        && !(manifest.supports_lossless_format)(config.input.format)
    {
        return Err(MediaError::Unsupported {
            feature: format!("codec '{}'", manifest.name),
            reason: format!("does not support lossless {} input", config.input.format),
        });
    }

    Ok(Box::new(BufferedStreamEncoderSession {
        manifest,
        config,
        frames: Vec::new(),
        flushed: false,
    }))
}

struct BufferedStreamEncoderSession {
    manifest: VideoEncoderManifest,
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
                psnr: None,
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

        (self.manifest.encode)(
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
