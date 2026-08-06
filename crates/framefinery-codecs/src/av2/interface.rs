use std::io::{Read, Write};

use framefinery_core::{
    boolean_setting_enabled, setting_name, u8_setting, ChromaSampling, PixelFormat,
    RawVideoFrameSource, SettingManifest, VideoEncodeFrameMetrics, VideoEncodeFrameMetricsCallback,
    VideoEncodeSourceRequest, VideoEncoderManifest, VideoEncoderManifestHooks,
};

use super::{
    av2_encode_fixed_black_444_with_options_and_frame_metrics, Av2EncodeFrameMetrics,
    Av2EncodeOptions, Av2EncodeParams, Av2EncodeRequest, Av2VideoGeometry,
};
use crate::session::{
    buffered_stream_session, encode_stream_from_source, StreamEncoderManifest,
    VideoEncodeStreamRequest,
};
use crate::settings::{PREDICTIVE_SETTING, QP_SETTING};

const AV2_SETTINGS: &[SettingManifest] = &[QP_SETTING, PREDICTIVE_SETTING];

pub const AV2_CODEC: VideoEncoderManifest = VideoEncoderManifest::new(
    "av2",
    "codec-av2",
    "local experimental FrameFinery AV2 encoder",
    AV2_SETTINGS,
    av2_accepts_format,
    av2_supports_lossless_format,
    VideoEncoderManifestHooks {
        create_session: create_av2_session,
        encode_source: encode_av2_source,
    },
);

pub(crate) const AV2_STREAM_ENCODER: StreamEncoderManifest = StreamEncoderManifest {
    public: AV2_CODEC,
    encode_stream: encode_av2_with_manifest,
};

fn create_av2_session(
    config: framefinery_core::VideoEncoderConfig,
) -> framefinery_core::Result<Box<dyn framefinery_core::VideoEncoderSession>> {
    buffered_stream_session(AV2_STREAM_ENCODER, config)
}

fn encode_av2_source(
    source: &mut dyn RawVideoFrameSource,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: VideoEncodeSourceRequest<'_>,
    frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
) -> framefinery_core::Result<()> {
    encode_stream_from_source(
        AV2_STREAM_ENCODER,
        source,
        output,
        recon,
        request,
        frame_metrics,
    )
}

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
    request: VideoEncodeStreamRequest<'_>,
    frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
) -> Result<(), String> {
    let options = av2_options_from_settings(request.lossless, request.settings)?;
    let request = Av2EncodeRequest {
        params: Av2EncodeParams {
            frames: request.frame_limit.unwrap_or(0),
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
    use framefinery_core::{
        CodecId, FrameInfo, MediaError, ReconstructionMode, VideoEncoderConfig, VideoRateControl,
    };

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

    #[test]
    fn source_encode_can_read_until_eof_without_frame_limit() {
        let info = FrameInfo::new(8, 8, PixelFormat::Yuv420p8).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("av2").unwrap(), info)
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
            .expect("AV2 source encode without frame limit");

        assert!(!output.is_empty());
        assert_eq!(frame_counts, vec![None]);
    }
}
