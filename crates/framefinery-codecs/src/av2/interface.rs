use std::io::{Read, Write};

use framefinery_core::{
    boolean_setting_enabled, setting_name, u8_setting, ChromaSampling, CodecEncodeFrameMetrics,
    CodecEncodeFrameMetricsCallback, CodecEncodeRequest, CodecManifest, PixelFormat,
    SettingManifest,
};

use super::{
    av2_encode_fixed_black_444_with_options_and_frame_metrics, Av2EncodeFrameMetrics,
    Av2EncodeOptions, Av2EncodeParams, Av2EncodeRequest, Av2VideoGeometry,
};
use crate::settings::{PREDICTIVE_SETTING, QP_SETTING};

const AV2_SETTINGS: &[SettingManifest] = &[QP_SETTING, PREDICTIVE_SETTING];

pub const AV2_CODEC: CodecManifest = CodecManifest {
    name: "av2",
    feature: "codec-av2",
    summary: "local experimental FrameFinery AV2 encoder",
    settings: AV2_SETTINGS,
    accepts_format: av2_accepts_format,
    supports_lossless_format: av2_supports_lossless_format,
    encode: encode_av2_with_manifest,
};

fn av2_accepts_format(format: PixelFormat) -> bool {
    matches!(format, PixelFormat::Gbrp8 | PixelFormat::Rgb24)
        || (matches!(
            format.chroma_sampling(),
            Some(ChromaSampling::Cs420 | ChromaSampling::Cs422 | ChromaSampling::Cs444)
        ) && matches!(format.bit_depth().bits(), 8 | 10))
}

fn av2_supports_lossless_format(format: PixelFormat) -> bool {
    av2_accepts_format(format)
}

fn encode_av2_with_manifest(
    input: &mut dyn Read,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: CodecEncodeRequest<'_>,
    frame_metrics: Option<CodecEncodeFrameMetricsCallback<'_>>,
) -> Result<(), String> {
    let options = av2_options_from_settings(request.lossless, request.settings)?;
    let request = Av2EncodeRequest {
        params: Av2EncodeParams {
            frames: request.frames,
        },
        geometry: Av2VideoGeometry {
            width: request.width,
            height: request.height,
        },
        format: request.format,
    };
    let has_frame_metrics = frame_metrics.is_some();
    let mut frame_metrics = frame_metrics;
    let mut callback = |metrics: Av2EncodeFrameMetrics<'_>| {
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
        Some(&mut callback as &mut dyn for<'a> FnMut(Av2EncodeFrameMetrics<'a>))
    } else {
        None
    };
    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        input, output, recon, request, options, callback,
    )
}

fn av2_options_from_settings(
    lossless: bool,
    settings: &[String],
) -> Result<Av2EncodeOptions, String> {
    let qp = u8_setting(settings, "qp")?;
    if lossless && qp.is_some() {
        return Err("--set qp=<1..255> is mutually exclusive with --set lossless".to_string());
    }
    let predictive = boolean_setting_enabled(settings, "predictive")?;
    for spec in settings {
        let name = setting_name(spec);
        if !matches!(name, "lossless" | "qp" | "predictive") {
            return Err(format!("AV2 encoder received unknown setting '{name}'"));
        }
    }
    Ok(Av2EncodeOptions {
        lossless,
        qp,
        predictive,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_qp_and_predictive_settings() {
        let options =
            av2_options_from_settings(false, &["qp=24".to_string(), "predictive".to_string()])
                .expect("parse AV2 settings");
        assert_eq!(options.qp, Some(24));
        assert!(options.predictive);
    }

    #[test]
    fn rejects_qp_with_lossless() {
        let err = av2_options_from_settings(true, &["qp=16".to_string()])
            .expect_err("QP and lossless should conflict");
        assert!(
            err.contains("--set qp=<1..255> is mutually exclusive with --set lossless"),
            "{err}"
        );
    }
}
