use super::*;

const SAMPLINGS: [(ChromaSampling, usize, usize); 3] = [
    (ChromaSampling::Cs420, 2, 2),
    (ChromaSampling::Cs422, 2, 1),
    (ChromaSampling::Cs444, 1, 1),
];

fn sample_at(component: usize, x: usize, y: usize, bits: u8) -> u16 {
    let max = (1_u16 << bits) - 1;
    match (x + 3 * y + 5 * component) % 8 {
        0 => 0,
        1 => max,
        2 => 1,
        3 => max - 1,
        // Component and coordinate changes exercise both high and low bits.
        _ => ((109 * (component + 1) + 37 * x + 53 * y) as u16) & max,
    }
}

fn frame_and_expected_bytes(
    width: usize,
    height: usize,
    bits: u8,
    sampling: (ChromaSampling, usize, usize),
    padding: u16,
) -> (VvcReconstructionFrame, Vec<u8>) {
    let (chroma_sampling, sx, sy) = sampling;
    let geometry = VvcVideoGeometry { width, height };
    let format = VvcPictureFormat {
        chroma_sampling,
        bit_depth: SampleBitDepth::new(bits).unwrap(),
    };
    geometry
        .validate_against_format(VvcVideoLimits::unbounded(), format)
        .unwrap();
    let mut frame = VvcReconstructionFrame::new_neutral(geometry, format);
    // Independent fixture geometry: the coded canvas rounds up to 8 samples.
    let coded_width = width.div_ceil(8) * 8;
    let coded_height = height.div_ceil(8) * 8;
    let planes = [
        (&mut frame.luma, coded_width, coded_height, width, height),
        (
            &mut frame.cb,
            coded_width / sx,
            coded_height / sy,
            width / sx,
            height / sy,
        ),
        (
            &mut frame.cr,
            coded_width / sx,
            coded_height / sy,
            width / sx,
            height / sy,
        ),
    ];
    let mut expected = Vec::new();
    for (component, (plane, stride, rows, visible_width, visible_height)) in
        planes.into_iter().enumerate()
    {
        assert_eq!(plane.len(), stride * rows);
        plane.fill(padding);
        for y in 0..visible_height {
            for x in 0..visible_width {
                let value = sample_at(component, x, y, bits);
                plane[y * stride + x] = value;
                // Byte oracle does not call reconstruction/layout helpers,
                // to_sample_yuv, pack_planar_samples or to_le_bytes.
                expected.push((value & 0xff) as u8);
                if bits > 8 {
                    expected.push((value >> 8) as u8);
                }
            }
        }
    }
    (frame, expected)
}

#[test]
fn reconstruction_packing_matches_visible_coordinates_and_bytes() {
    for bits in 8..=12 {
        for sampling @ (_, sx, sy) in SAMPLINGS {
            for (width, height) in [
                (sx, sy),
                (3 * sx, 5 * sy),
                (8, 16),
                (17 * sx, 9 * sy),
                (33 * sx, 35 * sy),
            ] {
                let (frame, expected) =
                    frame_and_expected_bytes(width, height, bits, sampling, (1 << bits) - 1);
                let output = frame.to_yuv();
                assert_eq!(
                    output, expected,
                    "visible byte contract: {width}x{height}, {bits} bits, {:?}",
                    sampling.0
                );

                // Also compare with the scalar sample-assembly route.
                let mut old_output = vec![0; expected.len()];
                pack_planar_samples(
                    &frame.to_sample_yuv(),
                    &mut old_output,
                    frame.format.bit_depth,
                );
                assert_eq!(output, old_output);
            }
        }
    }
}

#[test]
fn reconstruction_packing_excludes_padding_and_preserves_frame_storage() {
    for bits in 8..=12 {
        for sampling in SAMPLINGS {
            let (zero_padding, expected) = frame_and_expected_bytes(18, 14, bits, sampling, 0);
            let (max_padding, _) =
                frame_and_expected_bytes(18, 14, bits, sampling, (1 << bits) - 1);
            assert_ne!(zero_padding.luma, max_padding.luma);
            assert_ne!(zero_padding.cb, max_padding.cb);
            assert_ne!(zero_padding.cr, max_padding.cr);
            let before = max_padding.clone();
            assert_eq!(zero_padding.to_yuv(), expected);
            let mut output = max_padding.to_yuv();
            assert_eq!(output, expected);
            output.fill(0xff);
            assert_eq!(max_padding, before, "packing must only read coded storage");
            assert_eq!(max_padding.to_yuv(), expected);
        }
    }
}
