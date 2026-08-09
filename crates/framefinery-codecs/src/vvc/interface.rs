use std::io::{Read, Write};

use framefinery_api::{
    setting_name, u8_setting, ChromaSampling, PixelFormat, RawVideoFrameSource, SettingManifest,
    SettingSpecExample, SettingSpecForm, SettingSpecManifest, SettingValue,
    VideoEncodeFrameMetrics, VideoEncodeFrameMetricsCallback, VideoEncodeSourceRequest,
    VideoEncoderManifest, VideoEncoderManifestHooks,
};

use super::{
    vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics, VvcEncodeFrameMetrics,
    VvcEncodeOptions, VvcEncodeParams, VvcFastSearch, VvcVideoGeometry, VvcVideoLimits,
};
use crate::session::{
    buffered_stream_session, encode_stream_from_source, StreamEncoderManifest,
    VideoEncodeStreamRequest,
};
use crate::settings::{GopMode, GOP_SETTING, QP_SETTING};

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
    default_value: Some("lossless-speed"),
    spec: &VVC_FAST_SEARCH_SETTING_SPEC,
    summary: "enable experimental VVC spatially guided mode-search pruning",
};

const VVC_SETTINGS: &[SettingManifest] = &[QP_SETTING, GOP_SETTING, VVC_FAST_SEARCH_SETTING];

pub const VVC_CODEC: VideoEncoderManifest = VideoEncoderManifest::new(
    "vvc",
    "codec-vvc",
    "local experimental FrameFinery VVC/H.266 encoder",
    VVC_SETTINGS,
    vvc_accepts_format,
    vvc_supports_lossless_format,
    VideoEncoderManifestHooks {
        create_session: create_vvc_session,
        encode_source: encode_vvc_source,
    },
);

pub(crate) const VVC_STREAM_ENCODER: StreamEncoderManifest = StreamEncoderManifest {
    public: VVC_CODEC,
    encode_stream: encode_vvc_with_manifest,
};

fn create_vvc_session(
    config: framefinery_api::VideoEncoderConfig,
) -> framefinery_api::Result<Box<dyn framefinery_api::VideoEncoderSession>> {
    buffered_stream_session(VVC_STREAM_ENCODER, config)
}

fn encode_vvc_source(
    source: &mut dyn RawVideoFrameSource,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: VideoEncodeSourceRequest<'_>,
    frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
) -> framefinery_api::Result<()> {
    encode_stream_from_source(
        VVC_STREAM_ENCODER,
        source,
        output,
        recon,
        request,
        frame_metrics,
    )
}

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
    request: VideoEncodeStreamRequest<'_>,
    frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
) -> Result<(), String> {
    if !request.format.is_yuv() && request.format != PixelFormat::Gbrp8 {
        return Err(format!(
            "VVC encoder expects planar YUV or gbrp8 input; got {}x{} {}",
            request.width, request.height, request.format
        ));
    }

    let options = vvc_options_from_settings(request.lossless, request.settings)?;
    let params = VvcEncodeParams {
        frames: request.frame_limit.unwrap_or(0),
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
            callback(VideoEncodeFrameMetrics {
                frame_idx: metrics.frame_idx,
                frame_count: metrics.frame_count,
                bitstream_bytes: metrics.bitstream_bytes,
                total_bitstream_bytes: metrics.total_bitstream_bytes,
                encode_elapsed: metrics.encode_elapsed,
                psnr: None,
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
        return Err(
            "--set qp=<1..255> is mutually exclusive with lossless rate control".to_string(),
        );
    }
    let gop = GopMode::from_settings(settings)?;
    let fast_search = vvc_fast_search_setting(settings)?;
    for spec in settings {
        let name = setting_name(spec);
        if !matches!(name, "lossless" | "qp" | "gop" | "fast-search") {
            return Err(format!("VVC encoder received unknown setting '{name}'"));
        }
    }
    Ok(VvcEncodeOptions {
        lossless,
        qp,
        gop,
        fast_search,
    })
}

fn vvc_fast_search_setting(settings: &[String]) -> Result<VvcFastSearch, String> {
    for spec in settings {
        if setting_name(spec) != "fast-search" {
            continue;
        }
        let value = framefinery_api::setting_value(spec).unwrap_or("true");
        return value.parse::<VvcFastSearch>();
    }
    Ok(VvcFastSearch::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use framefinery_api::{
        CodecId, FrameInfo, MediaError, ReconstructionMode, VideoEncoderConfig, VideoRateControl,
    };

    #[test]
    fn parses_fast_search_setting() {
        let options = vvc_options_from_settings(false, &["fast-search=moderate".to_string()])
            .expect("parse VVC settings");
        assert_eq!(options.fast_search, VvcFastSearch::Moderate);
    }

    #[test]
    fn parses_qp_and_gop_settings() {
        let options =
            vvc_options_from_settings(false, &["qp=19".to_string(), "gop=30".to_string()])
                .expect("parse VVC settings");
        assert_eq!(options.qp, Some(19));
        assert_eq!(options.gop, GopMode::Fixed(30));
    }

    #[test]
    fn defaults_to_infinite_gop() {
        let options = vvc_options_from_settings(false, &[]).expect("parse VVC settings");
        assert_eq!(options.gop, GopMode::Infinite);
    }

    #[test]
    fn rejects_qp_with_lossless() {
        let err = vvc_options_from_settings(true, &["qp=16".to_string()])
            .expect_err("QP and lossless should conflict");
        assert!(
            err.contains("--set qp=<1..255> is mutually exclusive with lossless rate control"),
            "{err}"
        );
    }

    #[test]
    fn source_encode_can_read_until_eof_without_frame_limit() {
        let info = FrameInfo::new(8, 8, PixelFormat::Yuv420p8).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("vvc").unwrap(), info)
            .with_rate_control(VideoRateControl::Lossless)
            .with_reconstruction(ReconstructionMode::MetricsOnly);
        let source_frame = vec![0u8; info.expected_len()];
        let mut emitted = false;
        let mut source = |frame: &mut [u8]| -> Result<bool, MediaError> {
            if emitted {
                return Ok(false);
            }
            frame.copy_from_slice(&source_frame);
            emitted = true;
            Ok(true)
        };
        let mut output = Vec::new();
        let mut frame_counts = Vec::new();
        let mut metrics = |metrics: VideoEncodeFrameMetrics<'_>| {
            frame_counts.push(metrics.frame_count);
        };

        crate::encode_source(&config, &mut source, &mut output, None, Some(&mut metrics))
            .expect("VVC source encode without frame limit");

        assert!(!output.is_empty());
        assert_eq!(frame_counts, vec![None]);
    }
}
