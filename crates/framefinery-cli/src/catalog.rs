pub use framefinery_core::{
    filter_manifest as filter, setting_values_label, FilterManifest, FilterSpecValue,
    FilterStageKind, SettingManifest, FILTERS, GLOBAL_SETTINGS,
};

#[cfg(feature = "video-encoders")]
pub use framefinery_codecs::{encode_source, find_encoder_manifest, ENCODERS};

#[cfg(not(feature = "video-encoders"))]
use std::io::Write;

#[cfg(not(feature = "video-encoders"))]
use framefinery_core::{
    MediaError, RawVideoFrameSource, Result, VideoEncodeFrameMetricsCallback, VideoEncoderConfig,
    VideoEncoderManifest,
};

#[cfg(not(feature = "video-encoders"))]
pub const ENCODERS: &[VideoEncoderManifest] = &[];

#[cfg(not(feature = "video-encoders"))]
pub fn find_encoder_manifest(_name: &str) -> Option<VideoEncoderManifest> {
    None
}

#[cfg(not(feature = "video-encoders"))]
pub fn encode_source<'a>(
    config: &'a VideoEncoderConfig,
    _source: &mut dyn RawVideoFrameSource,
    _output: &mut dyn Write,
    _recon: Option<&mut dyn Write>,
    _frame_metrics: Option<VideoEncodeFrameMetricsCallback<'a>>,
) -> Result<()> {
    Err(MediaError::UnsupportedCodec {
        codec: config.codec.to_string(),
        reason: "no encoder with this codec id is compiled into this build".to_string(),
    })
}

pub fn global_setting(name: &str) -> Option<SettingManifest> {
    GLOBAL_SETTINGS
        .iter()
        .copied()
        .find(|setting| setting.name == name)
}

pub fn settings_label(global: &[SettingManifest], codec: &[SettingManifest]) -> String {
    let mut names = Vec::new();
    for setting in global.iter().chain(codec.iter()) {
        if !names.contains(&setting.name) {
            names.push(setting.name);
        }
    }
    if names.is_empty() {
        "-".to_string()
    } else {
        names.join(",")
    }
}
