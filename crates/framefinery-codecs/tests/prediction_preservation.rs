//! A focused capability regression, not a performance/market acceptance gate.
#![cfg(any(feature = "av2", feature = "vvc"))]

#[path = "support/prediction_texture.rs"]
mod fixture;

use framefinery_api::{
    CodecId, Frame, FrameInfo, PixelFormat, ReconstructionMode, VideoChunkKind, VideoEncodeOutput,
    VideoEncoderConfig, VideoEncoderSetting, VideoRateControl,
};
use std::{env, fs, path::Path, process::Command, time::SystemTime};

struct Recording {
    chunks: Vec<Vec<u8>>,
    reconstruction: Vec<u8>,
}

fn check_benefit(
    predictive: &Recording,
    intra: &Recording,
    source: &[u8],
) -> Result<(), &'static str> {
    for recording in [predictive, intra] {
        if recording.chunks.len() != fixture::PHASES.len()
            || recording.chunks.iter().any(Vec::is_empty)
        {
            return Err("missing complete frame output");
        }
        if source.is_empty() || recording.reconstruction != source {
            return Err("lossless source equality required for both settings");
        }
    }
    // Only changed pictures outside the GOP reset establish this benefit.
    // Repeated pictures alone cannot satisfy it. The substantial saving on
    // this deliberately textured fixture excludes mere header-size differences;
    // it is not a universal encoder acceptance threshold.
    for index in [1, 5] {
        if predictive.chunks[index].len() >= intra.chunks[index].len() / 2 {
            return Err("changed-frame texture reuse must save payload, not just headers");
        }
    }
    if predictive.chunks.iter().map(Vec::len).sum::<usize>()
        >= intra.chunks.iter().map(Vec::len).sum::<usize>()
    {
        return Err("prediction must reduce the complete fixture stream");
    }
    Ok(())
}

fn encode_and_decode(
    config: VideoEncoderConfig,
    frames: &[Frame],
    decoder: &Path,
    directory: &Path,
) -> Recording {
    fs::create_dir(directory).unwrap();
    let mut session = framefinery_codecs::create_encoder(config.clone()).unwrap();
    // Explicit caller-owned recording for this test only.
    let mut recording = Recording {
        chunks: Vec::new(),
        reconstruction: Vec::new(),
    };
    for (index, frame) in frames.iter().enumerate() {
        let step = session.encode_frame(frame.clone()).unwrap();
        assert_eq!(step.chunks.len(), 1, "complete frame required before flush");
        assert_eq!(step.chunks[0].kind, VideoChunkKind::Frame);
        assert_eq!(step.chunks[0].frame_index, Some(index));
        assert!(!step.chunks[0].data.is_empty());
        assert_eq!(step.reconstructions.len(), 1);
        assert_eq!(step.reconstructions[0], *frame);
        recording
            .reconstruction
            .extend_from_slice(step.reconstructions[0].data());
        recording
            .chunks
            .push(step.chunks.into_iter().next().unwrap().data);
    }
    let bytes = recording.chunks.concat();
    let bitstream = directory.join("stream.bin");
    let reconstruction = directory.join("reference.yuv");
    fs::write(&bitstream, &bytes).unwrap();
    fs::write(directory.join("internal.yuv"), &recording.reconstruction).unwrap();
    // Actual reference decoding of six ordered frames BEFORE session finalization.
    let mut command = Command::new(decoder);
    if config.codec.as_str() == "av2" {
        command
            .arg("--rawvideo")
            .arg("-o")
            .arg(&reconstruction)
            .arg(&bitstream);
    } else {
        command
            .arg("-b")
            .arg(&bitstream)
            .arg("-o")
            .arg(&reconstruction);
    }
    let result = command
        .output()
        .expect("required reference decoder must execute");
    fs::write(directory.join("decoder.stdout"), &result.stdout).unwrap();
    fs::write(directory.join("decoder.stderr"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "reference failed: {}",
        directory.display()
    );
    let reference = fs::read(reconstruction).unwrap();
    assert_eq!(reference.len(), config.input.expected_len() * frames.len());
    assert_eq!(
        reference, recording.reconstruction,
        "reference/source mismatch"
    );
    assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());

    let mut remaining = frames.iter();
    let mut source = |dst: &mut [u8]| {
        let Some(frame) = remaining.next() else {
            return Ok(false);
        };
        dst.copy_from_slice(frame.data());
        Ok(true)
    };
    let mut source_bytes = Vec::new();
    let mut source_recon = Vec::new();
    framefinery_codecs::encode_source(
        &config,
        &mut source,
        &mut source_bytes,
        Some(&mut source_recon),
        None,
    )
    .unwrap();
    assert_eq!(
        source_bytes, bytes,
        "unknown-length source/session bitstream parity"
    );
    assert_eq!(source_recon, recording.reconstruction);
    recording
}

