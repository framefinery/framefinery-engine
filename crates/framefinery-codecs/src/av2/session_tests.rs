use super::*;
use crate::av2::Av2StreamEncoder;
use framefinery_api::{FrameInfo, PixelFormat, VideoEncoderSetting};
use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

fn config(format: PixelFormat, lossless: bool) -> VideoEncoderConfig {
    VideoEncoderConfig::new(
        CodecId::new("av2").unwrap(),
        FrameInfo::new(32, 24, format).unwrap(),
    )
    .with_rate_control(if lossless {
        VideoRateControl::Lossless
    } else {
        VideoRateControl::ConstantQuantizer(24)
    })
    .with_reconstruction(ReconstructionMode::Frames)
    .with_setting(VideoEncoderSetting::integer("gop", 3).unwrap())
}

fn frame(info: FrameInfo, phase: usize) -> Frame {
    let mut data = vec![0; info.expected_len()];
    let bits = info.format.bit_depth();
    for index in 0..data.len() / info.format.bytes_per_sample() {
        let sample = (((index / 8 + phase * 13) % 191 + 16) as u16) << (bits.bits() - 8);
        framefinery_api::write_planar_sample(&mut data, index, sample, bits).unwrap();
    }
    Frame::new(info, data).unwrap()
}

fn source_encode(config: &VideoEncoderConfig, frames: &[Frame]) -> (Vec<u8>, Vec<u8>) {
    let mut remaining = frames.iter();
    let mut source = |dst: &mut [u8]| {
        let Some(frame) = remaining.next() else {
            return Ok(false);
        };
        dst.copy_from_slice(frame.data());
        Ok(true)
    };
    let mut output = Vec::new();
    let mut recon = Vec::new();
    crate::encode_source(config, &mut source, &mut output, Some(&mut recon), None).unwrap();
    (output, recon)
}

#[test]
fn complete_frame_output_precedes_flush_and_matches_source_prefixes() {
    let formats = [
        PixelFormat::Yuv420p8,
        PixelFormat::Yuv422p8,
        PixelFormat::Yuv444p8,
        PixelFormat::Gbrp8,
        PixelFormat::Rgb24,
        PixelFormat::yuv420(10).unwrap(),
        PixelFormat::yuv422(10).unwrap(),
        PixelFormat::yuv444(10).unwrap(),
    ];
    for format in formats {
        for lossless in [true, false] {
            let config = config(format, lossless);
            let frames: Vec<_> = [0, 0, 1, 2, 2, 3]
                .into_iter()
                .map(|phase| frame(config.input, phase))
                .collect();
            let mut session = Av2EncoderSession::new(config.clone()).unwrap();
            // Deliberate caller-owned recording only for ordered-prefix comparison.
            let mut prefix = Vec::new();
            let mut recon_prefix = Vec::new();
            for (index, frame) in frames.iter().enumerate() {
                let output = session.encode_frame(frame.clone()).unwrap();
                assert_eq!(output.chunks.len(), 1);
                let chunk = &output.chunks[0];
                assert_eq!(chunk.kind, VideoChunkKind::Frame);
                assert_eq!(chunk.frame_index, Some(index));
                assert!(!chunk.data.is_empty());
                assert_eq!(chunk.keyframe, !matches!(index, 1 | 4));
                assert_eq!(output.reconstructions.len(), 1);
                assert_eq!(output.metrics.len(), 1);
                assert_eq!(output.metrics[0].frame_index, index);
                assert_eq!(output.metrics[0].encoded_bytes, chunk.data.len());
                prefix.extend_from_slice(&chunk.data);
                recon_prefix.extend_from_slice(output.reconstructions[0].data());
                let (source_bytes, source_recon) = source_encode(&config, &frames[..=index]);
                assert_eq!(
                    prefix, source_bytes,
                    "{format} lossless={lossless} prefix={index}"
                );
                assert_eq!(recon_prefix, source_recon);
                if lossless {
                    assert_eq!(output.reconstructions[0], *frame);
                }
            }
            assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());
        }
    }
}

