use framefinery_api::{
    CodecId, EncodedVideoChunk, Frame, FrameEncodeMetrics, MediaError, ReconstructionMode, Result,
    VideoChunkKind, VideoEncodeOutput, VideoEncoderConfig, VideoEncoderSession, VideoRateControl,
};

use super::{
    interface::{public_frame_metrics, vvc_options_from_settings},
    VvcEncodeFrameMetrics, VvcEncodeParams, VvcEncodeRequest, VvcFrameSinks, VvcStreamEncoder,
    VvcStreamStatus, VvcVideoGeometry, VvcVideoLimits, VVC_CODEC,
};
use crate::session::complete_frame_metrics;

pub(super) struct VvcEncoderSession {
    config: VideoEncoderConfig,
    encoder: VvcStreamEncoder,
}

impl VvcEncoderSession {
    pub(super) fn new(config: VideoEncoderConfig) -> Result<Self> {
        VVC_CODEC.validate_config(&config)?;
        let options = vvc_options_from_settings(
            matches!(config.rate_control, VideoRateControl::Lossless),
            &config.setting_specs(),
        )
        .map_err(MediaError::Message)?;
        let encoder = VvcStreamEncoder::new(
            VvcEncodeRequest {
                params: VvcEncodeParams {
                    frames: config.frame_limit.unwrap_or(0),
                },
                geometry: VvcVideoGeometry {
                    width: config.input.width,
                    height: config.input.height,
                },
                limits: VvcVideoLimits::unbounded(),
                format: config.input.format,
            },
            options,
        )
        .map_err(MediaError::Message)?;
        Ok(Self { config, encoder })
    }
}

impl VideoEncoderSession for VvcEncoderSession {
    fn codec(&self) -> &CodecId {
        &self.config.codec
    }

    fn config(&self) -> &VideoEncoderConfig {
        &self.config
    }

    fn encode_frame(&mut self, frame: Frame) -> Result<VideoEncodeOutput> {
        if matches!(self.encoder.status, VvcStreamStatus::Finished) {
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
        let mut callback = |frame: VvcEncodeFrameMetrics<'_>| {
            let frame =
                complete_frame_metrics(config, &mut total_bytes, public_frame_metrics(frame));
            metrics.push(FrameEncodeMetrics {
                frame_index: frame.frame_idx,
                frame_count: frame.frame_count,
                encoded_bytes: frame.bitstream_bytes,
                psnr: frame.psnr.map(|value| value.all),
            });
        };
        let mut sinks = VvcFrameSinks {
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
        #[cfg(feature = "vvc-stats")]
        let frame_stats = self.encoder.frame_stats();
        let encoded = self
            .encoder
            .encode_frame(
                frame.data(),
                &mut sinks,
                #[cfg(feature = "vvc-stats")]
                frame_stats,
            )
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
