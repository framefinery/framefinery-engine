use std::fmt;
use std::io::{Read, Write};
use std::str::FromStr;
use std::time::Duration;

use crate::{
    Frame, FrameInfo, FramePsnr, MediaError, PixelFormat, Result, SettingManifest, SettingValue,
    Timestamp,
};

/// Stable codec identifier used by CLI, manifests, and library callers.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CodecId(String);

/// Rational frame rate metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRate {
    /// Frame-rate numerator.
    pub numerator: u32,
    /// Frame-rate denominator.
    pub denominator: u32,
}

/// Rate-control mode requested from a video encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoRateControl {
    /// Let the codec choose its default mode.
    #[default]
    CodecDefault,
    /// Request stream-exact lossless coding.
    Lossless,
    /// Request lossy coding with a codec-local quantizer value.
    ConstantQuantizer(u8),
}

/// Reconstruction data requested from a video encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReconstructionMode {
    /// Do not return reconstructions or compute reconstruction-derived metrics.
    #[default]
    None,
    /// Compute metrics while reconstructed samples are available internally.
    MetricsOnly,
    /// Return reconstructed frames to the caller.
    Frames,
}

/// Typed value for a codec extension setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoSettingValue {
    /// Boolean setting value.
    Boolean(bool),
    /// Integer setting value.
    Integer(i64),
    /// Text or choice setting value.
    Text(String),
}

/// One codec extension setting in a [`VideoEncoderConfig`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoEncoderSetting {
    /// Setting name.
    pub name: String,
    /// Typed setting value.
    pub value: VideoSettingValue,
}

/// Codec-neutral configuration for constructing or driving a video encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoEncoderConfig {
    /// Selected codec id.
    pub codec: CodecId,
    /// Input frame geometry and pixel format.
    pub input: FrameInfo,
    /// Optional frame-rate metadata.
    pub frame_rate: Option<FrameRate>,
    /// Optional caller/source frame limit.
    pub frame_limit: Option<usize>,
    /// Requested rate-control mode.
    pub rate_control: VideoRateControl,
    /// Requested reconstruction output mode.
    pub reconstruction: ReconstructionMode,
    /// Codec extension settings.
    pub settings: Vec<VideoEncoderSetting>,
}

/// Encoded chunk category emitted by video encoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoChunkKind {
    /// Codec configuration bytes.
    Config,
    /// One coded frame or access unit.
    Frame,
    /// End-of-stream marker.
    EndOfStream,
    /// Whole stream payload used by compatibility encoders.
    Stream,
}

/// Encoded bytes plus timing and keyframe metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedVideoChunk {
    /// Codec that produced the chunk.
    pub codec: CodecId,
    /// Chunk category.
    pub kind: VideoChunkKind,
    /// Encoded payload bytes.
    pub data: Vec<u8>,
    /// Optional zero-based frame index associated with this chunk.
    pub frame_index: Option<usize>,
    /// Optional presentation timestamp.
    pub pts: Option<Timestamp>,
    /// Optional decode timestamp.
    pub dts: Option<Timestamp>,
    /// Optional duration.
    pub duration: Option<Timestamp>,
    /// Whether this chunk starts or represents a keyframe.
    pub keyframe: bool,
}

/// Per-frame encode metrics returned by session-style encoders.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameEncodeMetrics {
    /// Zero-based frame index.
    pub frame_index: usize,
    /// Optional total frame count when known by the caller.
    pub frame_count: Option<usize>,
    /// Number of encoded bytes produced through this frame.
    pub encoded_bytes: usize,
    /// Optional aggregate PSNR for this frame.
    pub psnr: Option<f64>,
}

/// Output produced by one encoder-session operation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VideoEncodeOutput {
    /// Encoded chunks emitted by this operation.
    pub chunks: Vec<EncodedVideoChunk>,
    /// Reconstructed frames returned by this operation.
    pub reconstructions: Vec<Frame>,
    /// Per-frame metrics returned by this operation.
    pub metrics: Vec<FrameEncodeMetrics>,
}

