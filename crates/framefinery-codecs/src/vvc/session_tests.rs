use super::*;
use crate::vvc::VvcStreamEncoder;
use framefinery_api::{FrameInfo, PixelFormat, VideoEncodeFrameMetrics, VideoEncoderSetting};
use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;
use std::sync::Arc;

#[path = "../../tests/support/vvc_streaming.rs"]
mod fixtures;
use fixtures::frame;
fn config(format: PixelFormat, lossless: bool) -> VideoEncoderConfig {
    fixtures::config(format, lossless, 3)
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

#[derive(Debug, Default, PartialEq, Eq)]
struct SourceBoundaryWriter {
    write_attempts: usize,
    offered_bytes: usize,
    flushes: usize,
}

impl Write for SourceBoundaryWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_attempts += 1;
        self.offered_bytes += bytes.len();
        // Accept no bytes, but detect even a swallowed header-write error.
        Err(io::Error::other("unexpected source-boundary output"))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Err(io::Error::other("unexpected source-boundary flush"))
    }
}

#[test]
fn empty_source_emits_no_frame_output() {
    for lossless in [true, false] {
        let config = config(PixelFormat::Yuv420p8, lossless);
        assert_eq!(config.frame_limit, None);
        let mut pulls = 0;
        let mut source = |_: &mut [u8]| {
            pulls += 1;
            Ok(false)
        };
        let mut output = SourceBoundaryWriter::default();
        let mut recon = SourceBoundaryWriter::default();
        let mut callbacks = 0;
        let mut callback = |_: VideoEncodeFrameMetrics<'_>| callbacks += 1;

        let result = crate::encode_source(
            &config,
            &mut source,
            &mut output,
            Some(&mut recon),
            Some(&mut callback),
        );

        assert_eq!(result, Ok(()), "lossless={lossless}");
        assert_eq!(pulls, 1);
        assert_eq!(callbacks, 0);
        assert_eq!(output, SourceBoundaryWriter::default());
        assert_eq!(recon, SourceBoundaryWriter::default());
    }
}

#[test]
fn first_source_error_precedes_output() {
    for lossless in [true, false] {
        let config = config(PixelFormat::Yuv420p8, lossless);
        assert_eq!(config.frame_limit, None);
        let mut pulls = 0;
        let mut source = |_: &mut [u8]| {
            pulls += 1;
            Err(MediaError::Message("first-pull sentinel".into()))
        };
        let mut output = SourceBoundaryWriter::default();
        let mut recon = SourceBoundaryWriter::default();
        let mut callbacks = 0;
        let mut callback = |_: VideoEncodeFrameMetrics<'_>| callbacks += 1;

        let result = crate::encode_source(
            &config,
            &mut source,
            &mut output,
            Some(&mut recon),
            Some(&mut callback),
        );

        assert_eq!(
            result,
            Err(MediaError::Message(
                "failed to read VVC input frame 1: first-pull sentinel".into()
            )),
            "lossless={lossless}"
        );
        assert_eq!(pulls, 1);
        assert_eq!(callbacks, 0);
        assert_eq!(output, SourceBoundaryWriter::default());
        assert_eq!(recon, SourceBoundaryWriter::default());
    }
}

