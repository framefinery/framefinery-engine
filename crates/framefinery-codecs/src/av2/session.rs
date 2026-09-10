use framefinery_api::{
    CodecId, EncodedVideoChunk, Frame, FrameEncodeMetrics, MediaError, ReconstructionMode, Result,
    VideoChunkKind, VideoEncodeOutput, VideoEncoderConfig, VideoEncoderSession, VideoRateControl,
};

use super::{
    interface::{av2_options_from_settings, public_frame_metrics},
    Av2EncodeFrameMetrics, Av2EncodeParams, Av2EncodeRequest, Av2FrameSinks, Av2StreamEncoder,
    Av2StreamStatus, Av2VideoGeometry, AV2_CODEC,
};
use crate::session::complete_frame_metrics;

pub(super) struct Av2EncoderSession {
    config: VideoEncoderConfig,
    encoder: Av2StreamEncoder,
}

impl Av2EncoderSession {
    pub(super) fn new(config: VideoEncoderConfig) -> Result<Self> {
        AV2_CODEC.validate_config(&config)?;
        let options = av2_options_from_settings(
            matches!(config.rate_control, VideoRateControl::Lossless),
            &config.setting_specs(),
        )
        .map_err(MediaError::Message)?;
        let encoder = Av2StreamEncoder::new(
            Av2EncodeRequest {
                params: Av2EncodeParams {
                    frames: config.frame_limit.unwrap_or(0),
                },
                geometry: Av2VideoGeometry {
                    width: config.input.width,
                    height: config.input.height,
                },
                format: config.input.format,
            },
            options,
        )
        .map_err(MediaError::Message)?;
        Ok(Self { config, encoder })
    }
}

impl VideoEncoderSession for Av2EncoderSession {
    fn codec(&self) -> &CodecId {
        &self.config.codec
    }

    fn config(&self) -> &VideoEncoderConfig {
        &self.config
    }

    fn encode_frame(&mut self, frame: Frame) -> Result<VideoEncodeOutput> {
        if matches!(self.encoder.status, Av2StreamStatus::Finished) {
            return Err(MediaError::EncodeAfterFlush);
        }
        self.encoder.ensure_active().map_err(MediaError::Message)?;
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
        if let Some(limit) = self.config.frame_limit {
            if self.encoder.frame_index >= limit {
                return Err(MediaError::FrameLimitExceeded { limit });
            }
        }

        let mut bitstream = Vec::new();
        let mut recon = Vec::new();
        let mut metrics = Vec::new();
        let mut total_bytes = 0;
        let config = &self.config;
        let mut callback = |frame: Av2EncodeFrameMetrics<'_>| {
            let frame =
                complete_frame_metrics(config, &mut total_bytes, public_frame_metrics(frame));
            metrics.push(FrameEncodeMetrics {
                frame_index: frame.frame_idx,
                frame_count: frame.frame_count,
                encoded_bytes: frame.bitstream_bytes,
                psnr: frame.psnr.map(|value| value.all),
            });
        };
        let mut sinks = Av2FrameSinks {
            output: &mut bitstream,
            recon: if config.reconstruction == ReconstructionMode::Frames {
                Some(&mut recon)
            } else {
                None
            },
            frame_metrics: if config.reconstruction != ReconstructionMode::None {
                Some(&mut callback)
            } else {
                None
            },
        };
        let frame_stats = self.encoder.frame_stats();
        let encoded = self
            .encoder
            .encode_frame(frame.data(), &mut sinks, frame_stats)
            .map_err(MediaError::Message)?;
        let reconstructions = if self.config.reconstruction == ReconstructionMode::Frames {
            vec![Frame::new(self.config.input, recon).map_err(|error| {
                self.encoder.fail(error.to_string());
                error
            })?]
        } else {
            Vec::new()
        };
        let mut chunk =
            EncodedVideoChunk::new(self.config.codec.clone(), VideoChunkKind::Frame, bitstream);
        chunk.frame_index = Some(encoded.frame_index);
        chunk.keyframe = encoded.keyframe;
        Ok(VideoEncodeOutput {
            chunks: vec![chunk],
            reconstructions,
            metrics,
        })
    }

    fn flush(&mut self) -> Result<VideoEncodeOutput> {
        self.encoder.finish().map_err(MediaError::Message)?;
        Ok(VideoEncodeOutput::default())
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