/// Buffered frame-session video encoder interface.
///
/// This API is convenient for filtered frame flows and tests. Long file streams
/// should prefer source-driven encoding until the experimental codec sessions
/// become fully incremental.
pub trait VideoEncoderSession {
    /// Codec id for this encoder session.
    fn codec(&self) -> &CodecId;

    /// Configuration used to create this session.
    fn config(&self) -> &VideoEncoderConfig;

    /// Submit one frame to the encoder session.
    fn encode_frame(&mut self, frame: Frame) -> Result<VideoEncodeOutput>;

    /// Finish the stream and return any delayed output.
    fn flush(&mut self) -> Result<VideoEncodeOutput> {
        Ok(VideoEncodeOutput::default())
    }
}

#[doc(hidden)]
pub type VideoEncoderSessionFactory =
    fn(VideoEncoderConfig) -> Result<Box<dyn VideoEncoderSession>>;

/// Pull-based raw video source used by source-driven encoders.
pub trait RawVideoFrameSource {
    /// Fill `frame` with exactly one raw frame.
    ///
    /// Returns `Ok(false)` only at clean EOF before any bytes of the next frame
    /// are read.
    fn read_frame(&mut self, frame: &mut [u8]) -> Result<bool>;
}

impl<F> RawVideoFrameSource for F
where
    F: FnMut(&mut [u8]) -> Result<bool>,
{
    fn read_frame(&mut self, frame: &mut [u8]) -> Result<bool> {
        self(frame)
    }
}

/// [`RawVideoFrameSource`] adapter for any [`Read`] implementation.
pub struct RawVideoFrameReadSource<R> {
    inner: R,
}

impl<R: Read> RawVideoFrameReadSource<R> {
    /// Wrap a reader as a raw frame source.
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// Return the wrapped reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> RawVideoFrameSource for RawVideoFrameReadSource<R> {
    fn read_frame(&mut self, frame: &mut [u8]) -> Result<bool> {
        let mut offset = 0usize;
        while offset < frame.len() {
            let count = self
                .inner
                .read(&mut frame[offset..])
                .map_err(|err| MediaError::Message(format!("failed to read input frame: {err}")))?;
            if count == 0 {
                if offset == 0 {
                    return Ok(false);
                }
                return Err(MediaError::ShortFrameRead {
                    expected: frame.len(),
                    actual: offset,
                });
            }
            offset += count;
        }
        Ok(true)
    }
}

/// [`Read`] adapter for any [`RawVideoFrameSource`] implementation.
///
/// The adapter buffers one raw frame at a time so legacy stream encoders that
/// consume byte readers can still be driven from pull-based frame sources
/// without materializing a whole stream.
pub struct RawVideoFrameSourceReadAdapter<S> {
    source: S,
    frame: Vec<u8>,
    frame_offset: usize,
    frames_read: usize,
    frame_limit: Option<usize>,
}

impl<S> RawVideoFrameSourceReadAdapter<S>
where
    S: RawVideoFrameSource,
{
    /// Create a byte reader over `source` using the byte length implied by `info`.
    pub fn new(source: S, info: FrameInfo) -> Self {
        let frame_len = info.expected_len();
        Self {
            source,
            frame: vec![0; frame_len],
            frame_offset: frame_len,
            frames_read: 0,
            frame_limit: None,
        }
    }

    /// Stop reading after at most `frame_limit` complete frames.
    pub fn with_frame_limit(mut self, frame_limit: usize) -> Self {
        self.frame_limit = Some(frame_limit);
        self
    }

    /// Return the number of complete frames read from the source.
    pub const fn frames_read(&self) -> usize {
        self.frames_read
    }

    /// Consume the adapter and return the wrapped source.
    pub fn into_inner(self) -> S {
        self.source
    }

    fn load_frame(&mut self) -> std::io::Result<bool> {
        if self
            .frame_limit
            .is_some_and(|limit| self.frames_read >= limit)
        {
            return Ok(false);
        }
        let has_frame = self
            .source
            .read_frame(&mut self.frame)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err.to_string()))?;
        if !has_frame {
            return Ok(false);
        }
        self.frames_read += 1;
        self.frame_offset = 0;
        Ok(true)
    }
}

