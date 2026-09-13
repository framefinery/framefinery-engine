use std::io::{Read, Write};
#[cfg(test)]
use std::time::Duration;

use framefinery_api::{
    frame_psnr, MediaError, RawVideoFrameSource, RawVideoFrameSourceReadAdapter,
    ReconstructionMode, Result, VideoEncodeFrameMetrics, VideoEncodeFrameMetricsCallback,
    VideoEncodeSourceRequest, VideoEncoderConfig, VideoEncoderManifest, VideoRateControl,
};

pub(crate) type VideoEncodeStreamFn =
    for<'request, 'callback> fn(
        &mut dyn Read,
        &mut dyn Write,
        Option<&mut dyn Write>,
        VideoEncodeStreamRequest<'request>,
        Option<VideoEncodeFrameMetricsCallback<'callback>>,
    ) -> std::result::Result<(), String>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct StreamEncoderManifest {
    pub public: VideoEncoderManifest,
    pub encode_stream: VideoEncodeStreamFn,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VideoEncodeStreamRequest<'a> {
    pub frame_limit: Option<usize>,
    pub width: usize,
    pub height: usize,
    pub format: framefinery_api::PixelFormat,
    pub lossless: bool,
    pub settings: &'a [String],
}

pub(crate) fn encode_stream_from_source(
    manifest: StreamEncoderManifest,
    source: &mut dyn RawVideoFrameSource,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: VideoEncodeSourceRequest<'_>,
    frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
) -> Result<()> {
    let config = request.config;
    manifest.public.validate_config(config)?;
    let settings = config.setting_specs();
    let request = VideoEncodeStreamRequest {
        frame_limit: config.frame_limit,
        width: config.input.width,
        height: config.input.height,
        format: config.input.format,
        lossless: matches!(config.rate_control, VideoRateControl::Lossless),
        settings: &settings,
    };
    let mut source = |frame: &mut [u8]| source.read_frame(frame);
    let mut input = RawVideoFrameSourceReadAdapter::new(&mut source, config.input);
    if let Some(frame_limit) = config.frame_limit {
        input = input.with_frame_limit(frame_limit);
    }
    let mut total_bitstream_bytes = 0usize;
    let mut frame_metrics = frame_metrics;
    let has_frame_metrics = frame_metrics.is_some();
    let mut callback = |frame: VideoEncodeFrameMetrics<'_>| {
        let frame = complete_frame_metrics(config, &mut total_bitstream_bytes, frame);
        if let Some(callback) = frame_metrics.as_mut() {
            callback(frame);
        }
    };
    let frame_metrics = if has_frame_metrics {
        Some(&mut callback as VideoEncodeFrameMetricsCallback<'_>)
    } else {
        None
    };
    (manifest.encode_stream)(&mut input, output, recon, request, frame_metrics)
        .map_err(MediaError::Message)
}

