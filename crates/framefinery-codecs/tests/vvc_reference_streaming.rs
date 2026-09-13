//! Required open-session VTM validation with portable generated frames.
#![cfg(feature = "vvc")]

#[path = "support/vvc_streaming.rs"]
mod fixtures;

use framefinery_api::{VideoChunkKind, VideoEncodeOutput};
use std::{env, fs, path::Path, process::Command, time::SystemTime};

#[test]
#[ignore = "requires FRAMEFINERY_VVC_DECODER; see docs/validation.md"]
fn vvc_required_reference_decodes_complete_prefixes_before_flush() {
    let decoder = env::var_os("FRAMEFINERY_VVC_DECODER")
        .expect("set FRAMEFINERY_VVC_DECODER to the pinned VTM decoder; no skip is allowed");
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::var_os("FRAMEFINERY_TEST_ARTIFACT_DIR")
        .map_or_else(env::temp_dir, Into::into)
        .join(format!(
            "vvc-reference-streaming-{}-{stamp}",
            std::process::id()
        ));
    fs::create_dir_all(&root).unwrap();
    eprintln!("Preserving reference artifacts at {}", root.display());
    let cases = fixtures::formats()
        .into_iter()
        .flat_map(|format| [(format, true, 3), (format, false, 3)])
        .chain((8..=12).map(|bits| (framefinery_api::PixelFormat::yuv444(bits).unwrap(), true, 0)));
    for (format, lossless, gop) in cases {
        let config = fixtures::config(format, lossless, gop);
        let info = config.input;
        let mut session = framefinery_codecs::create_encoder(config).unwrap();
        let directory = root.join(format!("{format}-lossless-{lossless}-gop-{gop}"));
        fs::create_dir(&directory).unwrap();
        // Explicit caller-owned recording for prefix validation only.
        let mut prefix = Vec::new();
        let mut source = Vec::new();
        let mut internal = Vec::new();
        for (index, phase) in [0, 90, 90, 91, 91, 92].into_iter().enumerate() {
            let frame = fixtures::frame(info, phase);
            source.extend_from_slice(frame.data());
            let step = session.encode_frame(frame).unwrap();
            assert_eq!(
                step.chunks.len(),
                1,
                "complete frame output is required before flush"
            );
            assert_eq!(step.chunks[0].kind, VideoChunkKind::Frame);
            assert_eq!(step.chunks[0].frame_index, Some(index));
            assert_eq!(step.chunks[0].keyframe, index == 0 || gop == 0);
            assert_eq!(step.reconstructions.len(), 1);
            prefix.extend_from_slice(&step.chunks[0].data);
            internal.extend_from_slice(step.reconstructions[0].data());
            drop(step);
            let count = index + 1;
            let input = directory.join(format!("prefix-{count}.vvc"));
            let output = directory.join(format!("prefix-{count}-reference.yuv"));
            fs::write(&input, &prefix).unwrap();
            fs::write(directory.join("source.yuv"), &source).unwrap();
            fs::write(directory.join("internal.yuv"), &internal).unwrap();
            // EOF belongs to this decoder invocation; the encoder remains open.
            let result = Command::new(Path::new(&decoder))
                .arg("-b")
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .output()
                .expect("required VTM decoder must execute on this host");
            fs::write(
                directory.join(format!("prefix-{count}.stdout")),
                &result.stdout,
            )
            .unwrap();
            fs::write(
                directory.join(format!("prefix-{count}.stderr")),
                result.stderr,
            )
            .unwrap();
            assert!(result.status.success(), "VTM failed: {}", input.display());
            let reference = fs::read(&output).unwrap();
            assert_eq!(reference.len(), info.expected_len() * count);
            assert!(
                reference == internal,
                "reference/internal mismatch: {}",
                input.display()
            );
            if lossless {
                assert!(reference == source, "lossless source mismatch");
            }
            if count == 6 && gop == 3 {
                // Read actual decoded slice/reference syntax, not chunk metadata.
                // The repeated frame must reference POC 1; the GOP reset must
                // be intra. For lossy 4208, changed frames also exercise P coding.
                let trace = String::from_utf8_lossy(&result.stdout);
                let picture = |poc: usize| {
                    trace
                        .lines()
                        .find(|line| {
                            let mut words = line.split_whitespace();
                            words.next() == Some("POC")
                                && words.next().and_then(|s| s.parse::<usize>().ok()) == Some(poc)
                        })
                        .expect("required VTM trace must describe each decoded picture")
                };
                assert!(picture(2).contains("P-SLICE") && picture(2).contains("[L0 1]"));
                assert!(picture(3).contains("I-SLICE") && picture(3).contains("[L0]"));
                if !lossless && format == framefinery_api::PixelFormat::Yuv420p8 {
                    for (poc, reference) in [(1, 0), (5, 4)] {
                        assert!(picture(poc).contains("P-SLICE"));
                        assert!(picture(poc).contains(&format!("[L0 {reference}]")));
                    }
                }
            }
        }
        assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());
        assert_eq!(session.flush().unwrap(), VideoEncodeOutput::default());
        eprintln!("{format} lossless={lossless} gop={gop}: six required prefixes passed");
    }
}
