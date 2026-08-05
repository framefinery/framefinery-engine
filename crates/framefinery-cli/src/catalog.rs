pub use framefinery_core::{
    filter_manifest as filter, setting_values_label, FilterManifest, FilterSpecValue,
    FilterStageKind, SettingManifest, FILTERS, GLOBAL_SETTINGS,
};

#[cfg(feature = "video-encoders")]
pub use framefinery_codecs::{encoder, ENCODERS};

#[cfg(not(feature = "video-encoders"))]
use framefinery_core::VideoEncoderManifest;

#[cfg(not(feature = "video-encoders"))]
pub const ENCODERS: &[VideoEncoderManifest] = &[];

#[cfg(not(feature = "video-encoders"))]
pub fn encoder(_name: &str) -> Option<VideoEncoderManifest> {
    None
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