pub(crate) fn complete_frame_metrics<'a>(
    config: &VideoEncoderConfig,
    total_bitstream_bytes: &mut usize,
    mut frame: VideoEncodeFrameMetrics<'a>,
) -> VideoEncodeFrameMetrics<'a> {
    *total_bitstream_bytes += frame.bitstream_bytes;
    if frame.total_bitstream_bytes == 0 {
        frame.total_bitstream_bytes = *total_bitstream_bytes;
    } else {
        *total_bitstream_bytes = frame.total_bitstream_bytes;
    }
    frame.psnr = if config.reconstruction == ReconstructionMode::MetricsOnly {
        frame_psnr(config.input, frame.source, frame.reconstruction)
    } else {
        None
    };
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picture::{read_input_frame, FrameLimit};
    use framefinery_api::{CodecId, FrameInfo, PixelFormat, VideoEncoderSession};

    const TEST_CODEC: VideoEncoderManifest = VideoEncoderManifest::new(
        "test",
        "test-codec",
        "test stream bridge codec",
        &[],
        test_accepts_format,
        test_accepts_format,
        framefinery_api::VideoEncoderManifestHooks {
            create_session: test_create_session,
            encode_source: test_encode_source,
        },
    );

    const TEST_STREAM_ENCODER: StreamEncoderManifest = StreamEncoderManifest {
        public: TEST_CODEC,
        encode_stream: test_encode_stream,
    };

    fn test_accepts_format(format: PixelFormat) -> bool {
        format == PixelFormat::Rgb24
    }

    fn test_create_session(_: VideoEncoderConfig) -> Result<Box<dyn VideoEncoderSession>> {
        Err(MediaError::Message("source-only test manifest".into()))
    }

    fn test_encode_source(
        source: &mut dyn RawVideoFrameSource,
        output: &mut dyn Write,
        recon: Option<&mut dyn Write>,
        request: VideoEncodeSourceRequest<'_>,
        frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
    ) -> Result<()> {
        encode_stream_from_source(
            TEST_STREAM_ENCODER,
            source,
            output,
            recon,
            request,
            frame_metrics,
        )
    }

    fn encode_test_source<'a>(
        config: &'a VideoEncoderConfig,
        source: &mut dyn RawVideoFrameSource,
        output: &mut dyn Write,
        recon: Option<&mut dyn Write>,
        frame_metrics: Option<VideoEncodeFrameMetricsCallback<'a>>,
    ) -> Result<()> {
        TEST_CODEC.validate_config(config)?;
        (TEST_CODEC.source_encode_hook())(
            source,
            output,
            recon,
            VideoEncodeSourceRequest { config },
            frame_metrics,
        )
    }

    fn test_encode_stream(
        input: &mut dyn Read,
        output: &mut dyn Write,
        mut recon: Option<&mut dyn Write>,
        request: VideoEncodeStreamRequest<'_>,
        mut frame_metrics: Option<VideoEncodeFrameMetricsCallback<'_>>,
    ) -> std::result::Result<(), String> {
        let frame_len = request
            .format
            .frame_len(request.width, request.height)
            .ok_or_else(|| "frame length overflow".to_string())?;
        let frame_limit = FrameLimit::from_frame_limit(request.frame_limit);
        let mut frame = vec![0; frame_len];
        let mut frame_idx = 0usize;
        while frame_limit.should_read(frame_idx) {
            if !read_input_frame(input, &mut frame, frame_idx, frame_limit, "test input")? {
                break;
            }
            output.write_all(&frame).map_err(|err| err.to_string())?;
            if let Some(writer) = recon.as_mut() {
                writer.write_all(&frame).map_err(|err| err.to_string())?;
            }
            if let Some(callback) = frame_metrics.as_mut() {
                callback(VideoEncodeFrameMetrics {
                    frame_idx,
                    frame_count: frame_limit.metric_count(),
                    bitstream_bytes: frame_len,
                    total_bitstream_bytes: 0,
                    encode_elapsed: Duration::ZERO,
                    psnr: None,
                    source: &frame,
                    reconstruction: &frame,
                });
            }
            frame_idx += 1;
        }
        Ok(())
    }

    fn config(info: FrameInfo) -> VideoEncoderConfig {
        VideoEncoderConfig::new(CodecId::new("test").unwrap(), info).with_frame_limit(2)
    }

    #[test]
    fn source_bridge_pulls_one_frame_at_a_time() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let frames = [
            vec![1u8; info.expected_len()],
            vec![2u8; info.expected_len()],
        ];
        let mut index = 0usize;
        let mut source = |frame: &mut [u8]| {
            let Some(next) = frames.get(index) else {
                return Ok(false);
            };
            frame.copy_from_slice(next);
            index += 1;
            Ok(true)
        };
        let mut output = Vec::new();

        encode_test_source(&config(info), &mut source, &mut output, None, None)
            .expect("source bridge encode");

        assert_eq!(index, 2);
        assert_eq!(
            output,
            [frames[0].as_slice(), frames[1].as_slice()].concat()
        );
    }

    #[test]
    fn source_encode_reads_until_eof_without_frame_limit() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("test").unwrap(), info);
        let frames = [
            vec![1u8; info.expected_len()],
            vec![2u8; info.expected_len()],
        ];
        let mut index = 0usize;
        let mut source = |frame: &mut [u8]| {
            let Some(next) = frames.get(index) else {
                return Ok(false);
            };
            frame.copy_from_slice(next);
            index += 1;
            Ok(true)
        };
        let mut output = Vec::new();

        encode_test_source(&config, &mut source, &mut output, None, None)
            .expect("source bridge encode");

        assert_eq!(index, 2);
        assert_eq!(
            output,
            [frames[0].as_slice(), frames[1].as_slice()].concat()
        );
    }
}
