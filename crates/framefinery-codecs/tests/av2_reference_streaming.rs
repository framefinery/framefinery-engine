//! Explicit required-reference check; generated inputs need no private fixtures.
#![cfg(feature = "av2")]

use framefinery_api::{
    CodecId, Frame, FrameInfo, PixelFormat, ReconstructionMode, VideoChunkKind, VideoEncodeOutput,
    VideoEncoderConfig, VideoEncoderSetting, VideoRateControl,
};
use std::{env, fs, path::Path, process::Command, time::SystemTime};

fn changed_frame(info: FrameInfo, value: u16) -> Frame {
    let mut data = vec![0; info.expected_len()];
    let bits = info.format.bit_depth();
    for y in 0..info.height {
        for x in info.width / 2..info.width {
            framefinery_api::write_planar_sample(
                &mut data,
                y * info.width + x,
                value << (bits.bits() - 8),
                bits,
            )
            .unwrap();
        }
    }
    Frame::new(info, data).unwrap()
}

fn obu_types(mut data: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    while !data.is_empty() {
        let mut size = 0usize;
        let mut shift = 0;
        loop {
            let byte = data[0];
            data = &data[1..];
            assert!(shift < usize::BITS);
            size |= usize::from(byte & 127) << shift;
            if byte < 128 {
                break;
            }
            shift += 7;
        }
        assert!(size > 0 && size <= data.len());
        types.push((data[0] >> 2) & 31);
        data = &data[size..];
    }
    types
}

fn decode_required(decoder: &Path, directory: &Path, count: usize, prefix: &[u8]) -> Vec<u8> {
    let input = directory.join(format!("prefix-{count}.obu"));
    let output = directory.join(format!("prefix-{count}-reference.yuv"));
    fs::write(&input, prefix).unwrap();
    let result = Command::new(decoder)
        .arg("--rawvideo")
        .arg("-o")
        .arg(&output)
        .arg(&input)
        .output()
        .expect("required AVM decoder must execute on this host");
    fs::write(
        directory.join(format!("prefix-{count}.stdout")),
        result.stdout,
    )
    .unwrap();
    fs::write(
        directory.join(format!("prefix-{count}.stderr")),
        result.stderr,
    )
    .unwrap();
    assert!(
        result.status.success(),
        "AVM failed; inspect {}",
        directory.display()
    );
    fs::read(output).unwrap()
}

#[test]
#[ignore = "requires FRAMEFINERY_AV2_DECODER; see docs/validation.md#incremental-session-reference-check"]
fn av2_required_reference_decodes_complete_prefixes_before_flush() {
    let decoder = env::var_os("FRAMEFINERY_AV2_DECODER").expect(
        "set FRAMEFINERY_AV2_DECODER to the pinned AVM decoder; no reference skip is allowed",
    );
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::var_os("FRAMEFINERY_TEST_ARTIFACT_DIR")
        .map_or_else(env::temp_dir, Into::into)
        .join(format!(
            "av2-reference-streaming-{}-{stamp}",
            std::process::id()
        ));
    fs::create_dir_all(&root).unwrap();
    eprintln!("Preserving reference artifacts at {}", root.display());
    for bits in [8, 10] {
        for lossless in [true, false] {
            let info = FrameInfo::new(1024, 64, PixelFormat::yuv420(bits).unwrap()).unwrap();
            let config = VideoEncoderConfig::new(CodecId::new("av2").unwrap(), info)
                .with_rate_control(if lossless {
                    VideoRateControl::Lossless
                } else {
                    VideoRateControl::ConstantQuantizer(24)
                })
                .with_reconstruction(ReconstructionMode::Frames)
                .with_setting(VideoEncoderSetting::integer("gop", 3).unwrap());
            let mut session = framefinery_codecs::create_encoder(config).unwrap();
            let directory = root.join(format!("420-{bits}-lossless-{lossless}"));
            fs::create_dir(&directory).unwrap();
            // Explicit caller-owned recording for validation, never retained by the encoder.
            let mut prefix = Vec::new();
            let mut internal = Vec::new();
            let mut source = Vec::new();
            for (index, value) in [0, 90, 90, 91, 91, 92].into_iter().enumerate() {
                let frame = changed_frame(info, value);
                source.extend_from_slice(frame.data());
                let step = session.encode_frame(frame).unwrap();
                assert_eq!(step.chunks.len(), 1);
                assert_eq!(step.chunks[0].kind, VideoChunkKind::Frame);
                assert_eq!(step.chunks[0].frame_index, Some(index));
                if index % 3 == 0 {
                    assert!(step.chunks[0].keyframe);
                }
                assert_eq!(step.reconstructions.len(), 1);
                prefix.extend_from_slice(&step.chunks[0].data);
                internal.extend_from_slice(step.reconstructions[0].data());
                drop(step);
                fs::write(directory.join("internal.yuv"), &internal).unwrap();
                fs::write(directory.join("source.yuv"), &source).unwrap();
                // Decode the complete ordered prefix while the same encoder is still active.
                let reference =
                    decode_required(Path::new(&decoder), &directory, index + 1, &prefix);
                assert_eq!(reference.len(), info.expected_len() * (index + 1));
                assert!(
                    reference == internal,
                    "reference/internal mismatch: {} frame {index}",
                    directory.display()
                );
                if lossless {
                    assert!(reference == source, "lossless source mismatch");
                }
            }
            assert!(
                obu_types(&prefix).contains(&7),
                "must exercise regular predictive tile coding"
            );
            assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());
            assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());
        }
    }
}