impl<S> Read for RawVideoFrameSourceReadAdapter<S>
where
    S: RawVideoFrameSource,
{
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
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

/// Discovery manifest for one compiled video encoder.
///
/// The manifest exposes stable metadata for catalogs, help text, and validation
/// of codec-neutral configs. Normal callers should create and drive encoders
/// through registry helpers provided by `framefinery` or `framefinery-codecs`,
/// using the codec id already stored in [`VideoEncoderConfig`].
#[derive(Debug, Clone, Copy)]
pub struct VideoEncoderManifest {
    /// Stable codec id string.
    pub name: &'static str,
    /// Cargo feature that enables this encoder.
    pub feature: &'static str,
    /// Short user-facing encoder summary.
    pub summary: &'static str,
    /// Codec-specific settings accepted by this encoder.
    pub settings: &'static [SettingManifest],
    /// Function that tests whether an input pixel format is accepted.
    pub accepts_format: fn(PixelFormat) -> bool,
    /// Function that tests whether lossless mode is accepted for an input format.
    pub supports_lossless_format: fn(PixelFormat) -> bool,
    create_session_hook: VideoEncoderSessionFactory,
    encode_source_hook: VideoEncodeSourceFn,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct VideoEncodeSourceRequest<'a> {
    #[doc(hidden)]
    pub config: &'a VideoEncoderConfig,
}

/// Per-frame metrics passed to source-driven encode callbacks.
pub struct VideoEncodeFrameMetrics<'a> {
    /// Zero-based frame index.
    pub frame_idx: usize,
    /// Optional total frame count when known by the caller.
    pub frame_count: Option<usize>,
    /// Encoded bytes produced by this frame.
    pub bitstream_bytes: usize,
    /// Encoded bytes produced by all reported frames through this frame.
    pub total_bitstream_bytes: usize,
    /// Wall time spent encoding this frame after the source frame was read.
    pub encode_elapsed: Duration,
    /// PSNR for this frame when reconstruction metrics were requested.
    pub psnr: Option<FramePsnr>,
    /// Source frame bytes for metric calculations.
    pub source: &'a [u8],
    /// Reconstructed frame bytes for metric calculations.
    pub reconstruction: &'a [u8],
}

/// Callback type used to receive source-driven per-frame metrics.
pub type VideoEncodeFrameMetricsCallback<'a> =
    &'a mut dyn for<'frame> FnMut(VideoEncodeFrameMetrics<'frame>);

#[doc(hidden)]
pub type VideoEncodeSourceFn = for<'request, 'callback> fn(
    &mut dyn RawVideoFrameSource,
    &mut dyn Write,
    Option<&mut dyn Write>,
    VideoEncodeSourceRequest<'request>,
    Option<VideoEncodeFrameMetricsCallback<'callback>>,
) -> Result<()>;

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct VideoEncoderManifestHooks {
    #[doc(hidden)]
    pub create_session: VideoEncoderSessionFactory,
    #[doc(hidden)]
    pub encode_source: VideoEncodeSourceFn,
}

impl CodecId {
    /// Validate and create a codec id.
    ///
    /// Codec ids are lowercase ASCII names with optional digits and hyphens.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_codec_id(&value)?;
        Ok(Self(value))
    }

    /// Borrow the codec id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CodecId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CodecId").field(&self.0).finish()
    }
}

impl fmt::Display for CodecId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for CodecId {
    type Err = MediaError;

