use std::fmt;
use std::io::{Read, Write};
use std::str::FromStr;

use crate::{
    Frame, FrameInfo, MediaError, PixelFormat, Result, SettingManifest, SettingValue, Timestamp,
};

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CodecId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoRateControl {
    #[default]
    CodecDefault,
    Lossless,
    ConstantQuantizer(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReconstructionMode {
    #[default]
    None,
    MetricsOnly,
    Frames,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoSettingValue {
    Boolean(bool),
    Integer(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoEncoderSetting {
    pub name: String,
    pub value: VideoSettingValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoEncoderConfig {
    pub codec: CodecId,
    pub input: FrameInfo,
    pub frame_rate: Option<FrameRate>,
    pub frame_limit: Option<usize>,
    pub rate_control: VideoRateControl,
    pub reconstruction: ReconstructionMode,
    pub settings: Vec<VideoEncoderSetting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoChunkKind {
    Config,
    Frame,
    EndOfStream,
    Stream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedVideoChunk {
    pub codec: CodecId,
    pub kind: VideoChunkKind,
    pub data: Vec<u8>,
    pub frame_index: Option<usize>,
    pub pts: Option<Timestamp>,
    pub dts: Option<Timestamp>,
    pub duration: Option<Timestamp>,
    pub keyframe: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameEncodeMetrics {
    pub frame_index: usize,
    pub frame_count: Option<usize>,
    pub encoded_bytes: usize,
    pub psnr: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VideoEncodeOutput {
    pub chunks: Vec<EncodedVideoChunk>,
    pub reconstructions: Vec<Frame>,
    pub metrics: Vec<FrameEncodeMetrics>,
}

pub trait VideoEncoderSession {
    fn codec(&self) -> &CodecId;

    fn config(&self) -> &VideoEncoderConfig;

    fn encode_frame(&mut self, frame: Frame) -> Result<VideoEncodeOutput>;

    fn flush(&mut self) -> Result<VideoEncodeOutput> {
        Ok(VideoEncodeOutput::default())
    }
}

pub type VideoEncoderSessionFactory =
    fn(VideoEncoderConfig) -> Result<Box<dyn VideoEncoderSession>>;

pub trait RawVideoFrameSource {
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

pub struct RawVideoFrameReadSource<R> {
    inner: R,
}

impl<R: Read> RawVideoFrameReadSource<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

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

#[derive(Debug, Clone, Copy)]
pub struct VideoEncoderManifest {
    pub name: &'static str,
    pub feature: &'static str,
    pub summary: &'static str,
    pub settings: &'static [SettingManifest],
    pub accepts_format: fn(PixelFormat) -> bool,
    pub supports_lossless_format: fn(PixelFormat) -> bool,
    pub create_session: VideoEncoderSessionFactory,
    pub encode_source: VideoEncodeSourceFn,
}

#[derive(Debug, Clone, Copy)]
pub struct VideoEncodeSourceRequest<'a> {
    pub config: &'a VideoEncoderConfig,
}

pub struct VideoEncodeFrameMetrics<'a> {
    pub frame_idx: usize,
    pub frame_count: Option<usize>,
    pub bitstream_bytes: usize,
    pub source: &'a [u8],
    pub reconstruction: &'a [u8],
}

pub type VideoEncodeFrameMetricsCallback<'a> =
    &'a mut dyn for<'frame> FnMut(VideoEncodeFrameMetrics<'frame>);

pub type VideoEncodeSourceFn = for<'request> fn(
    &mut dyn RawVideoFrameSource,
    &mut dyn Write,
    Option<&mut dyn Write>,
    VideoEncodeSourceRequest<'request>,
    Option<VideoEncodeFrameMetricsCallback<'request>>,
) -> Result<()>;

impl CodecId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_codec_id(&value)?;
        Ok(Self(value))
    }

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
    pub fn as_cli_value(&self) -> String {
        match self {
            Self::Boolean(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
            Self::Text(value) => value.clone(),
        }
    }
}

impl VideoEncoderSetting {
    pub fn new(name: impl Into<String>, value: VideoSettingValue) -> Result<Self> {
        let name = name.into();
        validate_setting_name(&name)?;
        Ok(Self { name, value })
    }

    pub fn boolean(name: impl Into<String>, value: bool) -> Result<Self> {
        Self::new(name, VideoSettingValue::Boolean(value))
    }

    pub fn integer(name: impl Into<String>, value: i64) -> Result<Self> {
        Self::new(name, VideoSettingValue::Integer(value))
    }

    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        Self::new(name, VideoSettingValue::Text(value.into()))
    }

    pub fn as_cli_spec(&self) -> String {
        format!("{}={}", self.name, self.value.as_cli_value())
    }
}

impl VideoEncoderConfig {
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

    pub fn with_frame_rate(mut self, frame_rate: FrameRate) -> Self {
        self.frame_rate = Some(frame_rate);
        self
    }

    pub fn with_frame_limit(mut self, frame_limit: usize) -> Self {
        self.frame_limit = Some(frame_limit);
        self
    }

    pub fn with_rate_control(mut self, rate_control: VideoRateControl) -> Self {
        self.rate_control = rate_control;
        self
    }

    pub fn with_reconstruction(mut self, reconstruction: ReconstructionMode) -> Self {
        self.reconstruction = reconstruction;
        self
    }

    pub fn with_setting(mut self, setting: VideoEncoderSetting) -> Self {
        self.settings.push(setting);
        self
    }

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
    pub fn from_chunk(chunk: EncodedVideoChunk) -> Self {
        Self {
            chunks: vec![chunk],
            reconstructions: Vec::new(),
            metrics: Vec::new(),
        }
    }
}

impl VideoEncoderManifest {
    pub fn codec_id(self) -> Result<CodecId> {
        CodecId::new(self.name)
    }

    pub fn setting(self, name: &str) -> Option<SettingManifest> {
        self.settings
            .iter()
            .copied()
            .find(|setting| setting.name == name)
    }

    pub fn accepts_frame_info(self, input: FrameInfo) -> bool {
        (self.accepts_format)(input.format)
    }

    pub fn supports_lossless_frame_info(self, input: FrameInfo) -> bool {
        (self.supports_lossless_format)(input.format)
    }

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

    pub fn create_encoder(
        self,
        config: VideoEncoderConfig,
    ) -> Result<Box<dyn VideoEncoderSession>> {
        self.validate_config(&config)?;
        (self.create_session)(config)
    }

    pub fn encode_source<'a>(
        self,
        source: &mut dyn RawVideoFrameSource,
        output: &mut dyn Write,
        recon: Option<&mut dyn Write>,
        config: &'a VideoEncoderConfig,
        frame_metrics: Option<VideoEncodeFrameMetricsCallback<'a>>,
    ) -> Result<()> {
        self.validate_config(config)?;
        (self.encode_source)(
            source,
            output,
            recon,
            VideoEncodeSourceRequest { config },
            frame_metrics,
        )
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

    const TEST_CODEC: VideoEncoderManifest = VideoEncoderManifest {
        name: "test",
        feature: "test-codec",
        summary: "test codec",
        settings: TEST_SETTINGS,
        accepts_format: test_accepts_format,
        supports_lossless_format: test_supports_lossless_format,
        create_session: test_create_session,
        encode_source: test_encode_source,
    };

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