#[test]
fn complete_frame_output_precedes_flush_and_matches_source_prefixes() {
    let formats = fixtures::formats();
    for format in formats {
        for lossless in [true, false] {
            let config = config(format, lossless);
            let frames: Vec<_> = [0, 90, 90, 91, 91, 92]
                .into_iter()
                .map(|phase| frame(config.input, phase))
                .collect();
            let mut session = VvcEncoderSession::new(config.clone()).unwrap();
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
                assert_eq!(chunk.keyframe, index == 0);
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
    let mut session = VvcEncoderSession::new(config).unwrap();
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
    let mut empty = VvcEncoderSession::new(super::tests::config(info.format, true)).unwrap();
    assert_eq!(empty.flush().unwrap(), VideoEncodeOutput::default());
    assert_eq!(empty.flush().unwrap(), VideoEncodeOutput::default());
    assert_eq!(
        empty.encode_frame(frame(info, 0)),
        Err(MediaError::EncodeAfterFlush)
    );
}

#[test]
fn session_validates_settings_and_profile_before_creation() {
    let invalid = config(PixelFormat::Yuv420p8, true)
        .with_setting(VideoEncoderSetting::boolean("missing", true).unwrap());
    assert!(matches!(VvcEncoderSession::new(invalid),
        Err(MediaError::UnknownSetting { codec, setting }) if codec == "vvc" && setting == "missing"));
    let invalid = config(PixelFormat::yuv444(12).unwrap(), true)
        .with_setting(VideoEncoderSetting::text("profile", "main-10").unwrap());
    assert!(VvcEncoderSession::new(invalid).is_err());
}

#[test]
fn rejected_raw_frame_does_not_consume_headers_or_advance_state() {
    let config = config(PixelFormat::Yuv420p8, true);
    let input = frame(config.input, 0);
    let mut session = VvcEncoderSession::new(config).unwrap();
    let mut bytes = Vec::new();
    #[cfg(feature = "vvc-stats")]
    let stats = session.encoder.frame_stats();
    assert!(session
        .encoder
        .encode_frame(
            &input.data()[1..],
            &mut VvcFrameSinks {
                output: &mut bytes,
                recon: None,
                frame_metrics: None,
            },
            #[cfg(feature = "vvc-stats")]
            stats
        )
        .is_err());
    assert!(bytes.is_empty());
    assert_eq!(session.encoder.frame_index, 0);
    assert!(session.encoder.pending_parameter_sets.is_some());
    assert!(!session.encode_frame(input).unwrap().chunks[0]
        .data
        .is_empty());
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
    let mut session = VvcEncoderSession::new(config).unwrap();
    session.encode_frame(input.clone()).unwrap();
    let cache = Arc::downgrade(session.encoder.previous_predictive_cache.as_ref().unwrap());
    let state = Rc::new(RefCell::new(Observation::default()));
    let mut writer = ProbeWriter {
        state: state.clone(),
        fail_at: Some(5),
    };
    #[cfg(feature = "vvc-stats")]
    let stats = session.encoder.frame_stats();
    let error = session
        .encoder
        .encode_frame(
            input.data(),
            &mut VvcFrameSinks {
                output: &mut writer,
                recon: None,
                frame_metrics: None,
            },
            #[cfg(feature = "vvc-stats")]
            stats,
        )
        .err()
        .expect("injected partial-write failure");
    assert_eq!(state.borrow().bytes.len(), 5);
    assert!(error.contains("injected output failure"));
    assert!(
        cache.upgrade().is_none(),
        "failed stream releases its reference"
    );
    assert_eq!(
        session.encode_frame(input),
        Err(MediaError::Message(error.clone()))
    );
    assert_eq!(session.flush(), Err(MediaError::Message(error.clone())));
    assert_eq!(session.flush(), Err(MediaError::Message(error)));
    assert!(session.encoder.previous_predictive_cache.is_none());
    assert!(session.encoder.pending_parameter_sets.is_none());
}

#[test]
fn reconstruction_write_failure_is_terminal_and_suppresses_metrics() {
    let config = config(PixelFormat::Yuv420p8, true);
    let input = frame(config.input, 0);
    let mut session = VvcEncoderSession::new(config).unwrap();
    session.encode_frame(input.clone()).unwrap();
    let reference = Arc::downgrade(session.encoder.previous_predictive_cache.as_ref().unwrap());
    let state = Rc::new(RefCell::new(Observation::default()));
    let mut writer = ProbeWriter {
        state: state.clone(),
        fail_at: Some(5),
    };
    let mut bytes = Vec::new();
    let mut callbacks = 0;
    let mut callback = |_: VvcEncodeFrameMetrics<'_>| callbacks += 1;
    #[cfg(feature = "vvc-stats")]
    let stats = session.encoder.frame_stats();
    let error = session
        .encoder
        .encode_frame(
            input.data(),
            &mut VvcFrameSinks {
                output: &mut bytes,
                recon: Some(&mut writer),
                frame_metrics: Some(&mut callback),
            },
            #[cfg(feature = "vvc-stats")]
            stats,
        )
        .err()
        .unwrap();
    assert!(
        !bytes.is_empty(),
        "encoded output may already have been delivered"
    );
    assert_eq!(callbacks, 0);
    assert_eq!(state.borrow().bytes.len(), 5);
    assert!(reference.upgrade().is_none());
    assert_eq!(session.flush(), Err(MediaError::Message(error.clone())));
    assert_eq!(session.encode_frame(input), Err(MediaError::Message(error)));
}

#[test]
fn source_and_consumer_errors_stop_further_pulls() {
    let config = config(PixelFormat::Yuv420p8, true);
    let input = frame(config.input, 0);
    let expected = source_encode(&config, std::slice::from_ref(&input)).0;
    let mut calls = 0;
    let mut source = |dst: &mut [u8]| {
        calls += 1;
        if calls == 2 {
            return Err(MediaError::Message("injected source failure".into()));
        }
        dst.copy_from_slice(input.data());
        Ok(true)
    };
    let mut bytes = Vec::new();
    let error = crate::encode_source(&config, &mut source, &mut bytes, None, None).unwrap_err();
    assert!(error.to_string().contains("injected source failure"));
    assert_eq!(calls, 2);
    assert_eq!(bytes, expected);

    // Exercise the documented synchronous consume-before-next-frame driver.
    let mut pulls = 0;
    let result = (|| -> Result<()> {
        let mut session = VvcEncoderSession::new(config.clone())?;
        let frames = [0, 90, 91].into_iter().map(|phase| {
            pulls += 1;
            frame(config.input, phase)
        });
        let consume = |step: VideoEncodeOutput| -> Result<()> {
            assert_eq!(step.chunks.len(), 1);
            Err(MediaError::Message("injected consumer failure".into()))
        };
        for frame in frames {
            consume(session.encode_frame(frame)?)?;
        }
        consume(session.flush()?)
    })();
    assert_eq!(
        result,
        Err(MediaError::Message("injected consumer failure".into()))
    );
    assert_eq!(pulls, 1);
}

#[test]
fn retained_state_has_fixed_frame_slots_and_no_packet_history() {
    for lossless in [true, false] {
        let config = config(PixelFormat::Yuv420p8, lossless);
        let info = config.input;
        let mut session = VvcEncoderSession::new(config).unwrap();
        for index in 0..128 {
            let old_cache = session
                .encoder
                .previous_predictive_cache
                .as_ref()
                .map(Arc::downgrade);
            let output = session
                .encode_frame(frame(info, (index % 3) as u16))
                .unwrap();
            assert_eq!(output.chunks.len(), 1);
            assert_eq!(output.reconstructions.len(), 1);
            drop(output);
            // Exhaustive ownership review: any new state field must be classified.
            let VvcEncoderSession { config: _, encoder } = &session;
            let VvcStreamEncoder {
                config,
                pending_parameter_sets,
                previous_predictive_cache,
                ctu_inter_skip_slice_payload_cache,
                frame_index,
                total_bitstream_bytes: _,
                status: _,
                #[cfg(feature = "vvc-stats")]
                    vvc_stats: _,
                #[cfg(feature = "vvc-stats")]
                    vvc_ctu_bits: _,
            } = encoder;
            let crate::vvc::VvcStreamConfig {
                geometry: _,
                frame_limit: _,
                format: _,
                stream_format: _,
                stream_frame_layout: _,
                frame_len,
                ctu_cols: _,
                ctu_count,
                residual_mode: _,
                residual_policy: _,
                predictive_enabled: _,
                ctu_sliced_partitioning_supported: _,
                lossy_ctu_skip_enabled: _,
                slice_config: _,
                scc_444_enabled: _,
                predictive_single_slice_config: _,
                luma_qp: _,
                chroma_qp: _,
                transform_skip_quant_tables: _,
                explicit_inter_eligible_luma_leaf_count: _,
                options: _,
            } = config;
            assert_eq!(*frame_index, index + 1);
            assert!(pending_parameter_sets.is_none());
            let cache = previous_predictive_cache.as_ref().unwrap();
            assert_eq!(
                Arc::strong_count(cache),
                1,
                "no prior frame-cache owner remains"
            );
            let crate::vvc::VvcPredictiveFrameCache {
                source,
                reconstruction,
                ctu_decisions,
            } = &**cache;
            assert!(source.capacity() <= *frame_len);
            assert_eq!(ctu_decisions.len(), *ctu_count);
            assert!(ctu_decisions.iter().all(|ctu| Arc::strong_count(ctu) == 1));
            let crate::vvc::VvcReconstructionFrame {
                geometry: _,
                format: _,
                luma,
                cb,
                cr,
                luma_available,
                cb_available,
                cr_available,
            } = reconstruction;
            let samples = info.expected_len() / info.format.bytes_per_sample();
            assert_eq!(luma.capacity() + cb.capacity() + cr.capacity(), samples);
            assert_eq!(
                luma_available.len() + cb_available.len() + cr_available.len(),
                samples
            );
            assert!(ctu_inter_skip_slice_payload_cache.retained_payload_count() <= *ctu_count);
            if let Some(old) = old_cache {
                assert!(
                    old.upgrade().is_none(),
                    "changed frame must not retain prior cache"
                );
            }
        }
        let last = Arc::downgrade(session.encoder.previous_predictive_cache.as_ref().unwrap());
        session.flush().unwrap();
        assert!(last.upgrade().is_none());
        assert!(session.encoder.previous_predictive_cache.is_none());
        assert_eq!(
            session
                .encoder
                .ctu_inter_skip_slice_payload_cache
                .retained_payload_count(),
            0
        );
    }
}

#[test]
fn reconstruction_options_and_intra_keep_source_session_parity() {
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
            let mut session = VvcEncoderSession::new(config).unwrap();
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