    fn from_str(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<&str> for CodecId {
    type Error = MediaError;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl FrameRate {
    /// Create a nonzero rational frame rate.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self> {
        if numerator == 0 || denominator == 0 {
            return Err(MediaError::Message(
                "frame rate numerator and denominator must be non-zero".to_string(),
            ));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

impl VideoRateControl {
    /// Create a constant-quantizer rate-control mode.
    ///
    /// The accepted public range is `1..=255`.
    pub fn constant_quantizer(qp: u8) -> Result<Self> {
        if qp == 0 {
            return Err(MediaError::Message(
                "constant quantizer must be in the range 1..255".to_string(),
            ));
        }
        Ok(Self::ConstantQuantizer(qp))
    }
}

impl VideoSettingValue {
    /// Render this typed value as a CLI-compatible setting value.
    pub fn as_cli_value(&self) -> String {
        match self {
            Self::Boolean(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
            Self::Text(value) => value.clone(),
        }
    }
}

impl From<bool> for VideoSettingValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<i64> for VideoSettingValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for VideoSettingValue {
    fn from(value: i32) -> Self {
        Self::Integer(i64::from(value))
    }
}

impl From<u8> for VideoSettingValue {
    fn from(value: u8) -> Self {
        Self::Integer(i64::from(value))
    }
}

impl From<String> for VideoSettingValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for VideoSettingValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl VideoEncoderSetting {
    /// Create a setting with a validated setting name and typed value.
    pub fn new(name: impl Into<String>, value: VideoSettingValue) -> Result<Self> {
        let name = name.into();
        validate_setting_name(&name)?;
        Ok(Self { name, value })
    }

    /// Create a boolean codec extension setting.
    pub fn boolean(name: impl Into<String>, value: bool) -> Result<Self> {
        Self::new(name, VideoSettingValue::Boolean(value))
    }

    /// Create an integer codec extension setting.
    pub fn integer(name: impl Into<String>, value: i64) -> Result<Self> {
        Self::new(name, VideoSettingValue::Integer(value))
    }

    /// Create a text or choice codec extension setting.
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        Self::new(name, VideoSettingValue::Text(value.into()))
    }

    /// Render this setting as a CLI-compatible `name=value` spec.
    pub fn as_cli_spec(&self) -> String {
        format!("{}={}", self.name, self.value.as_cli_value())
    }
}

impl VideoEncoderConfig {
    /// Create an encoder config using codec defaults for optional fields.
    pub fn new(codec: CodecId, input: FrameInfo) -> Self {
        Self {
            codec,
            input,
            frame_rate: None,
            frame_limit: None,
            rate_control: VideoRateControl::CodecDefault,
            reconstruction: ReconstructionMode::None,
            settings: Vec::new(),
        }
    }

    /// Create an encoder config from a textual codec id.
    pub fn for_codec(codec: impl AsRef<str>, input: FrameInfo) -> Result<Self> {
        Ok(Self::new(CodecId::new(codec.as_ref())?, input))
    }

    /// Set optional frame-rate metadata.
    pub fn with_frame_rate(mut self, frame_rate: FrameRate) -> Self {
        self.frame_rate = Some(frame_rate);
        self
    }

    /// Set an optional source/caller frame limit.
    pub fn with_frame_limit(mut self, frame_limit: usize) -> Self {
        self.frame_limit = Some(frame_limit);
        self
    }

    /// Set the requested rate-control mode.
    pub fn with_rate_control(mut self, rate_control: VideoRateControl) -> Self {
        self.rate_control = rate_control;
        self
    }

    /// Set the requested reconstruction mode.
    pub fn with_reconstruction(mut self, reconstruction: ReconstructionMode) -> Self {
        self.reconstruction = reconstruction;
        self
    }

    /// Append one codec extension setting.
    pub fn with_setting(mut self, setting: VideoEncoderSetting) -> Self {
        self.settings.push(setting);
        self
    }

    /// Return CLI-compatible setting specs represented by this config.
    pub fn setting_specs(&self) -> Vec<String> {
        let mut specs = Vec::new();
        match self.rate_control {
            VideoRateControl::CodecDefault => {}
            VideoRateControl::Lossless => specs.push("lossless=true".to_string()),
            VideoRateControl::ConstantQuantizer(qp) => specs.push(format!("qp={qp}")),
        }
        specs.extend(self.settings.iter().map(VideoEncoderSetting::as_cli_spec));
        specs
    }
}

impl EncodedVideoChunk {
    /// Create an encoded chunk without timing metadata.
    pub fn new(codec: CodecId, kind: VideoChunkKind, data: Vec<u8>) -> Self {
        Self {
            codec,
            kind,
            data,
            frame_index: None,
            pts: None,
            dts: None,
            duration: None,
            keyframe: false,
        }
    }
}

impl VideoEncodeOutput {
    /// Create output containing a single encoded chunk.
    pub fn from_chunk(chunk: EncodedVideoChunk) -> Self {
        Self {
            chunks: vec![chunk],
            reconstructions: Vec::new(),
            metrics: Vec::new(),
        }
    }
}

impl VideoEncoderManifest {
    #[doc(hidden)]
    pub const fn new(
        name: &'static str,
        feature: &'static str,
        summary: &'static str,
        settings: &'static [SettingManifest],
        accepts_format: fn(PixelFormat) -> bool,
        supports_lossless_format: fn(PixelFormat) -> bool,
        hooks: VideoEncoderManifestHooks,
    ) -> Self {
        Self {
            name,
            feature,
            summary,
            settings,
            accepts_format,
            supports_lossless_format,
            create_session_hook: hooks.create_session,
            encode_source_hook: hooks.encode_source,
        }
    }

    #[doc(hidden)]
    pub fn session_factory(self) -> VideoEncoderSessionFactory {
        self.create_session_hook
    }

    #[doc(hidden)]
    pub fn source_encode_hook(self) -> VideoEncodeSourceFn {
        self.encode_source_hook
    }

    /// Return this manifest's codec id as a validated [`CodecId`].
    pub fn codec_id(self) -> Result<CodecId> {
        CodecId::new(self.name)
    }

    /// Find a codec-specific setting manifest by name.
    pub fn setting(self, name: &str) -> Option<SettingManifest> {
        self.settings
            .iter()
            .copied()
            .find(|setting| setting.name == name)
    }

    /// Return whether this encoder accepts the frame format in `input`.
    pub fn accepts_frame_info(self, input: FrameInfo) -> bool {
        (self.accepts_format)(input.format)
    }

    /// Return whether this encoder supports lossless coding for `input`.
    pub fn supports_lossless_frame_info(self, input: FrameInfo) -> bool {
        (self.supports_lossless_format)(input.format)
    }

    /// Validate codec id, input format, rate control, and extension settings.
    pub fn validate_config(self, config: &VideoEncoderConfig) -> Result<()> {
        if config.codec.as_str() != self.name {
            return Err(MediaError::UnsupportedCodec {
                codec: config.codec.to_string(),
                reason: format!("manifest '{}' cannot create it", self.name),
            });
        }
        if !self.accepts_frame_info(config.input) {
            return Err(MediaError::UnsupportedPixelFormat {
                codec: self.name.to_string(),
                format: config.input.format.to_string(),
            });
        }
        if matches!(config.rate_control, VideoRateControl::Lossless)
            && !self.supports_lossless_frame_info(config.input)
        {
            return Err(MediaError::UnsupportedPixelFormat {
                codec: self.name.to_string(),
                format: format!("lossless {}", config.input.format),
            });
        }

        let mut seen = Vec::new();
        for setting in &config.settings {
            if setting.name == "lossless" {
                return Err(MediaError::ConflictingSettings {
                    setting: setting.name.clone(),
                    conflict: "VideoRateControl::Lossless".to_string(),
                });
            }
            if matches!(config.rate_control, VideoRateControl::ConstantQuantizer(_))
                && setting.name == "qp"
            {
                return Err(MediaError::ConflictingSettings {
                    setting: setting.name.clone(),
                    conflict: "VideoRateControl::ConstantQuantizer".to_string(),
                });
            }
            if matches!(config.rate_control, VideoRateControl::Lossless) && setting.name == "qp" {
                return Err(MediaError::ConflictingSettings {
                    setting: setting.name.clone(),
                    conflict: "lossless rate control".to_string(),
                });
            }
            if seen.contains(&setting.name.as_str()) {
                return Err(MediaError::DuplicateSetting {
                    setting: setting.name.clone(),
                });
            }
            seen.push(setting.name.as_str());
            let Some(manifest) = self.setting(&setting.name) else {
                return Err(MediaError::UnknownSetting {
                    codec: self.name.to_string(),
                    setting: setting.name.clone(),
                });
            };
            if !setting_value_matches_manifest(setting, manifest.value) {
                return Err(MediaError::InvalidSettingValue {
                    codec: self.name.to_string(),
                    setting: manifest.name.to_string(),
                    expected: crate::setting_values_label(manifest),
                    actual: setting.value.as_cli_value(),
                });
            }
        }
        Ok(())
    }
}

fn setting_value_matches_manifest(setting: &VideoEncoderSetting, manifest: SettingValue) -> bool {
    match (manifest, &setting.value) {
        (SettingValue::Boolean, VideoSettingValue::Boolean(_)) => true,
        (SettingValue::Choice(values), VideoSettingValue::Text(value)) => {
            values.contains(&value.as_str())
        }
        (SettingValue::IntegerRange { min, max }, VideoSettingValue::Integer(value)) => {
            (i64::from(min)..=i64::from(max)).contains(value)
        }
        _ => false,
    }
}

fn validate_codec_id(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(MediaError::Message("codec id cannot be empty".to_string()));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(MediaError::Message(format!(
            "codec id '{value}' must contain only lowercase ASCII letters, digits, or '-'"
        )));
    }
    Ok(())
}

fn validate_setting_name(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(MediaError::Message(
            "setting name cannot be empty".to_string(),
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(MediaError::Message(format!(
            "setting name '{value}' must contain only lowercase ASCII letters, digits, or '-'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PixelFormat, SettingManifest, SettingSpecForm, SettingSpecManifest, SettingValue};

    const PREDICTIVE_FORMS: &[SettingSpecForm] = &[SettingSpecForm {
        syntax: "predictive=<bool>",
        summary: "toggle prediction",
    }];
    const PREDICTIVE_SPEC: SettingSpecManifest = SettingSpecManifest {
        forms: PREDICTIVE_FORMS,
        examples: &[],
        notes: &[],
    };
    const PREDICTIVE_SETTING: SettingManifest = SettingManifest {
        name: "predictive",
        value: SettingValue::Boolean,
        spec: &PREDICTIVE_SPEC,
        summary: "toggle prediction",
    };
    const TEST_SETTINGS: &[SettingManifest] = &[PREDICTIVE_SETTING];

    const TEST_CODEC: VideoEncoderManifest = VideoEncoderManifest::new(
        "test",
        "test-codec",
        "test codec",
        TEST_SETTINGS,
        test_accepts_format,
        test_supports_lossless_format,
        VideoEncoderManifestHooks {
            create_session: test_create_session,
            encode_source: test_encode_source,
        },
    );

    struct TestSession {
        config: VideoEncoderConfig,
    }

    impl VideoEncoderSession for TestSession {
        fn codec(&self) -> &CodecId {
            &self.config.codec
        }

        fn config(&self) -> &VideoEncoderConfig {
            &self.config
        }

        fn encode_frame(&mut self, _frame: Frame) -> Result<VideoEncodeOutput> {
            Ok(VideoEncodeOutput::default())
        }
    }

    fn test_accepts_format(format: PixelFormat) -> bool {
        format == PixelFormat::Yuv420p8
    }

    fn test_supports_lossless_format(format: PixelFormat) -> bool {
        test_accepts_format(format)
    }

    fn test_create_session(config: VideoEncoderConfig) -> Result<Box<dyn VideoEncoderSession>> {
        Ok(Box::new(TestSession { config }))
    }

    fn test_encode_source(
        _source: &mut dyn RawVideoFrameSource,
        _output: &mut dyn Write,
        _recon: Option<&mut dyn Write>,
        _request: VideoEncodeSourceRequest<'_>,
        _frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
    ) -> Result<()> {
        Ok(())
    }

    #[test]
    fn codec_id_accepts_stable_lowercase_names() {
        assert_eq!(CodecId::new("av2").unwrap().as_str(), "av2");
        assert_eq!(CodecId::new("vvc-main10").unwrap().as_str(), "vvc-main10");
    }

    #[test]
    fn codec_id_rejects_names_that_do_not_round_trip_to_cli() {
        assert!(CodecId::new("").is_err());
        assert!(CodecId::new("AV2").is_err());
        assert!(CodecId::new("vvc/main10").is_err());
    }

    #[test]
    fn config_collects_rate_control_and_extension_settings_as_specs() {
        let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("vvc").unwrap(), info)
            .with_rate_control(VideoRateControl::constant_quantizer(24).unwrap())
            .with_setting(VideoEncoderSetting::boolean("predictive", true).unwrap())
            .with_setting(VideoEncoderSetting::text("fast-search", "moderate").unwrap());

        assert_eq!(
            config.setting_specs(),
            vec![
                "qp=24".to_string(),
                "predictive=true".to_string(),
                "fast-search=moderate".to_string()
            ]
        );
    }

    #[test]
    fn lossless_rate_control_maps_to_a_normal_setting_spec() {
        let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("av2").unwrap(), info)
            .with_rate_control(VideoRateControl::Lossless);

        assert_eq!(config.setting_specs(), vec!["lossless=true".to_string()]);
    }

    #[test]
    fn manifest_validates_codec_format_and_settings() {
        let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("test").unwrap(), info)
            .with_setting(VideoEncoderSetting::boolean("predictive", true).unwrap());

        TEST_CODEC
            .validate_config(&config)
            .expect("valid manifest config");
    }

    #[test]
    fn manifest_rejects_unknown_and_duplicate_settings() {
        let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8).unwrap();
        let unknown = VideoEncoderConfig::new(CodecId::new("test").unwrap(), info)
            .with_setting(VideoEncoderSetting::boolean("missing", true).unwrap());
        let duplicate = VideoEncoderConfig::new(CodecId::new("test").unwrap(), info)
            .with_setting(VideoEncoderSetting::boolean("predictive", true).unwrap())
            .with_setting(VideoEncoderSetting::boolean("predictive", false).unwrap());

        assert!(matches!(
            TEST_CODEC.validate_config(&unknown).unwrap_err(),
            MediaError::UnknownSetting { codec, setting }
                if codec == "test" && setting == "missing"
        ));
        assert!(matches!(
            TEST_CODEC.validate_config(&duplicate).unwrap_err(),
            MediaError::DuplicateSetting { setting } if setting == "predictive"
        ));
    }

    #[test]
    fn manifest_rejects_invalid_setting_values_and_conflicts() {
        let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8).unwrap();
        let invalid_value = VideoEncoderConfig::new(CodecId::new("test").unwrap(), info)
            .with_setting(VideoEncoderSetting::text("predictive", "maybe").unwrap());
        let conflicting_qp = VideoEncoderConfig::new(CodecId::new("test").unwrap(), info)
            .with_rate_control(VideoRateControl::constant_quantizer(24).unwrap())
            .with_setting(VideoEncoderSetting::integer("qp", 19).unwrap());

        assert!(matches!(
            TEST_CODEC.validate_config(&invalid_value).unwrap_err(),
            MediaError::InvalidSettingValue {
                codec,
                setting,
                expected,
                actual,
            } if codec == "test"
                && setting == "predictive"
                && expected == "true|false"
                && actual == "maybe"
        ));
        assert!(matches!(
            TEST_CODEC.validate_config(&conflicting_qp).unwrap_err(),
            MediaError::ConflictingSettings { setting, conflict }
                if setting == "qp" && conflict == "VideoRateControl::ConstantQuantizer"
        ));
    }

    #[test]
    fn manifest_rejects_codec_and_format_mismatches_structurally() {
        let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8).unwrap();
        let wrong_codec = VideoEncoderConfig::new(CodecId::new("other").unwrap(), info);
        let wrong_format = VideoEncoderConfig::new(
            CodecId::new("test").unwrap(),
            FrameInfo::new(16, 16, PixelFormat::Rgb24).unwrap(),
        );

        assert!(matches!(
            TEST_CODEC.validate_config(&wrong_codec).unwrap_err(),
            MediaError::UnsupportedCodec { codec, .. } if codec == "other"
        ));
        assert!(matches!(
            TEST_CODEC.validate_config(&wrong_format).unwrap_err(),
            MediaError::UnsupportedPixelFormat { codec, format }
                if codec == "test" && format == "rgb24"
        ));
    }

    #[test]
    fn raw_video_read_source_reports_short_frames_structurally() {
        let mut source = RawVideoFrameReadSource::new(std::io::Cursor::new(vec![1, 2, 3]));
        let mut frame = [0u8; 4];
        let err = source.read_frame(&mut frame).unwrap_err();

        assert!(matches!(
            err,
            MediaError::ShortFrameRead {
                expected: 4,
                actual: 3,
            }
        ));
    }
}