#[test]
#[ignore = "requires pinned AVM/VTM decoders for enabled codecs; see docs/validation.md"]
fn required_lossless_changed_texture_preserves_prediction_benefit() {
    let codecs = [
        #[cfg(feature = "av2")]
        ("av2", "FRAMEFINERY_AV2_DECODER", 1024),
        #[cfg(feature = "vvc")]
        ("vvc", "FRAMEFINERY_VVC_DECODER", 128),
    ];
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::var_os("FRAMEFINERY_TEST_ARTIFACT_DIR")
        .expect("set an external artifact directory to preserve required-reference evidence");
    let root = Path::new(&root).join(format!(
        "prediction-preservation-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    eprintln!("Preserving prediction evidence at {}", root.display());
    for (codec, variable, width) in codecs {
        let decoder = env::var_os(variable).expect("required decoder missing; no reference skip");
        for bits in [8, 10] {
            let info = FrameInfo::new(width, 64, PixelFormat::yuv420(bits).unwrap()).unwrap();
            let frames = fixture::frames(info);
            for index in [1, 5] {
                assert_ne!(
                    frames[index],
                    frames[index - 1],
                    "must change source pixels"
                );
            }
            assert_eq!(frames[1], frames[2]);
            assert_eq!(frames[3], frames[4]);
            let source: Vec<_> = frames
                .iter()
                .flat_map(|frame| frame.data().iter().copied())
                .collect();
            fs::write(root.join(format!("{codec}-{bits}-source.yuv")), &source).unwrap();
            let config = |gop| {
                VideoEncoderConfig::new(CodecId::new(codec).unwrap(), info)
                    .with_rate_control(VideoRateControl::Lossless)
                    .with_reconstruction(ReconstructionMode::Frames)
                    .with_setting(VideoEncoderSetting::integer("gop", gop).unwrap())
            };
            let predictive = encode_and_decode(
                config(3),
                &frames,
                Path::new(&decoder),
                &root.join(format!("{codec}-{bits}-predictive")),
            );
            let intra = encode_and_decode(
                config(0),
                &frames,
                Path::new(&decoder),
                &root.join(format!("{codec}-{bits}-intra")),
            );
            eprintln!(
                "{codec}/{bits} per-frame bytes: predictive={:?}, intra={:?}",
                predictive.chunks.iter().map(Vec::len).collect::<Vec<_>>(),
                intra.chunks.iter().map(Vec::len).collect::<Vec<_>>()
            );
            assert_eq!(check_benefit(&predictive, &intra, &source), Ok(()));
            // A genuinely decoded all-intra stream with identical lossless pixels
            // must fail, even if a caller relabels its chunks as predictive.
            assert!(check_benefit(&intra, &intra, &source).is_err());
        }
    }
}

#[test]
fn prediction_evidence_rejects_omission_corruption_and_all_intra_substitution() {
    let source = vec![7; 24];
    let intra = Recording {
        chunks: vec![vec![1; 100]; 6],
        reconstruction: source.clone(),
    };
    let valid = || Recording {
        chunks: vec![vec![1; 10]; 6],
        reconstruction: source.clone(),
    };
    assert_eq!(check_benefit(&valid(), &intra, &source), Ok(()));
    assert!(check_benefit(&intra, &intra, &source).is_err());
    let mut missing = valid();
    missing.chunks.pop();
    assert!(check_benefit(&missing, &intra, &source).is_err());
    let mut empty = valid();
    empty.chunks[1].clear();
    assert!(check_benefit(&empty, &intra, &source).is_err());
    let mut corrupt = valid();
    corrupt.reconstruction[0] ^= 1;
    assert!(check_benefit(&corrupt, &intra, &source).is_err());
    let mut repeat_only = valid();
    for index in [1, 5] {
        repeat_only.chunks[index] = intra.chunks[index].clone();
    }
    assert!(check_benefit(&repeat_only, &intra, &source).is_err());
}
