#![cfg(feature = "codec-av2")]

use std::io::{Cursor, Read};

use framefinery::{
    cli_options_for_scope, create_encoder, encode_frame, encode_source, encoder,
    find_encoder_manifest, CliOptionScope, CodecId, Frame, FrameInfo, MediaError, PixelFormat,
    RawVideoFrameReadSource, RawVideoFrameSource, ReconstructionMode, Result,
    VideoEncodeFrameMetrics, VideoEncoderConfig, VideoEncoderSetting, VideoRateControl,
};

#[test]
fn facade_drives_source_and_buffered_encoders() -> Result<()> {
    let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
    let config = VideoEncoderConfig::new(CodecId::new("av2")?, info)
        .with_rate_control(VideoRateControl::Lossless)
        .with_reconstruction(ReconstructionMode::Frames);

    let pixels = vec![0; info.expected_len()];
    let mut source = RawVideoFrameReadSource::new(Cursor::new(pixels));
    let mut bitstream = Vec::new();
    let mut metric_rows = Vec::new();
    let mut on_metrics = |metrics: VideoEncodeFrameMetrics<'_>| {
        metric_rows.push((
            metrics.frame_idx,
            metrics.frame_count,
            metrics.bitstream_bytes,
            metrics.source.len(),
            metrics.reconstruction.len(),
        ));
    };

    let codec = find_encoder_manifest("av2").expect("av2 feature should expose the encoder");
    assert_eq!(codec.name, "av2");
    encode_source(
        &config,
        &mut source,
        &mut bitstream,
        None,
        Some(&mut on_metrics),
    )?;

    assert!(!bitstream.is_empty());
    assert_eq!(metric_rows.len(), 1);
    assert_eq!(metric_rows[0].1, None);
    assert_eq!(metric_rows[0].3, info.expected_len());
    assert_eq!(metric_rows[0].4, info.expected_len());

    let output = encode_frame(config.clone(), Frame::blank(info))?;
    assert_eq!(output.chunks.len(), 1);
    assert!(!output.chunks[0].data.is_empty());
    assert_eq!(output.reconstructions, vec![Frame::blank(info)]);

    let mut encoder = create_encoder(config)?;
    let step = encoder.encode_frame(Frame::blank(info))?;
    assert!(step.chunks.is_empty());

    let output = encoder.flush()?;
    assert_eq!(output.chunks.len(), 1);
    assert!(!output.chunks[0].data.is_empty());
    assert_eq!(output.reconstructions, vec![Frame::blank(info)]);
    Ok(())
}

#[test]
fn facade_exposes_fluent_encoder_builder() -> Result<()> {
    let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
    let config = encoder("av2")?
        .input(info)
        .fps(30, 1)?
        .lossless()
        .reconstruction_frames()
        .setting("predictive", true)?
        .into_config()?;

    assert_eq!(config.codec.as_str(), "av2");
    assert_eq!(config.input, info);
    assert_eq!(config.frame_rate.expect("fps should be set").numerator, 30);
    assert!(matches!(config.rate_control, VideoRateControl::Lossless));
    assert_eq!(config.reconstruction, ReconstructionMode::Frames);
    assert_eq!(config.settings[0].as_cli_spec(), "predictive=true");

    let output = encoder("av2")?
        .input(info)
        .lossless()
        .reconstruction_frames()
        .encode_frame(Frame::blank(info))?;
    assert_eq!(output.reconstructions, vec![Frame::blank(info)]);
    assert!(!output.chunks[0].data.is_empty());

    let missing_input = match encoder("av2")?.build() {
        Ok(_) => panic!("builder should require input metadata"),
        Err(err) => err,
    };
    match missing_input {
        MediaError::MissingRequiredField { field } => assert_eq!(field, "input"),
        other => panic!("expected MissingRequiredField, got {other}"),
    }

    let unavailable = encoder("not-compiled").expect_err("unknown codec should fail early");
    match unavailable {
        MediaError::UnsupportedCodec { codec, .. } => assert_eq!(codec, "not-compiled"),
        other => panic!("expected UnsupportedCodec, got {other}"),
    }

    Ok(())
}

#[test]
fn facade_exposes_cli_option_inventory() {
    let output_options = cli_options_for_scope(CliOptionScope::Output).collect::<Vec<_>>();
    assert!(output_options
        .iter()
        .any(|option| option.matches_name("--encode")));
    assert!(output_options
        .iter()
        .any(|option| option.matches_name("--reconstruction")));
    assert!(output_options
        .iter()
        .any(|option| option.matches_name("--psnr")));
}

#[cfg(feature = "filter-pattern")]
#[test]
fn facade_exposes_raw_source_filter_pipeline() -> Result<()> {
    use framefinery::{FilterPipelineSpec, RawVideoFrameSourceReadAdapter};

    let info = FrameInfo::new(8, 8, PixelFormat::Rgb24)?;
    let pipeline = FilterPipelineSpec::from_source_filter()
        .filter("pattern=checker")?
        .build()?;
    let mut source = pipeline
        .build_raw_video_source(info, 2)?
        .expect("pattern source");
    let mut reader =
        RawVideoFrameSourceReadAdapter::new(move |frame: &mut [u8]| source.read_frame(frame), info);
    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .map_err(|err| MediaError::Message(err.to_string()))?;

    assert_eq!(data.len(), info.expected_len() * 2);
    assert_ne!(&data[..info.expected_len()], &data[info.expected_len()..]);
    Ok(())
}

#[test]
fn facade_exposes_structured_api_errors() -> Result<()> {
    let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
    let invalid = VideoEncoderConfig::new(CodecId::new("av2")?, info)
        .with_setting(VideoEncoderSetting::boolean("no-such-setting", true)?);

    let err = match create_encoder(invalid) {
        Ok(_) => panic!("unknown setting should not create an encoder"),
        Err(err) => err,
    };
    match err {
        MediaError::UnknownSetting { codec, setting } => {
            assert_eq!(codec, "av2");
            assert_eq!(setting, "no-such-setting");
        }
        other => panic!("expected UnknownSetting, got {other}"),
    }

    let mut source = RawVideoFrameReadSource::new(Cursor::new(vec![0; info.expected_len() - 1]));
    let mut frame = vec![0; info.expected_len()];
    let err = source
        .read_frame(&mut frame)
        .expect_err("partial raw frame should fail");
    match err {
        MediaError::ShortFrameRead { expected, actual } => {
            assert_eq!(expected, info.expected_len());
            assert_eq!(actual, info.expected_len() - 1);
        }
        other => panic!("expected ShortFrameRead, got {other}"),
    }

    let config = VideoEncoderConfig::new(CodecId::new("av2")?, info);
    let mut encoder = create_encoder(config)?;
    encoder.flush()?;
    let err = encoder
        .encode_frame(Frame::blank(info))
        .expect_err("encoding after flush should fail");
    match err {
        MediaError::EncodeAfterFlush => {}
        other => panic!("expected EncodeAfterFlush, got {other}"),
    }

    Ok(())
}
