use std::io::{Read, Write};

use framefinery_core::{
    boolean_setting_enabled, setting_name, u8_setting, ChromaSampling, CodecEncodeFrameMetrics,
    CodecEncodeFrameMetricsCallback, CodecEncodeRequest, CodecManifest, PixelFormat,
    SettingManifest, SettingSpecExample, SettingSpecForm, SettingSpecManifest, SettingValue,
};

use super::{
    vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics, VvcEncodeFrameMetrics,
    VvcEncodeOptions, VvcEncodeParams, VvcFastSearch, VvcVideoGeometry, VvcVideoLimits,
};
use crate::settings::{PREDICTIVE_SETTING, QP_SETTING};

const VVC_FAST_SEARCH_SPEC_FORMS: &[SettingSpecForm] = &[SettingSpecForm {
    syntax: "fast-search=<mode>",
    summary: "select VVC spatially guided mode-search pruning",
}];

const VVC_FAST_SEARCH_SPEC_EXAMPLES: &[SettingSpecExample] = &[
    SettingSpecExample {
        spec: "fast-search=moderate",
        summary: "use moderate VVC mode-search pruning",
    },
    SettingSpecExample {
        spec: "fast-search=lossless-speed",
        summary: "use the fastest currently available VVC lossless search mode",
    },
];

const VVC_FAST_SEARCH_SPEC_NOTES: &[&str] =
    &["experimental VVC-only tuning setting; compare bytes, fps, and PSNR before adopting"];

pub const VVC_FAST_SEARCH_SETTING_SPEC: SettingSpecManifest = SettingSpecManifest {
    forms: VVC_FAST_SEARCH_SPEC_FORMS,
    examples: VVC_FAST_SEARCH_SPEC_EXAMPLES,
    notes: VVC_FAST_SEARCH_SPEC_NOTES,
};

pub const VVC_FAST_SEARCH_SETTING: SettingManifest = SettingManifest {
    name: "fast-search",
    value: SettingValue::Choice(VvcFastSearch::VALUES),
    spec: &VVC_FAST_SEARCH_SETTING_SPEC,
    summary: "enable experimental VVC spatially guided mode-search pruning",
};

const VVC_SETTINGS: &[SettingManifest] = &[QP_SETTING, PREDICTIVE_SETTING, VVC_FAST_SEARCH_SETTING];

pub const VVC_CODEC: CodecManifest = CodecManifest {
    name: "vvc",
    feature: "codec-vvc",
    summary: "local experimental FrameFinery VVC/H.266 encoder",
    settings: VVC_SETTINGS,
    accepts_format: vvc_accepts_format,
    supports_lossless_format: vvc_supports_lossless_format,
    encode: encode_vvc_with_manifest,
};

fn vvc_accepts_format(format: PixelFormat) -> bool {
    format == PixelFormat::Gbrp8
        || match format.chroma_sampling() {
            Some(ChromaSampling::Cs420 | ChromaSampling::Cs422 | ChromaSampling::Cs444) => {
                vvc_accepts_bit_depth(format)
            }
            _ => false,
        }
}

fn vvc_supports_lossless_format(format: PixelFormat) -> bool {
    vvc_accepts_format(format)
}

fn vvc_accepts_bit_depth(format: PixelFormat) -> bool {
    (8..=12).contains(&format.bit_depth().bits())
}

fn encode_vvc_with_manifest(
    input: &mut dyn Read,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: CodecEncodeRequest<'_>,
    frame_metrics: Option<CodecEncodeFrameMetricsCallback<'_>>,
) -> Result<(), String> {
    if !request.format.is_yuv() && request.format != PixelFormat::Gbrp8 {
        return Err(format!(
            "VVC encoder expects planar YUV or gbrp8 input; got {}x{} {}",
            request.width, request.height, request.format
        ));
    }

    let options = vvc_options_from_settings(request.lossless, request.settings)?;
    let params = VvcEncodeParams {
        frames: request.frames,
    };
    let geometry = VvcVideoGeometry {
        width: request.width,
        height: request.height,
    };
    let limits = VvcVideoLimits::unbounded();
    geometry.validate_against(limits)?;
    let has_frame_metrics = frame_metrics.is_some();
    let mut frame_metrics = frame_metrics;
    let mut callback = |metrics: VvcEncodeFrameMetrics<'_>| {
        if let Some(callback) = frame_metrics.as_mut() {
            callback(CodecEncodeFrameMetrics {
                frame_idx: metrics.frame_idx,
                frame_count: metrics.frame_count,
                bitstream_bytes: metrics.bitstream_bytes,
                source: metrics.source,
                reconstruction: metrics.reconstruction,
            });
        }
    };
    let callback = if has_frame_metrics {
        Some(&mut callback as &mut dyn for<'a> FnMut(VvcEncodeFrameMetrics<'a>))
    } else {
        None
    };
    vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics(
        input,
        output,
        recon,
        params,
        geometry,
        limits,
        request.format,
        options,
        callback,
    )
}

fn vvc_options_from_settings(
    lossless: bool,
    settings: &[String],
) -> Result<VvcEncodeOptions, String> {
    let qp = u8_setting(settings, "qp")?;
    if lossless && qp.is_some() {
        return Err("--set qp=<1..255> is mutually exclusive with --set lossless".to_string());
    }
    let predictive = boolean_setting_enabled(settings, "predictive")?;
    let fast_search = vvc_fast_search_setting(settings)?;
    for spec in settings {
        let name = setting_name(spec);
        if !matches!(name, "lossless" | "qp" | "predictive" | "fast-search") {
            return Err(format!("VVC encoder received unknown setting '{name}'"));
        }
    }
    Ok(VvcEncodeOptions {
        lossless,
        qp,
        predictive,
        fast_search,
    })
}

fn vvc_fast_search_setting(settings: &[String]) -> Result<VvcFastSearch, String> {
    for spec in settings {
        if setting_name(spec) != "fast-search" {
            continue;
        }
        let value = framefinery_core::setting_value(spec).unwrap_or("true");
        return value.parse::<VvcFastSearch>();
    }
    Ok(VvcFastSearch::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fast_search_setting() {
        let options = vvc_options_from_settings(false, &["fast-search=moderate".to_string()])
            .expect("parse VVC settings");
        assert_eq!(options.fast_search, VvcFastSearch::Moderate);
    }

    #[test]
    fn parses_qp_and_predictive_settings() {
        let options =
            vvc_options_from_settings(false, &["qp=19".to_string(), "predictive".to_string()])
                .expect("parse VVC settings");
        assert_eq!(options.qp, Some(19));
        assert!(options.predictive);
    }

    #[test]
    fn rejects_qp_with_lossless() {
        let err = vvc_options_from_settings(true, &["qp=16".to_string()])
            .expect_err("QP and lossless should conflict");
        assert!(
            err.contains("--set qp=<1..255> is mutually exclusive with --set lossless"),
            "{err}"
        );
    }
}