#[test]
fn input_rejection_and_terminal_flush_preserve_lifecycle() {
    let config = config(PixelFormat::Yuv420p8, true).with_frame_limit(2);
    let info = config.input;
    let mut session = Av2EncoderSession::new(config).unwrap();
    let wrong = FrameInfo::new(16, 16, info.format).unwrap();
    assert!(matches!(
        session.encode_frame(frame(wrong, 0)),
        Err(MediaError::IncompatibleFormat { .. })
    ));
    assert_eq!(session.encoder.frame_index, 0);
    session.encode_frame(frame(info, 0)).unwrap();
    session.encode_frame(frame(info, 0)).unwrap();
    assert_eq!(
        session.encode_frame(frame(info, 1)),
        Err(MediaError::FrameLimitExceeded { limit: 2 })
    );
    assert_eq!(session.encoder.frame_index, 2);
    assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());
    assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());
    assert_eq!(
        session.encode_frame(frame(info, 1)),
        Err(MediaError::EncodeAfterFlush)
    );
    let mut empty = Av2EncoderSession::new(super::tests::config(info.format, true)).unwrap();
    assert_eq!(empty.flush().unwrap(), VideoEncodeOutput::default());
    assert_eq!(empty.flush().unwrap(), VideoEncodeOutput::default());
    assert_eq!(
        empty.encode_frame(frame(info, 0)),
        Err(MediaError::EncodeAfterFlush)
    );
}

#[derive(Default)]
struct Observation {
    bytes: Vec<u8>,
    pulls: usize,
    flushes: usize,
}
struct ProbeWriter {
    state: Rc<RefCell<Observation>>,
    fail_at: Option<usize>,
}
impl Write for ProbeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.state.borrow_mut();
        let room = self
            .fail_at
            .map_or(usize::MAX, |limit| limit.saturating_sub(state.bytes.len()));
        if room == 0 {
            return Err(io::Error::other("injected output failure"));
        }
        let count = bytes.len().min(7).min(room); // Exercise write_all with short writes.
        state.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.state.borrow_mut().flushes += 1;
        Ok(())
    }
}

#[test]
fn source_writes_each_complete_frame_before_pulling_the_next() {
    let config = config(PixelFormat::Yuv420p8, true);
    let frames: Vec<_> = [0, 0, 1]
        .into_iter()
        .map(|phase| frame(config.input, phase))
        .collect();
    let expected: Vec<_> = (1..=frames.len())
        .map(|count| source_encode(&config, &frames[..count]).0)
        .collect();
    for fail in [false, true] {
        let state = Rc::new(RefCell::new(Observation::default()));
        let mut writer = ProbeWriter {
            state: state.clone(),
            fail_at: fail.then_some(expected[0].len() + 3),
        };
        let mut index = 0;
        let mut source = |dst: &mut [u8]| {
            if index > 0 {
                assert_eq!(
                    state.borrow().bytes,
                    expected[index - 1],
                    "all frame bytes must be written before another pull"
                );
            }
            let Some(frame) = frames.get(index) else {
                return Ok(false);
            };
            dst.copy_from_slice(frame.data());
            index += 1;
            state.borrow_mut().pulls += 1;
            Ok(true)
        };
        let result = crate::encode_source(&config, &mut source, &mut writer, None, None);
        if fail {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("injected output failure"));
            assert_eq!(
                state.borrow().pulls,
                2,
                "write failure stops further input consumption"
            );
        } else {
            result.unwrap();
            assert_eq!(state.borrow().bytes, *expected.last().unwrap());
            assert_eq!(state.borrow().pulls, frames.len());
        }
        assert_eq!(state.borrow().flushes, 0, "caller owns writer flushing");
        writer.flush().unwrap();
        assert_eq!(state.borrow().flushes, 1);
    }
}

