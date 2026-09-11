//! Required VTM regression for the shared lossless single-tree YUV444 path.
#![cfg(feature = "vvc")]

use framefinery_api::{
    CodecId, FrameInfo, PixelFormat, VideoEncoderConfig, VideoEncoderSetting, VideoRateControl,
};
use std::{env, fs, path::Path, process::Command, time::SystemTime};

#[test]
#[ignore = "requires FRAMEFINERY_VVC_DECODER; see docs/validation.md"]
fn vvc_required_reference_decodes_lossless_scc_444() {
    let decoder = env::var_os("FRAMEFINERY_VVC_DECODER")
        .expect("set FRAMEFINERY_VVC_DECODER to the pinned VTM decoder; no skip is allowed");
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = env::var_os("FRAMEFINERY_TEST_ARTIFACT_DIR")
        .map_or_else(env::temp_dir, Into::into)
        .join(format!("vvc-reference-scc-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    eprintln!("Preserving reference artifacts at {}", root.display());
    let mut failures = Vec::new();
    for (depth, width, height) in
        (8..=12).flat_map(|depth| [(depth, 128, 64), (depth, 192, 128), (depth, 640, 128)])
    {
        let mixed = height > 64;
        let info = FrameInfo::new(width, height, PixelFormat::yuv444(depth).unwrap()).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("vvc").unwrap(), info)
            .with_rate_control(VideoRateControl::Lossless)
            .with_setting(VideoEncoderSetting::integer("gop", 0).unwrap())
            .with_setting(VideoEncoderSetting::text("fast-search", "lossless-speed").unwrap());
        // The first constant frame and later changed/repeated halves reproduce
        // the original failure without external media. Keep all three planes
        // distinct so chroma interpretation cannot accidentally match luma.
        let mut phases = [0, 90, 90, 91, 91, 92].into_iter();
        let mut source_bytes = Vec::new();
        let mut source = |frame: &mut [u8]| -> framefinery_api::Result<bool> {
            let Some(phase) = phases.next() else {
                return Ok(false);
            };
            for channel in 0..3 {
                for y in 0..info.height {
                    for x in 0..info.width {
                        let delta = if x < info.width / 2 {
                            0
                        } else if mixed {
                            if (x / 8 + y / 8) % 2 == 0 {
                                0
                            } else {
                                phase + 37
                            }
                        } else {
                            phase
                        };
                        let value = 16 + channel * 32 + delta;
                        framefinery_api::write_planar_sample(
                            frame,
                            (channel * info.height + y) * info.width + x,
                            (value as u16) << (depth - 8),
                            info.format.bit_depth(),
                        )
                        .expect("validated planar frame geometry");
                    }
                }
            }
            // Explicit caller-owned recording for reconstruction validation.
            source_bytes.extend_from_slice(frame);
            Ok(true)
        };
        let mut encoded = Vec::new();
        let mut internal = Vec::new();
        framefinery_codecs::encode_source(
            &config,
            &mut source,
            &mut encoded,
            Some(&mut internal),
            None,
        )
        .unwrap();
        assert_eq!(source_bytes.len(), info.expected_len() * 6);
        assert!(
            internal == source_bytes,
            "internal/source mismatch at {depth} bits"
        );
        let directory = root.join(format!("444-{depth}-{width}x{height}"));
        fs::create_dir(&directory).unwrap();
        let input = directory.join("stream.vvc");
        let output = directory.join("reference.yuv");
        fs::write(&input, encoded).unwrap();
        fs::write(directory.join("source.yuv"), &source_bytes).unwrap();
        fs::write(directory.join("internal.yuv"), &internal).unwrap();
        let result = Command::new(Path::new(&decoder))
            .arg("-b")
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .expect("required VTM decoder must execute on this host");
        fs::write(directory.join("decoder.stdout"), result.stdout).unwrap();
        fs::write(directory.join("decoder.stderr"), result.stderr).unwrap();
        let exact = result.status.success()
            && fs::read(&output).is_ok_and(|reference| reference == internal);
        eprintln!("YUV444 {depth}-bit six-frame lossless SCC {width}x{height}: exact={exact}");
        if !exact {
            failures.push((depth, width, height));
        }
    }
    assert!(
        failures.is_empty(),
        "VTM mismatch at depths {failures:?}; see {}",
        root.display()
    );
}
