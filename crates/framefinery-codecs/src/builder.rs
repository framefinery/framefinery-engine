use std::io::Write;

use framefinery_core::{
    CodecId, Frame, FrameInfo, FrameRate, MediaError, RawVideoFrameSource, ReconstructionMode,
    Result, VideoEncodeFrameMetricsCallback, VideoEncodeOutput, VideoEncoderConfig,
    VideoEncoderSession, VideoEncoderSetting, VideoRateControl, VideoSettingValue,
};

/// Start a fluent encoder builder for one compiled codec.
///
/// This is the ergonomic entry point for applications that want a checked
/// encoder session without manually constructing [`CodecId`] and
/// [`VideoEncoderConfig`].
pub fn encoder(codec: impl AsRef<str>) -> Result<VideoEncoderBuilder> {
    VideoEncoderBuilder::new(codec)
}

/// Fluent builder for a codec-neutral video encoder configuration.
///
/// The builder is registry-bound: [`VideoEncoderBuilder::new`] first checks
/// that `codec` names an encoder compiled into this crate. Low-level callers
/// that need to assemble configs without a compiled encoder can still use
/// [`VideoEncoderConfig::new`] directly.
#[derive(Debug, Clone)]
pub struct VideoEncoderBuilder {
    codec: CodecId,
    input: Option<FrameInfo>,
    frame_rate: Option<FrameRate>,
    frame_limit: Option<usize>,
    rate_control: VideoRateControl,
    reconstruction: ReconstructionMode,
    settings: Vec<VideoEncoderSetting>,
}

impl VideoEncoderBuilder {
    /// Create a builder for one compiled codec id.
    pub fn new(codec: impl AsRef<str>) -> Result<Self> {
        let codec = CodecId::new(codec.as_ref())?;
        if crate::find_encoder_manifest(codec.as_str()).is_none() {
            return Err(crate::unsupported_codec(codec.as_str()));
        }
        Ok(Self {
            codec,
            input: None,
            frame_rate: None,
            frame_limit: None,
            rate_control: VideoRateControl::CodecDefault,
            reconstruction: ReconstructionMode::None,
            settings: Vec::new(),
        })
    }

    /// Return the selected codec id.
    pub fn codec(&self) -> &CodecId {
        &self.codec
    }

    /// Set input frame geometry and pixel format.
    pub fn input(mut self, input: FrameInfo) -> Self {
        self.input = Some(input);
        self
    }

    /// Set optional frame-rate metadata.
    pub fn frame_rate(mut self, frame_rate: FrameRate) -> Self {
        self.frame_rate = Some(frame_rate);
        self
    }

    /// Set optional frame-rate metadata from a rational numerator/denominator.
    pub fn fps(self, numerator: u32, denominator: u32) -> Result<Self> {
        Ok(self.frame_rate(FrameRate::new(numerator, denominator)?))
    }

    /// Set an optional source/caller frame limit.
    pub fn frame_limit(mut self, frame_limit: usize) -> Self {
        self.frame_limit = Some(frame_limit);
        self
    }

    /// Set the requested rate-control mode directly.
    pub fn rate_control(mut self, rate_control: VideoRateControl) -> Self {
        self.rate_control = rate_control;
        self
    }

    /// Request stream-exact lossless coding.
    pub fn lossless(self) -> Self {
        self.rate_control(VideoRateControl::Lossless)
    }

    /// Request lossy coding with a codec-local quantizer value.
    pub fn qp(self, qp: u8) -> Result<Self> {
        Ok(self.rate_control(VideoRateControl::constant_quantizer(qp)?))
    }

    /// Set reconstruction output behavior directly.
    pub fn reconstruction(mut self, reconstruction: ReconstructionMode) -> Self {
        self.reconstruction = reconstruction;
        self
    }

    /// Request no reconstructed frames or reconstruction-derived metrics.
    pub fn no_reconstruction(self) -> Self {
        self.reconstruction(ReconstructionMode::None)
    }

    /// Request metrics computed from internal reconstruction frames.
    pub fn metrics_only(self) -> Self {
        self.reconstruction(ReconstructionMode::MetricsOnly)
    }

    /// Request reconstructed frames in the returned encode output.
    pub fn reconstruction_frames(self) -> Self {
        self.reconstruction(ReconstructionMode::Frames)
    }

    /// Append one codec extension setting.
    pub fn setting(
        mut self,
        name: impl Into<String>,
        value: impl Into<VideoSettingValue>,
    ) -> Result<Self> {
        self.settings
            .push(VideoEncoderSetting::new(name, value.into())?);
        Ok(self)
    }

    /// Build and validate the codec-neutral encoder config.
    pub fn into_config(self) -> Result<VideoEncoderConfig> {
        let input = self.input.ok_or_else(|| MediaError::MissingRequiredField {
            field: "input".to_string(),
        })?;
        let mut config = VideoEncoderConfig::new(self.codec, input)
            .with_rate_control(self.rate_control)
            .with_reconstruction(self.reconstruction);
        if let Some(frame_rate) = self.frame_rate {
            config = config.with_frame_rate(frame_rate);
        }
        if let Some(frame_limit) = self.frame_limit {
            config = config.with_frame_limit(frame_limit);
        }
        for setting in self.settings {
            config = config.with_setting(setting);
        }
        let manifest = crate::find_encoder_manifest(config.codec.as_str())
            .ok_or_else(|| crate::unsupported_codec(config.codec.as_str()))?;
        manifest.validate_config(&config)?;
        Ok(config)
    }

    /// Create a buffered encoder session from this builder.
    pub fn build(self) -> Result<Box<dyn VideoEncoderSession>> {
        crate::create_encoder(self.into_config()?)
    }

    /// Encode one owned frame using this builder.
    pub fn encode_frame(self, frame: Frame) -> Result<VideoEncodeOutput> {
        crate::encode_frame(self.into_config()?, frame)
    }

    /// Encode frames pulled from `source` using this builder.
    ///
    /// `frame_metrics`, when present, is called after each encoded frame with
    /// timing, per-frame bytes, cumulative bytes, and optional PSNR.
    pub fn encode_source(
        self,
        source: &mut dyn RawVideoFrameSource,
        output: &mut dyn Write,
        recon: Option<&mut dyn Write>,
        frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
    ) -> Result<()> {
        let config = self.into_config()?;
        crate::encode_source(&config, source, output, recon, frame_metrics)
    }
}