#[test]
fn partial_write_failure_is_terminal_in_shared_codec_state() {
    let config = config(PixelFormat::Yuv420p8, true);
    let input = frame(config.input, 0);
    let mut session = Av2EncoderSession::new(config).unwrap();
    let state = Rc::new(RefCell::new(Observation::default()));
    let mut writer = ProbeWriter {
        state: state.clone(),
        fail_at: Some(5),
    };
    let stats = session.encoder.frame_stats();
    let error = session
        .encoder
        .encode_frame(
            input.data(),
            &mut Av2FrameSinks {
                output: &mut writer,
                recon: None,
                frame_metrics: None,
            },
            stats,
        )
        .err()
        .expect("injected partial-write failure");
    assert_eq!(state.borrow().bytes.len(), 5);
    assert!(error.contains("injected output failure"));
    assert_eq!(
        session.encode_frame(input),
        Err(MediaError::Message(error.clone()))
    );
    assert_eq!(session.flush(), Err(MediaError::Message(error.clone())));
    assert_eq!(session.flush(), Err(MediaError::Message(error)));
    assert!(session.encoder.predictive_reference.is_none());
    assert!(session.encoder.predictive_reconstruction.is_none());
}

#[test]
fn retained_state_has_fixed_frame_slots_and_no_packet_history() {
    for lossless in [true, false] {
        let config = config(PixelFormat::Yuv420p8, lossless);
        let info = config.input;
        let mut session = Av2EncoderSession::new(config).unwrap();
        for index in 0..128 {
            let output = session.encode_frame(frame(info, index % 3)).unwrap();
            assert_eq!(output.chunks.len(), 1);
            assert_eq!(output.reconstructions.len(), 1);
            assert_eq!(output.metrics.len(), 1);
            drop(output);
            // Exhaustive ownership review: adding any retained field requires
            // updating this test rather than hiding a new history queue.
            let Av2EncoderSession { config: _, encoder } = &session;
            let Av2StreamEncoder {
                request: _,
                options: _,
                visible_geometry: _,
                coded_geometry: _,
                stream_format: _,
                source_expected_len: _,
                coded_expected_len,
                predictive_headers_written: _,
                predictive_reference,
                predictive_reconstruction,
                frame_index,
                total_bitstream_bytes: _,
                av2_stats: _,
                status: _,
            } = encoder;
            assert_eq!(*frame_index, index + 1);
            let retained = predictive_reference.as_ref().map_or(0, Vec::capacity)
                + predictive_reconstruction.as_ref().map_or(0, Vec::capacity);
            assert!(retained <= 2 * coded_expected_len);
        }
        session.flush().unwrap();
        assert!(session.encoder.predictive_reference.is_none());
        assert!(session.encoder.predictive_reconstruction.is_none());
    }
}

#[test]
fn reconstruction_options_and_legacy_intra_keep_source_session_parity() {
    for legacy_intra in [false, true] {
        for reconstruction in [
            ReconstructionMode::None,
            ReconstructionMode::MetricsOnly,
            ReconstructionMode::Frames,
        ] {
            let mut config = config(PixelFormat::Yuv444p8, false)
                .with_frame_limit(2)
                .with_reconstruction(reconstruction);
            if legacy_intra {
                config.rate_control = VideoRateControl::CodecDefault;
                config.settings = vec![VideoEncoderSetting::integer("gop", 0).unwrap()];
            }
            let frames = vec![Frame::blank(config.input); 2];
            let expected = source_encode(&config, &frames);
            let mut session = Av2EncoderSession::new(config).unwrap();
            let mut prefix = Vec::new();
            for (index, frame) in frames.into_iter().enumerate() {
                let step = session.encode_frame(frame).unwrap();
                prefix.extend_from_slice(&step.chunks[0].data);
                assert_eq!(
                    step.reconstructions.len(),
                    usize::from(reconstruction == ReconstructionMode::Frames)
                );
                if reconstruction == ReconstructionMode::None {
                    assert!(step.metrics.is_empty());
                } else {
                    assert_eq!(step.metrics.len(), 1);
                    assert_eq!(step.metrics[0].frame_index, index);
                    assert_eq!(step.metrics[0].frame_count, Some(2));
                    assert_eq!(step.metrics[0].encoded_bytes, step.chunks[0].data.len());
                    assert_eq!(
                        step.metrics[0].psnr.is_some(),
                        reconstruction == ReconstructionMode::MetricsOnly
                    );
                }
            }
            assert_eq!(prefix, expected.0);
            assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());
        }
    }
}
