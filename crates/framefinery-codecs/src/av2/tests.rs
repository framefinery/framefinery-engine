use super::syntax::Av2SyntaxCode;
use super::*;

#[test]
fn av2_lossless_chroma_candidate_minimum_preserves_exhaustive_order() {
    let candidates = [
        (false, super::palette::Av2ChromaIntraMode::Horizontal, 0),
        (false, super::palette::Av2ChromaIntraMode::Vertical, 192),
        (true, super::palette::Av2ChromaIntraMode::Horizontal, 64),
    ];
    for scores in [[8, 2, 0], [8, 2, 128], [1, 1, 1], [40, 0, 20]] {
        for allow_bdpcm in [false, true] {
            let mut expected = None;
            let mut expected_key = usize::MAX;
            for (index, &(use_bdpcm, _, syntax_penalty)) in candidates.iter().enumerate() {
                if use_bdpcm && !allow_bdpcm {
                    continue;
                }
                let key = scores[index] + syntax_penalty;
                if key < expected_key {
                    expected = Some(index);
                    expected_key = key;
                }
            }
            assert_eq!(
                super::tile::best_lossless_chroma_candidate_index(
                    &candidates,
                    &scores,
                    allow_bdpcm,
                ),
                expected,
                "separable chroma minimum must match exhaustive candidate order"
            );
        }
    }
}

#[test]
fn av2_accepts_basic_yuv_request_shape() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry: Av2VideoGeometry {
            width: 64,
            height: 64,
        },
        format: PixelFormat::Yuv420p8,
    };

    assert!(request.validate().is_ok());
}

#[test]
fn av2_accepts_non_mi_aligned_request_shape() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry: Av2VideoGeometry {
            width: 64,
            height: 60,
        },
        format: PixelFormat::Yuv420p8,
    };

    assert!(request.validate().is_ok());
}

#[test]
fn av2_dc_average_matches_reference_reciprocal_rounding() {
    assert_eq!(tile::av2_reference_dc_average(2_156, 12), 179);
    assert_eq!(tile::av2_reference_dc_average(2_184, 12), 182);
    assert_eq!(tile::av2_reference_dc_average(1_512, 12), 126);
}

#[test]
fn av2_rejects_empty_request_shape() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry: Av2VideoGeometry {
            width: 0,
            height: 16,
        },
        format: PixelFormat::Yuv444p8,
    };

    let err = request.validate().expect_err("zero width must be rejected");
    assert!(err.contains("non-zero width and height"), "{err}");
}

#[test]
fn av2_rgb24_repack_uses_planar_gbr_identity_order() {
    let geometry = Av2VideoGeometry {
        width: 2,
        height: 1,
    };
    let rgb = vec![1, 2, 3, 4, 5, 6];

    let planar = rgb24_to_planar_gbr(&rgb, geometry);

    assert_eq!(planar, vec![2, 5, 3, 6, 1, 4]);
    assert_eq!(planar_gbr_to_rgb24(&planar, geometry), rgb);
}

#[test]
fn av2_rgb24_lossless_emits_identity_metadata_and_packed_recon() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format: PixelFormat::Rgb24,
    };
    let frame_len = Picture::expected_len(geometry.width, geometry.height, request.format);
    let input: Vec<u8> = (0..frame_len)
        .map(|index| ((index * 17 + 3) & 0xff) as u8)
        .collect();
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: true,
            ..Default::default()
        },
        None,
    )
    .expect("AV2 rgb24 lossless encode should preserve packed RGB bytes");

    assert_eq!(recon, input);
    let ci_header = av2_obu_header(Av2ObuType::ContentInterpretation);
    assert!(
        output
            .windows(ci_header.len())
            .any(|window| window == ci_header.as_slice()),
        "RGB identity stream should carry a content-interpretation OBU"
    );
}

#[test]
fn av2_gbrp8_lossless_emits_identity_metadata_and_planar_recon() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format: PixelFormat::Gbrp8,
    };
    let frame_len = Picture::expected_len(geometry.width, geometry.height, request.format);
    let input: Vec<u8> = (0..frame_len)
        .map(|index| ((index * 19 + 5) & 0xff) as u8)
        .collect();
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: true,
            ..Default::default()
        },
        None,
    )
    .expect("AV2 gbrp8 lossless encode should preserve planar RGB bytes");

    assert_eq!(recon, input);
    let ci_header = av2_obu_header(Av2ObuType::ContentInterpretation);
    assert!(
        output
            .windows(ci_header.len())
            .any(|window| window == ci_header.as_slice()),
        "RGB identity stream should carry a content-interpretation OBU"
    );
}

#[test]
fn av2_gbrp8_lossless_crops_coded_padding_to_visible_recon() {
    let geometry = Av2VideoGeometry {
        width: 13,
        height: 11,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format: PixelFormat::Gbrp8,
    };
    let frame_len = Picture::expected_len(geometry.width, geometry.height, request.format);
    let input: Vec<u8> = (0..frame_len)
        .map(|index| ((index * 23 + 7) & 0xff) as u8)
        .collect();
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: true,
            ..Default::default()
        },
        None,
    )
    .expect("AV2 gbrp8 lossless encode should support non-8-aligned visible geometry");

    assert_eq!(recon, input);
    let stream_format =
        Av2StreamFormat::from_pixel_format(request.format).expect("gbrp8 maps to AV2 4:4:4");
    let sequence = av2_mvp_sequence_header_payload_for_visible(
        geometry.coded(),
        geometry,
        Av2Black444MvpProfile::current(),
        stream_format,
    );
    assert_has_field_with_bit_count(
        &sequence,
        "sequence_header.conf_win_enabled_flag",
        Av2SyntaxCode::Flag,
        1,
    );
    assert_has_field_with_bit_count(
        &sequence,
        "sequence_header.conf_win_right_offset",
        Av2SyntaxCode::Uvlc,
        expected_uvlc_bit_count(geometry.crop_right()),
    );
    assert_has_field_with_bit_count(
        &sequence,
        "sequence_header.conf_win_bottom_offset",
        Av2SyntaxCode::Uvlc,
        expected_uvlc_bit_count(geometry.crop_bottom()),
    );
}

#[test]
fn av2_rgb24_non_lossless_emits_identity_metadata_and_packed_recon() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format: PixelFormat::Rgb24,
    };
    let frame_len = Picture::expected_len(geometry.width, geometry.height, request.format);
    let input: Vec<u8> = (0..frame_len)
        .map(|index| ((index * 29 + 7) & 0xff) as u8)
        .collect();
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            gop: crate::settings::GopMode::IntraOnly,
            ..Default::default()
        },
        None,
    )
    .expect("AV2 rgb24 non-lossless encode should keep public RGB byte layout");

    assert_eq!(recon, input);
    let ci_header = av2_obu_header(Av2ObuType::ContentInterpretation);
    assert!(
        output
            .windows(ci_header.len())
            .any(|window| window == ci_header.as_slice()),
        "RGB identity stream should carry a content-interpretation OBU"
    );
}

#[test]
fn av2_rgb_identity_content_interpretation_uses_srgb_idc() {
    let payload = av2_rgb_identity_content_interpretation_payload();

    assert!(
        payload.fields.iter().any(|field| {
            field.name == "content_interpretation.color_description_idc"
                && field.code == Av2SyntaxCode::RiceGolomb
                && field.bit_count == 4
        }),
        "sRGB color_description_idc=4 should be Rice-Golomb coded with k=2"
    );
    assert!(payload.fields.iter().any(|field| {
        field.name == "content_interpretation.full_range_flag" && field.code == Av2SyntaxCode::Flag
    }));
}

#[test]
fn av2_fixed_black_444_emits_generated_obu_stream_and_reconstruction() {
    for geometry in supported_black_444_geometries() {
        let request = Av2EncodeRequest {
            params: Av2EncodeParams { frames: 1 },
            geometry,
            format: PixelFormat::Yuv444p8,
        };
        let input =
            av2_black_444_reconstruction(geometry).expect("supported AV2 fixed black geometry");
        let mut source = input.as_slice();
        let mut output = Vec::new();
        let mut recon = Vec::new();

        let result =
            av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request);

        result.expect("AV2 OBU encode should succeed");
        assert_eq!(output, av2_black_444_bitstream_for_geometry(geometry));
        assert_eq!(&output[..2], &[0x01, 0x08]);
        assert_ne!(output, input);
        assert_eq!(recon, input);
    }
}

#[test]
fn av2_mvp_444_encodes_all_requested_frames() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 2 },
        geometry,
        format: PixelFormat::Yuv444p8,
    };
    let frame_len = Picture::expected_len(geometry.width, geometry.height, request.format);
    let first = vec![0; frame_len];
    let mut second = vec![0; frame_len];
    for sample in second.iter_mut().take(geometry.width * geometry.height) {
        *sample = 73;
    }
    let mut input = first.clone();
    input.extend_from_slice(&second);
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request)
        .expect("AV2 MVP stream encode should process every requested frame");

    let mut expected_output = av2_mvp_444_bitstream_for_mode(
        geometry,
        geometry,
        request.format.bit_depth(),
        &Av2Mvp444FrameMode::from_frame(&first, geometry, request.format.bit_depth())
            .expect("first frame mode"),
        false,
    );
    expected_output.extend_from_slice(&av2_mvp_444_bitstream_for_mode(
        geometry,
        geometry,
        request.format.bit_depth(),
        &Av2Mvp444FrameMode::from_frame(&second, geometry, request.format.bit_depth())
            .expect("second frame mode"),
        false,
    ));
    assert_eq!(output, expected_output);
    assert_eq!(recon, input);
}

#[test]
fn av2_zero_frames_streams_complete_frames_until_eof() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 0 },
        geometry,
        format: PixelFormat::Yuv444p8,
    };
    let frame_len = Picture::expected_len(geometry.width, geometry.height, request.format);
    let first = vec![0; frame_len];
    let mut second = vec![0; frame_len];
    for sample in second.iter_mut().take(geometry.width * geometry.height) {
        *sample = 31;
    }
    let mut input = first.clone();
    input.extend_from_slice(&second);
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();
    let mut metrics_frame_count = Vec::new();
    let mut metrics = |metrics: Av2EncodeFrameMetrics<'_>| {
        metrics_frame_count.push(metrics.frame_count);
    };

    av2_encode_fixed_black_444_with_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Some(&mut metrics),
    )
    .expect("AV2 zero-frame request should stream until EOF");

    assert!(!output.is_empty());
    assert_eq!(recon, input);
    assert_eq!(metrics_frame_count, vec![None, None]);
}

#[test]
fn av2_lossless_predictive_reuses_repeated_frames_as_sef() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 3 },
        geometry,
        format: PixelFormat::Yuv420p8,
    };
    let frame_len = Picture::expected_len(geometry.width, geometry.height, request.format);
    let frame: Vec<u8> = (0..frame_len)
        .map(|index| ((index * 17 + 23) & 0xff) as u8)
        .collect();
    let mut input = Vec::with_capacity(frame_len * request.params.frames);
    for _ in 0..request.params.frames {
        input.extend_from_slice(&frame);
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();
    let mut frame_sizes = Vec::new();
    let mut metrics = |metrics: Av2EncodeFrameMetrics<'_>| {
        assert_eq!(metrics.source, metrics.reconstruction);
        frame_sizes.push(metrics.bitstream_bytes);
    };

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: true,
            qp: None,
            gop: crate::settings::GopMode::Infinite,
        },
        Some(&mut metrics),
    )
    .expect("AV2 lossless predictive repeated-frame encode should succeed");

    assert_eq!(recon, input);
    assert_eq!(frame_sizes.len(), 3);
    assert!(frame_sizes[0] > frame_sizes[1]);
    assert_eq!(frame_sizes[1], 6);
    assert_eq!(frame_sizes[2], 6);
    assert_eq!(output.len(), frame_sizes.iter().sum::<usize>());
}

#[test]
fn av2_lossy_predictive_reuses_repeated_frames_as_sef() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 3 },
        geometry,
        format: PixelFormat::Yuv420p8,
    };
    let frame_len = Picture::expected_len(geometry.width, geometry.height, request.format);
    let frame: Vec<u8> = (0..frame_len)
        .map(|index| ((index * 17 + 23) & 0xff) as u8)
        .collect();
    let mut input = Vec::with_capacity(frame_len * request.params.frames);
    for _ in 0..request.params.frames {
        input.extend_from_slice(&frame);
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();
    let mut frame_sizes = Vec::new();
    let mut metrics = |metrics: Av2EncodeFrameMetrics<'_>| {
        frame_sizes.push(metrics.bitstream_bytes);
    };

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: false,
            qp: Some(24),
            gop: crate::settings::GopMode::Infinite,
        },
        Some(&mut metrics),
    )
    .expect("AV2 lossy predictive repeated-frame encode should succeed");

    assert_eq!(recon.len(), input.len());
    assert_eq!(frame_sizes.len(), 3);
    assert!(frame_sizes[0] > frame_sizes[1]);
    assert_eq!(frame_sizes[1], 6);
    assert_eq!(frame_sizes[2], 6);
    assert_eq!(&recon[..frame_len], &recon[frame_len..frame_len * 2]);
    assert_eq!(
        &recon[frame_len..frame_len * 2],
        &recon[frame_len * 2..frame_len * 3]
    );
}

#[test]
fn av2_lossy_predictive_zero_mv_tiles_reuse_previous_reconstruction() {
    let geometry = Av2VideoGeometry {
        width: 1024,
        height: 64,
    };
    let format = PixelFormat::Yuv420p8;
    let stream_format =
        Av2StreamFormat::from_pixel_format(format).expect("yuv420p8 is an AV2 stream format");
    let frame_len = Picture::expected_len(geometry.width, geometry.height, format);
    let first: Vec<u8> = (0..frame_len)
        .map(|index| ((index * 13 + 19) & 0xff) as u8)
        .collect();
    let mut second = first.clone();
    for y in 0..geometry.height {
        let row = y * geometry.width;
        for x in 512..geometry.width {
            second[row + x] = second[row + x].wrapping_add(17);
        }
    }
    let mut frame_stats =
        stats::Av2FrameStats::new(0, geometry, format, stream_format, false, Some(24), -1);

    let (_, first_recon) =
        av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
            geometry,
            geometry,
            stream_format,
            &first,
            24,
            true,
            0,
            false,
            &mut frame_stats,
        );
    let (_, inter_recon) =
        av2_lossy_subsampled_zero_mv_inter_tiles_bitstream_and_reconstruction_for_frame(
            geometry,
            stream_format,
            &second,
            &first,
            &first_recon,
            24,
            1,
        )
        .expect("unchanged left tile should use a zero-MV inter frame");
    let layout = planar::Av2PlanarYuvLayout::new(
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .expect("valid planar layout");

    assert!(layout.regions_equal_between(
        &inter_recon,
        0,
        0,
        &first_recon,
        0,
        0,
        512,
        geometry.height
    ));
    assert!(!layout.regions_equal_between(
        &inter_recon,
        512,
        0,
        &first_recon,
        512,
        0,
        512,
        geometry.height
    ));
}

#[test]
fn av2_lossy_exact_motion_residual_map_uses_newmv_for_shifted_8bit_blocks() {
    let geometry = Av2VideoGeometry {
        width: 1024,
        height: 64,
    };
    let format = PixelFormat::Yuv420p8;
    let first = shifted_tile_reference_frame(geometry);
    let second = shifted_tile_current_frame(&first, geometry);
    let stream_format =
        Av2StreamFormat::from_pixel_format(format).expect("yuv420p8 is an AV2 stream format");
    let motion_map = motion::build_lossless_motion_map(
        &second,
        &first,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .expect("shifted tile motion map should build");
    let right_tile = Av2TileRegion {
        origin_x: 512,
        origin_y: 0,
        width: 512,
        height: 64,
    };
    let residual_blocks = lossy_tile_inter_residual_block_modes(&motion_map, right_tile)
        .expect("shifted tile should expose exact NEWMV residual blocks");
    assert_eq!(
        residual_blocks.block_mode_at(0, 0),
        Some(Av2LosslessInterBlockMode::NewMvResidual {
            row_px: 0,
            col_px: -8
        })
    );
}

#[test]
fn av2_lossy_predictive_requires_qp_for_legacy_444_path() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let format = PixelFormat::Yuv444p8;
    let input = vec![0; Picture::expected_len(geometry.width, geometry.height, format)];
    let mut source = input.as_slice();
    let mut output = Vec::new();

    let err = av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        None,
        Av2EncodeRequest {
            params: Av2EncodeParams { frames: 1 },
            geometry,
            format,
        },
        Av2EncodeOptions {
            lossless: false,
            qp: None,
            gop: crate::settings::GopMode::Infinite,
        },
        None,
    )
    .expect_err("predictive non-lossless 4:4:4 should require the QP residual path");

    assert!(
        err.contains("requires --set qp=<1..255>"),
        "unexpected predictive fallback error: {err}"
    );
}

#[test]
fn av2_lossless_predictive_uses_zero_mv_inter_for_unchanged_tiles() {
    let geometry = Av2VideoGeometry {
        width: 1024,
        height: 64,
    };
    let format = PixelFormat::Yuv420p8;
    let frame_len = Picture::expected_len(geometry.width, geometry.height, format);
    let first = vec![0u8; frame_len];
    let mut second = first.clone();
    for y in 0..geometry.height {
        let row = y * geometry.width;
        for x in 512..geometry.width {
            second[row + x] = 90;
        }
    }
    let mut input = first.clone();
    input.extend_from_slice(&second);
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();
    let mut frame_sizes = Vec::new();
    let mut metrics = |metrics: Av2EncodeFrameMetrics<'_>| {
        assert_eq!(metrics.source, metrics.reconstruction);
        frame_sizes.push(metrics.bitstream_bytes);
    };
    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        Av2EncodeRequest {
            params: Av2EncodeParams { frames: 2 },
            geometry,
            format,
        },
        Av2EncodeOptions {
            lossless: true,
            qp: None,
            gop: crate::settings::GopMode::Infinite,
        },
        Some(&mut metrics),
    )
    .expect("AV2 tile-level zero-MV predictive encode should succeed");

    assert_eq!(recon, input);
    assert_eq!(frame_sizes.len(), 2);
    assert!(frame_sizes[1] > 6);
    assert!(
        frame_sizes[1] < frame_sizes[0],
        "unchanged tile should make the regular inter frame smaller than the first key frame"
    );
}

#[test]
fn av2_lossless_predictive_uses_newmv_inter_for_shifted_tile() {
    let geometry = Av2VideoGeometry {
        width: 1024,
        height: 64,
    };
    let format = PixelFormat::Yuv420p8;
    let frame_len = Picture::expected_len(geometry.width, geometry.height, format);
    let first = shifted_tile_reference_frame(geometry);
    let second = shifted_tile_current_frame(&first, geometry);
    let stream_format =
        Av2StreamFormat::from_pixel_format(format).expect("yuv420p8 is an AV2 stream format");
    let motion_map = motion::build_lossless_motion_map(
        &second,
        &first,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .expect("shifted tile motion map should build");
    let shifted_mv = uniform_lossless_tile_motion(
        &motion_map,
        Av2TileRegion {
            origin_x: 512,
            origin_y: 0,
            width: 512,
            height: 64,
        },
    )
    .expect("right tile should have one uniform exact motion vector");
    assert_eq!(
        shifted_mv,
        Av2MotionVector {
            row_px: 0,
            col_px: -8
        }
    );
    let mut input = Vec::with_capacity(frame_len * 2);
    input.extend_from_slice(&first);
    input.extend_from_slice(&second);
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();
    let mut frame_sizes = Vec::new();
    let mut metrics = |metrics: Av2EncodeFrameMetrics<'_>| {
        assert_eq!(metrics.source, metrics.reconstruction);
        frame_sizes.push(metrics.bitstream_bytes);
    };
    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        Av2EncodeRequest {
            params: Av2EncodeParams { frames: 2 },
            geometry,
            format,
        },
        Av2EncodeOptions {
            lossless: true,
            qp: None,
            gop: crate::settings::GopMode::Infinite,
        },
        Some(&mut metrics),
    )
    .expect("AV2 tile-level NEWMV predictive encode should succeed");

    assert_eq!(recon, input);
    assert_eq!(frame_sizes.len(), 2);
    assert!(
        frame_sizes[1] < frame_sizes[0],
        "shifted tile should make the regular inter frame smaller than the first key frame"
    );
}

#[test]
fn av2_lossless_predictive_uses_mixed_newmv_inter_for_nonuniform_shifted_tile() {
    let geometry = Av2VideoGeometry {
        width: 1024,
        height: 64,
    };
    let format = PixelFormat::Yuv420p8;
    let frame_len = Picture::expected_len(geometry.width, geometry.height, format);
    let first = shifted_tile_reference_frame(geometry);
    let second = mixed_shifted_tile_current_frame(&first, geometry);
    let stream_format =
        Av2StreamFormat::from_pixel_format(format).expect("yuv420p8 is an AV2 stream format");
    let motion_map = motion::build_lossless_motion_map(
        &second,
        &first,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .expect("mixed shifted tile motion map should build");
    let right_tile = Av2TileRegion {
        origin_x: 512,
        origin_y: 0,
        width: 512,
        height: 64,
    };
    assert_eq!(uniform_lossless_tile_motion(&motion_map, right_tile), None);
    assert!(
        lossless_tile_inter_block_modes(&motion_map, right_tile).is_some(),
        "right tile should have exact non-uniform inter block modes"
    );

    let mut input = Vec::with_capacity(frame_len * 2);
    input.extend_from_slice(&first);
    input.extend_from_slice(&second);
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();
    let mut frame_sizes = Vec::new();
    let mut metrics = |metrics: Av2EncodeFrameMetrics<'_>| {
        assert_eq!(metrics.source, metrics.reconstruction);
        frame_sizes.push(metrics.bitstream_bytes);
    };
    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        Av2EncodeRequest {
            params: Av2EncodeParams { frames: 2 },
            geometry,
            format,
        },
        Av2EncodeOptions {
            lossless: true,
            qp: None,
            gop: crate::settings::GopMode::Infinite,
        },
        Some(&mut metrics),
    )
    .expect("AV2 mixed tile-level NEWMV predictive encode should succeed");

    assert_eq!(recon, input);
    assert_eq!(frame_sizes.len(), 2);
    assert!(
        frame_sizes[1] < frame_sizes[0],
        "mixed shifted tile should make the regular inter frame smaller than the first key frame"
    );
}

#[test]
fn av2_lossless_zero_mv_regular_inter_payload_emits_inter_symbols() {
    let geometry = Av2VideoGeometry {
        width: 16,
        height: 16,
    };
    let stream_format = Av2StreamFormat::from_pixel_format(PixelFormat::Yuv420p8)
        .expect("yuv420p8 is an AV2 stream format");
    let profile = Av2Black444MvpProfile::current();
    let entropy = av2_lossless_zero_mv_inter_tile_entropy_payload_for_region_with_fields(
        Av2TileRegion::root(geometry),
        profile,
        stream_format.chroma_format,
        true,
    );
    let names: Vec<_> = entropy.fields.iter().map(|field| field.name).collect();

    assert!(names.contains(&"tile.inter.is_inter"));
    assert!(names.contains(&"tile.inter.skip_txfm"));
    assert!(names.contains(&"tile.inter.single_mode"));

    let payload = av2_lossless_zero_mv_regular_inter_payload(geometry, stream_format, 1);
    assert!(payload
        .fields
        .iter()
        .any(|field| field.name == "tile_group.tile_entropy_payload"));

    let mut obu = Vec::new();
    append_obu(&mut obu, Av2ObuType::RegularTileGroup, &payload);
    assert!(!obu.is_empty());
}

#[test]
fn av2_zero_mv_tile_payload_depends_on_shape_not_origin() {
    let profile = Av2Black444MvpProfile::current();
    let first = av2_lossless_zero_mv_inter_tile_entropy_payload_for_region_with_fields(
        Av2TileRegion {
            origin_x: 0,
            origin_y: 0,
            width: 512,
            height: 64,
        },
        profile,
        Av2ChromaFormat::Yuv420,
        false,
    );
    let shifted = av2_lossless_zero_mv_inter_tile_entropy_payload_for_region_with_fields(
        Av2TileRegion {
            origin_x: 512,
            origin_y: 128,
            width: 512,
            height: 64,
        },
        profile,
        Av2ChromaFormat::Yuv420,
        false,
    );
    assert_eq!(first, shifted);
}

#[test]
fn av2_lossless_newmv_regular_inter_payload_emits_mv_symbols() {
    let geometry = Av2VideoGeometry {
        width: 16,
        height: 16,
    };
    let stream_format = Av2StreamFormat::from_pixel_format(PixelFormat::Yuv420p8)
        .expect("yuv420p8 is an AV2 stream format");
    let profile = Av2Black444MvpProfile::current();
    let entropy = av2_lossless_new_mv_inter_tile_entropy_payload_for_region_with_fields(
        Av2TileRegion::root(geometry),
        profile,
        stream_format.chroma_format,
        -8,
        16,
        true,
    );
    let fields: Vec<_> = entropy
        .fields
        .iter()
        .map(|field| (field.name, field.symbol, field.literal_value))
        .collect();

    assert!(fields
        .iter()
        .any(|(name, symbol, _)| { *name == "tile.inter.single_mode" && *symbol == Some(2) }));
    assert!(fields
        .iter()
        .any(|(name, _, _)| *name == "tile.inter.mv.shell_set"));
    assert!(fields
        .iter()
        .any(|(name, _, literal)| { *name == "tile.inter.mv.sign" && *literal == Some(1) }));
}

fn shifted_tile_reference_frame(geometry: Av2VideoGeometry) -> Vec<u8> {
    assert_eq!(geometry.width, 1024);
    assert_eq!(geometry.height, 64);
    let mut frame =
        vec![0; Picture::expected_len(geometry.width, geometry.height, PixelFormat::Yuv420p8,)];
    let y_len = geometry.width * geometry.height;
    let chroma_width = geometry.width / 2;
    let chroma_height = geometry.height / 2;
    let chroma_len = chroma_width * chroma_height;
    let u_offset = y_len;
    let v_offset = y_len + chroma_len;

    for y in 0..geometry.height {
        for x in 0..geometry.width {
            frame[y * geometry.width + x] = ((x * 3 + y * 5 + 17) & 0xff) as u8;
        }
    }
    for y in 0..chroma_height {
        for x in 0..chroma_width {
            frame[u_offset + y * chroma_width + x] = ((x * 7 + y * 11 + 31) & 0xff) as u8;
            frame[v_offset + y * chroma_width + x] = ((x * 13 + y * 17 + 59) & 0xff) as u8;
        }
    }
    frame
}

fn shifted_tile_current_frame(reference: &[u8], geometry: Av2VideoGeometry) -> Vec<u8> {
    let mut frame = vec![0; reference.len()];
    let y_len = geometry.width * geometry.height;
    let chroma_width = geometry.width / 2;
    let chroma_height = geometry.height / 2;
    let chroma_len = chroma_width * chroma_height;
    let u_offset = y_len;
    let v_offset = y_len + chroma_len;

    for y in 0..geometry.height {
        let row = y * geometry.width;
        for x in 0..512 {
            frame[row + x] = 233;
        }
        for x in 512..geometry.width {
            frame[row + x] = reference[row + x - 8];
        }
    }

    for y in 0..chroma_height {
        let row = y * chroma_width;
        for x in 0..256 {
            frame[u_offset + row + x] = 129;
            frame[v_offset + row + x] = 55;
        }
        for x in 256..chroma_width {
            frame[u_offset + row + x] = reference[u_offset + row + x - 4];
            frame[v_offset + row + x] = reference[v_offset + row + x - 4];
        }
    }

    frame
}

fn mixed_shifted_tile_current_frame(reference: &[u8], geometry: Av2VideoGeometry) -> Vec<u8> {
    let mut frame = vec![0; reference.len()];
    let y_len = geometry.width * geometry.height;
    let chroma_width = geometry.width / 2;
    let chroma_height = geometry.height / 2;
    let chroma_len = chroma_width * chroma_height;
    let u_offset = y_len;
    let v_offset = y_len + chroma_len;

    for y in 0..geometry.height {
        let row = y * geometry.width;
        let shift = if y < geometry.height / 2 { 8 } else { 16 };
        for x in 0..512 {
            frame[row + x] = 233;
        }
        for x in 512..geometry.width {
            frame[row + x] = reference[row + x - shift];
        }
    }

    for y in 0..chroma_height {
        let row = y * chroma_width;
        let shift = if y < chroma_height / 2 { 4 } else { 8 };
        for x in 0..256 {
            frame[u_offset + row + x] = 129;
            frame[v_offset + row + x] = 55;
        }
        for x in 256..chroma_width {
            frame[u_offset + row + x] = reference[u_offset + row + x - shift];
            frame[v_offset + row + x] = reference[v_offset + row + x - shift];
        }
    }

    frame
}

#[test]
fn av2_mvp_444_accepts_high_bit_depth_yuv444_without_downscaling() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    for bits in [10] {
        let format = PixelFormat::yuv444(bits).expect("valid AV2 high-depth 4:4:4 format");
        let request = Av2EncodeRequest {
            params: Av2EncodeParams { frames: 1 },
            geometry,
            format,
        };
        let max_sample = format.bit_depth().max_sample();
        let mid_sample = 1u16 << u32::from(bits - 1);
        let plane_len = geometry.width * geometry.height;
        let frame_len = Picture::expected_len(geometry.width, geometry.height, format);
        let mut input = vec![0; frame_len];
        for sample_index in 0..plane_len {
            let x = sample_index % geometry.width;
            let y = sample_index / geometry.width;
            let y_sample = if (x + y) % 2 == 0 { 0 } else { max_sample - 3 };
            let u_sample = mid_sample + ((x * 3 + y) % 8) as u16;
            let v_sample = (max_sample / 8) + ((x + y * 5) % 16) as u16;
            framefinery_api::write_planar_sample(
                &mut input,
                sample_index,
                y_sample,
                format.bit_depth(),
            )
            .expect("write Y sample");
            framefinery_api::write_planar_sample(
                &mut input,
                plane_len + sample_index,
                u_sample,
                format.bit_depth(),
            )
            .expect("write U sample");
            framefinery_api::write_planar_sample(
                &mut input,
                2 * plane_len + sample_index,
                v_sample,
                format.bit_depth(),
            )
            .expect("write V sample");
        }
        let mut source = input.as_slice();
        let mut output = Vec::new();
        let mut recon = Vec::new();

        av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request)
            .expect("AV2 high-depth 4:4:4 encode should succeed");

        assert!(!output.is_empty());
        assert_eq!(recon, input);
        let sequence = av2_mvp_444_sequence_header_payload(
            geometry,
            format.bit_depth(),
            Av2Black444MvpProfile::current(),
        );
        assert_has_field(
            &sequence,
            "sequence_header.bitdepth_lut_idx",
            Av2SyntaxCode::Uvlc,
            15,
            expected_uvlc_bit_count(
                Av2StreamFormat::from_pixel_format(format)
                    .expect("valid AV2 stream format")
                    .bitdepth_lut_index(),
            ),
        );
    }
}

#[test]
fn av2_fixed_black_420_can_use_exact_residual_reconstruction() {
    let geometry = Av2VideoGeometry {
        width: 16,
        height: 16,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format: PixelFormat::Yuv420p8,
    };
    let input =
        vec![0; Picture::expected_len(geometry.width, geometry.height, PixelFormat::Yuv420p8,)];
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request)
        .expect("AV2 4:2:0 black OBU encode should succeed");

    assert_ne!(output, input);
    assert_ne!(
        output,
        av2_black_bitstream_for_geometry(geometry, Av2StreamFormat::yuv420_8())
    );
    assert_eq!(recon, input);
    assert_eq!(recon.len(), input.len());
    let sequence = av2_mvp_sequence_header_payload(
        geometry,
        Av2Black444MvpProfile::current(),
        Av2StreamFormat::yuv420_8(),
    );
    assert_has_field(
        &sequence,
        "sequence_header.seq_chroma_format_idc",
        Av2SyntaxCode::Uvlc,
        12,
        1,
    );
}

#[test]
fn av2_yuv420_nonblack_emits_lossy_residual_syntax() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format: PixelFormat::Yuv420p8,
    };
    let mut input =
        vec![0; Picture::expected_len(geometry.width, geometry.height, PixelFormat::Yuv420p8,)];
    for (index, sample) in input.iter_mut().enumerate() {
        *sample = (17 + index * 5) as u8;
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request)
        .expect("AV2 4:2:0 lossy residual encode should succeed");

    assert_ne!(
        output,
        av2_black_bitstream_for_geometry(geometry, Av2StreamFormat::yuv420_8())
    );
    assert_eq!(recon.len(), input.len());
    let trace = av2_mvp_444_trace_jsonl_for_frame(&input, request)
        .expect("AV2 4:2:0 lossy residual trace should be emitted");
    assert!(
        trace.contains("tile.coeff.y.txb_nonzero_tx4x4_ctx"),
        "non-black 4:2:0 inputs should emit residual coefficient syntax"
    );
}

#[test]
fn av2_regular_qp_intra_modes_skip_lossless_bdpcm_flags() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let format = PixelFormat::Yuv420p8;
    let bit_depth = SampleBitDepth::new(8).expect("8-bit depth is supported");
    let mut source = vec![0; Picture::expected_len(geometry.width, geometry.height, format)];
    for (index, sample) in source.iter_mut().enumerate() {
        *sample = (23 + index * 7) as u8;
    }
    let mut recon = vec![0; source.len()];
    let qp = 24;
    let payload = av2_lossy_subsampled_tile_entropy_payload_for_region_with_fields(
        Av2TileRegion::root(geometry),
        Av2Black444MvpProfile::current(),
        geometry,
        Av2ChromaFormat::Yuv420,
        bit_depth,
        &source,
        &mut recon,
        qp,
        Av2QuantizationParams::regular_qp(qp, bit_depth).base_qindex,
        true,
    );

    assert!(
        payload
            .fields
            .iter()
            .any(|field| field.name == "tile.intra.y_mode_set_index"),
        "regular-q lossy luma should start at read_intra_luma_mode syntax"
    );
    assert!(
        payload
            .fields
            .iter()
            .any(|field| field.name.starts_with("tile.intra.uv_mode_idx")),
        "regular-q lossy chroma should start at read_intra_uv_mode syntax"
    );
    assert!(
        payload
            .fields
            .iter()
            .all(|field| field.name != "tile.intra.use_dpcm_y"),
        "regular-q lossy luma must not emit lossless BDPCM syntax"
    );
    assert!(
        payload
            .fields
            .iter()
            .all(|field| field.name != "tile.intra.use_dpcm_uv"),
        "regular-q lossy chroma must not emit lossless BDPCM syntax"
    );
}

#[test]
fn av2_regular_qp_yuv444_chroma_uses_8x8_skip_syntax() {
    let geometry = Av2VideoGeometry {
        width: 16,
        height: 8,
    };
    let format = PixelFormat::Yuv444p8;
    let bit_depth = SampleBitDepth::new(8).expect("8-bit depth is supported");
    let source = vec![0; Picture::expected_len(geometry.width, geometry.height, format)];
    let mut recon = vec![0; source.len()];
    let qp = 24;
    let payload = av2_lossy_subsampled_tile_entropy_payload_for_region_with_fields(
        Av2TileRegion::root(geometry),
        Av2Black444MvpProfile::current(),
        geometry,
        Av2ChromaFormat::Yuv444,
        bit_depth,
        &source,
        &mut recon,
        qp,
        Av2QuantizationParams::regular_qp(qp, bit_depth).base_qindex,
        true,
    );

    let u_tx8x8_decisions = payload
        .fields
        .iter()
        .filter(|field| {
            field.name.starts_with("tile.coeff.u.txb_all_zero_tx8x8")
                || field.name.starts_with("tile.coeff.u.txb_nonzero_tx8x8")
        })
        .count();
    let v_tx8x8_decisions = payload
        .fields
        .iter()
        .filter(|field| {
            field
                .name
                .starts_with("tile.coeff.v.txb_all_zero_tx4x4_ctx")
                || field.name.starts_with("tile.coeff.v.txb_nonzero_tx4x4_ctx")
        })
        .count();

    assert_eq!(
        u_tx8x8_decisions, 2,
        "two 8x8 4:4:4 coding leaves should emit one U TX_8X8 decision each"
    );
    assert_eq!(
        v_tx8x8_decisions, 2,
        "two 8x8 4:4:4 coding leaves should emit one V TX_8X8 decision each"
    );
    assert!(
        payload.fields.iter().all(|field| !matches!(
            field.name,
            "tile.coeff.v.txb_all_zero_tx4x4_ctx3"
                | "tile.coeff.v.txb_nonzero_tx4x4_ctx3"
                | "tile.coeff.v.txb_all_zero_tx4x4_ctx4"
                | "tile.coeff.v.txb_nonzero_tx4x4_ctx4"
                | "tile.coeff.v.txb_all_zero_tx4x4_ctx5"
                | "tile.coeff.v.txb_nonzero_tx4x4_ctx5"
                | "tile.coeff.v.txb_all_zero_tx4x4_ctx9"
                | "tile.coeff.v.txb_nonzero_tx4x4_ctx9"
                | "tile.coeff.v.txb_all_zero_tx4x4_ctx10"
                | "tile.coeff.v.txb_nonzero_tx4x4_ctx10"
                | "tile.coeff.v.txb_all_zero_tx4x4_ctx11"
                | "tile.coeff.v.txb_nonzero_tx4x4_ctx11"
        )),
        "TX_8X8 V decisions must not use the 4x4 block-larger-than-TXB offset"
    );
}

#[test]
fn av2_qp_path_can_keep_yuv420_blocks_lossless() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let format = PixelFormat::Yuv420p8;
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format,
    };
    let mut input = vec![128; Picture::expected_len(geometry.width, geometry.height, format)];
    let y_len = geometry.width * geometry.height;
    for sample in &mut input[y_len..] {
        *sample = 129;
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: false,
            qp: Some(8),
            gop: crate::settings::GopMode::IntraOnly,
        },
        None,
    )
    .expect("AV2 QP residual path should encode predictor-matched blocks");

    assert!(!output.is_empty());
    assert_eq!(recon, input);
}

#[test]
fn av2_qp_path_accepts_yuv422() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let format = PixelFormat::yuv422(8).expect("valid 8-bit 4:2:2 format");
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format,
    };
    let mut input = vec![128; Picture::expected_len(geometry.width, geometry.height, format)];
    let y_len = geometry.width * geometry.height;
    for sample in &mut input[y_len..] {
        *sample = 129;
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: false,
            qp: Some(8),
            gop: crate::settings::GopMode::IntraOnly,
        },
        None,
    )
    .expect("AV2 QP residual path should encode yuv422");

    assert!(!output.is_empty());
    assert_eq!(recon, input);
}

#[test]
fn av2_yuv420_accepts_high_bit_depth_without_downscaling() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    for bits in [10] {
        let format = PixelFormat::yuv420(bits).expect("valid AV2 high-depth 4:2:0 format");
        let request = Av2EncodeRequest {
            params: Av2EncodeParams { frames: 1 },
            geometry,
            format,
        };
        let sample_count = Picture::expected_len(geometry.width, geometry.height, format)
            / format.bytes_per_sample();
        let max_sample = format.bit_depth().max_sample();
        let mut input = vec![0; Picture::expected_len(geometry.width, geometry.height, format)];
        for sample_index in 0..sample_count {
            framefinery_api::write_planar_sample(
                &mut input,
                sample_index,
                max_sample,
                format.bit_depth(),
            )
            .expect("write high-depth 4:2:0 sample");
        }
        let mut source = input.as_slice();
        let mut output = Vec::new();
        let mut recon = Vec::new();

        av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request)
            .expect("AV2 high-depth 4:2:0 lossy residual encode should succeed");

        assert!(!output.is_empty());
        assert_eq!(recon.len(), input.len());
        assert!(
            framefinery_api::read_planar_sample(&recon, 0, format.bit_depth())
                .expect("read reconstructed sample")
                > u16::from(u8::MAX),
            "high-depth 4:2:0 reconstruction should not be downscaled to 8-bit"
        );
        let stream_format =
            Av2StreamFormat::from_pixel_format(format).expect("valid AV2 stream format");
        let sequence = av2_mvp_sequence_header_payload(
            geometry,
            Av2Black444MvpProfile::current(),
            stream_format,
        );
        assert_has_field_with_bit_count(
            &sequence,
            "sequence_header.bitdepth_lut_idx",
            Av2SyntaxCode::Uvlc,
            expected_uvlc_bit_count(stream_format.bitdepth_lut_index()),
        );
    }
}

#[test]
fn av2_yuv420_lossless_preserves_high_bit_depth_samples() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let format = PixelFormat::yuv420(10).expect("valid AV2 high-depth 4:2:0 format");
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format,
    };
    let sample_count =
        Picture::expected_len(geometry.width, geometry.height, format) / format.bytes_per_sample();
    let max_sample = format.bit_depth().max_sample();
    let mut input = vec![0; Picture::expected_len(geometry.width, geometry.height, format)];
    for sample_index in 0..sample_count {
        let sample = ((sample_index * 37 + 11) as u16) & max_sample;
        framefinery_api::write_planar_sample(&mut input, sample_index, sample, format.bit_depth())
            .expect("write high-depth 4:2:0 lossless sample");
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: true,
            ..Default::default()
        },
        None,
    )
    .expect("AV2 lossless 4:2:0 should encode stream-exact");

    assert!(!output.is_empty());
    assert_eq!(recon, input);
}

#[test]
fn av2_yuv420_lossless_fast_path_writes_reconstruction() {
    let geometry = Av2VideoGeometry {
        width: 128,
        height: 128,
    };
    let format = PixelFormat::Yuv420p8;
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format,
    };
    let mut input = vec![0; Picture::expected_len(geometry.width, geometry.height, format)];
    for (index, sample) in input.iter_mut().enumerate() {
        *sample = ((index * 37 + index / 11 + 23) & 0xff) as u8;
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: true,
            ..Default::default()
        },
        None,
    )
    .expect("AV2 4:2:0 fast lossless path should encode stream-exact");

    assert!(!output.is_empty());
    assert_eq!(recon, input);
}

#[test]
fn av2_yuv422_lossless_preserves_high_bit_depth_samples() {
    let geometry = Av2VideoGeometry {
        width: 8,
        height: 8,
    };
    let format = PixelFormat::yuv422(10).expect("valid AV2 high-depth 4:2:2 format");
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry,
        format,
    };
    let sample_count =
        Picture::expected_len(geometry.width, geometry.height, format) / format.bytes_per_sample();
    let max_sample = format.bit_depth().max_sample();
    let mut input = vec![0; Picture::expected_len(geometry.width, geometry.height, format)];
    for sample_index in 0..sample_count {
        let sample = ((sample_index * 53 + 7) as u16) & max_sample;
        framefinery_api::write_planar_sample(&mut input, sample_index, sample, format.bit_depth())
            .expect("write high-depth 4:2:2 lossless sample");
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        &mut source,
        &mut output,
        Some(&mut recon),
        request,
        Av2EncodeOptions {
            lossless: true,
            ..Default::default()
        },
        None,
    )
    .expect("AV2 lossless 4:2:2 should encode stream-exact");

    assert!(!output.is_empty());
    assert_eq!(recon, input);
    let stream_format =
        Av2StreamFormat::from_pixel_format(format).expect("valid AV2 stream format");
    let sequence =
        av2_mvp_sequence_header_payload(geometry, Av2Black444MvpProfile::current(), stream_format);
    assert_has_field(
        &sequence,
        "sequence_header.seq_chroma_format_idc",
        Av2SyntaxCode::Uvlc,
        12,
        expected_uvlc_bit_count(stream_format.chroma_format.sequence_header_idc()),
    );
}

#[test]
fn av2_fixed_black_444_sequence_header_has_labeled_fields() {
    let payload = av2_black_444_sequence_header_payload(Av2VideoGeometry {
        width: 64,
        height: 64,
    });

    assert_eq!(
        payload.bytes,
        vec![0x92, 0x06, 0x95, 0x7f, 0xfc, 0x00, 0x01, 0x12, 0x0d, 0xc0, 0x44,]
    );
    assert_has_field(
        &payload,
        "sequence_header.seq_profile_idc",
        Av2SyntaxCode::Literal,
        1,
        5,
    );
    assert_has_field(
        &payload,
        "sequence_header.max_frame_width_minus_1",
        Av2SyntaxCode::Literal,
        26,
        6,
    );
    assert_has_field(
        &payload,
        "sequence_transform.enable_chroma_dctonly",
        Av2SyntaxCode::Flag,
        62,
        1,
    );
    assert_has_field(
        &payload,
        "sequence_transform.base_uv_ac_delta_q_minus_min",
        Av2SyntaxCode::Literal,
        69,
        5,
    );
    assert_has_field(
        &payload,
        "trailing_bits",
        Av2SyntaxCode::TrailingBits,
        85,
        3,
    );
}

#[test]
fn av2_fixed_black_444_closed_loop_key_labels_header_fields() {
    let payload = av2_black_444_closed_loop_key_header_payload();

    assert_eq!(payload.bytes, vec![0xe2, 0x00, 0x00]);
    assert_has_field(
        &payload,
        "tile_group.first_tile_group_in_frame",
        Av2SyntaxCode::Flag,
        0,
        1,
    );
    assert_has_field(
        &payload,
        "quantization.base_qindex",
        Av2SyntaxCode::Literal,
        7,
        8,
    );
}

#[test]
fn av2_lossless_header_stays_coded_lossless_compatible() {
    let tile_layout = Av2TileLayout::for_geometry(Av2VideoGeometry {
        width: 64,
        height: 64,
    });
    let payload = av2_mvp_444_closed_loop_key_header_payload(
        false,
        false,
        &tile_layout,
        Av2StreamFormat::yuv420_8(),
        Av2QuantizationParams::lossless(),
    );

    assert_has_field_with_bit_count(
        &payload,
        "quantization.base_qindex",
        Av2SyntaxCode::Literal,
        8,
    );
    assert_no_field(&payload, "delta_q.present");
    assert_no_field(&payload, "loop_filter.apply_deblocking_filter_y_vertical");
    assert_no_field(&payload, "uncompressed_header.tx_mode_select");
}

#[test]
fn av2_regular_qp_header_can_signal_qindex_and_disabled_delta_q() {
    let tile_layout = Av2TileLayout::for_geometry(Av2VideoGeometry {
        width: 64,
        height: 64,
    });
    let bit_depth = SampleBitDepth::new(10).expect("10-bit depth is supported");
    let quantization = Av2QuantizationParams::regular_qp(24, bit_depth);
    assert_eq!(quantization.base_qindex, 80);
    let payload = av2_mvp_444_closed_loop_key_header_payload(
        false,
        false,
        &tile_layout,
        Av2StreamFormat {
            chroma_format: Av2ChromaFormat::Yuv420,
            bit_depth,
        },
        quantization,
    );

    assert_has_field_with_bit_count(
        &payload,
        "quantization.base_qindex",
        Av2SyntaxCode::Literal,
        9,
    );
    assert_has_field_with_bit_count(&payload, "delta_q.present", Av2SyntaxCode::Flag, 1);
    assert_has_field_with_bit_count(
        &payload,
        "loop_filter.apply_deblocking_filter_y_vertical",
        Av2SyntaxCode::Flag,
        1,
    );
    assert_has_field_with_bit_count(
        &payload,
        "loop_filter.apply_deblocking_filter_y_horizontal",
        Av2SyntaxCode::Flag,
        1,
    );
    assert_has_field_with_bit_count(
        &payload,
        "uncompressed_header.tx_mode_select",
        Av2SyntaxCode::Flag,
        1,
    );
    assert_no_field(&payload, "delta_q.resolution_log2");
}

#[test]
fn av2_fixed_black_444_closed_loop_key_carries_generated_tile_entropy_payload() {
    let payload = av2_black_444_closed_loop_key_payload(Av2VideoGeometry {
        width: 64,
        height: 64,
    });

    assert_eq!(&payload.bytes[..3], &[0xf1, 0x00, 0x00]);
    assert!(payload.bytes.len() > 3);
    let entropy_field = payload
        .fields
        .iter()
        .find(|field| field.name == "tile_group.tile_entropy_payload")
        .expect("missing AV2 tile entropy payload field");
    assert_eq!(entropy_field.code, Av2SyntaxCode::TileEntropyPayload);
    assert_eq!(entropy_field.bit_offset, 24);
    assert_eq!(entropy_field.bit_count, (payload.bytes.len() - 3) * 8);
}

#[test]
fn av2_luma_palette_444_accepts_two_luma_colors_with_zero_chroma() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry: Av2VideoGeometry {
            width: AV2_FIXED_BLACK_444_WIDTH,
            height: AV2_FIXED_BLACK_444_HEIGHT,
        },
        format: PixelFormat::Yuv444p8,
    };
    let mut input = av2_black_64x64_444_reconstruction();
    let y_plane_len = AV2_FIXED_BLACK_444_WIDTH * AV2_FIXED_BLACK_444_HEIGHT;
    for sample in &mut input[y_plane_len / 2..y_plane_len] {
        *sample = 96;
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    let result = av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request);

    result.expect("two-color luma palette should encode");
    assert_ne!(
        output,
        av2_black_444_bitstream_for_geometry(request.geometry)
    );
    assert_eq!(recon, input);
}

#[test]
fn av2_mvp_444_preserves_chroma_with_bdpcm_residuals() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry: Av2VideoGeometry {
            width: AV2_FIXED_BLACK_444_WIDTH,
            height: AV2_FIXED_BLACK_444_HEIGHT,
        },
        format: PixelFormat::Yuv444p8,
    };
    let mut input = av2_black_64x64_444_reconstruction();
    let y_plane_len = AV2_FIXED_BLACK_444_WIDTH * AV2_FIXED_BLACK_444_HEIGHT;
    input[y_plane_len] = 1;
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    let result = av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request);

    result.expect("content must not be rejected by the AV2 MVP path");
    assert_ne!(
        output,
        av2_black_444_bitstream_for_geometry(request.geometry)
    );
    assert_eq!(recon, input);
}

#[test]
fn av2_mvp_444_can_select_vertical_chroma_bdpcm() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry: Av2VideoGeometry {
            width: 8,
            height: 16,
        },
        format: PixelFormat::Yuv444p8,
    };
    let plane_len = request.geometry.width * request.geometry.height;
    let mut input = vec![0u8; plane_len * 3];
    for y in 0..16usize {
        for x in 0..8usize {
            let index = y * 8 + x;
            // Keep the two 8x8 blocks from becoming an IntraBC copy while
            // preserving the chroma edge that vertical DPCM can reuse.
            input[index] = if y < 8 { 0 } else { 1 };
            input[plane_len + index] = 127 + (x as u8 * 7);
            input[2 * plane_len + index] = 127 + (x as u8 * 7);
        }
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request)
        .expect("vertical chroma BDPCM should encode");

    assert_eq!(recon, input);
    let trace =
        av2_mvp_444_trace_jsonl_for_frame(&input, request).expect("AV2 trace should be emitted");
    assert!(
        trace.lines().any(|line| {
            line.contains("\"name\":\"tile.intra.dpcm_uv_horz\"") && line.contains("\"symbol\":0")
        }),
        "vertical chroma BDPCM should signal dpcm_uv_horz=0"
    );
}

#[test]
fn av2_mvp_444_preserves_over_limit_luma_colors_with_lossless_residual() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry: Av2VideoGeometry {
            width: AV2_FIXED_BLACK_444_WIDTH,
            height: AV2_FIXED_BLACK_444_HEIGHT,
        },
        format: PixelFormat::Yuv444p8,
    };
    let mut input = av2_black_64x64_444_reconstruction();
    let y_plane_len = AV2_FIXED_BLACK_444_WIDTH * AV2_FIXED_BLACK_444_HEIGHT;
    for (index, sample) in input[..y_plane_len].iter_mut().enumerate() {
        *sample = (index & 0xff) as u8;
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    let result = av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request);

    result.expect("over-limit luma colors should encode through lossless residuals");
    assert_ne!(
        output,
        av2_black_444_bitstream_for_geometry(request.geometry)
    );
    assert_eq!(recon, input);
    let trace =
        av2_mvp_444_trace_jsonl_for_frame(&input, request).expect("AV2 trace should be emitted");
    assert!(
        trace.contains("tile.coeff.y.idtx_base")
            || trace.contains("tile.coeff.y.txb_nonzero_tx4x4_ctx"),
        "over-limit luma palette blocks must emit lossless luma coefficient residuals"
    );
    assert!(recon[y_plane_len..].iter().all(|&sample| sample == 0));
}

#[test]
fn av2_mvp_444_can_select_horizontal_luma_dpcm_prediction() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 1 },
        geometry: Av2VideoGeometry {
            width: 16,
            height: 8,
        },
        format: PixelFormat::Yuv444p8,
    };
    let mut input = vec![0u8; 16 * 8 * 3];
    for y in 0..8usize {
        let edge = 16 + y as u8 * 28;
        input[y * 16 + 7] = edge;
        for x in 0..8usize {
            input[y * 16 + 8 + x] = if x < 3 { edge } else { edge + 20 };
        }
    }
    let mut source = input.as_slice();
    let mut output = Vec::new();
    let mut recon = Vec::new();

    av2_encode_fixed_black_444(&mut source, &mut output, Some(&mut recon), request)
        .expect("horizontal intra luma prediction should encode");

    assert_eq!(recon, input);
    let trace =
        av2_mvp_444_trace_jsonl_for_frame(&input, request).expect("AV2 trace should be emitted");
    assert!(
        trace
            .lines()
            .any(|line| line.contains("\"name\":\"tile.intra.use_dpcm_y\"")
                && line.contains("\"symbol\":1")),
        "lossless luma DPCM should be selected for the right block"
    );
    assert!(
        trace
            .lines()
            .any(|line| line.contains("\"name\":\"tile.intra.dpcm_y_horz\"")
                && line.contains("\"symbol\":1")),
        "horizontal luma DPCM should be selected for the right block"
    );
}

#[test]
fn av2_accepts_zero_frames_as_unbounded_input() {
    let request = Av2EncodeRequest {
        params: Av2EncodeParams { frames: 0 },
        geometry: Av2VideoGeometry {
            width: 64,
            height: 64,
        },
        format: PixelFormat::Yuv420p8,
    };

    assert!(request.validate().is_ok());
}

#[test]
fn av2_closed_loop_key_uses_variable_leb_for_large_payloads() {
    assert_eq!(leb128_len((1 << 21) - 1), 3);
    assert_eq!(leb128_len(1 << 21), 4);

    let mut out = Vec::new();
    write_leb128(1 << 21, &mut out);
    assert_eq!(out, [0x80, 0x80, 0x80, 0x01]);
}

fn assert_has_field(
    payload: &Av2SyntaxPayload,
    name: &'static str,
    code: Av2SyntaxCode,
    bit_offset: usize,
    bit_count: usize,
) {
    assert!(
        payload.fields.iter().any(|field| {
            field.name == name
                && field.code == code
                && field.bit_offset == bit_offset
                && field.bit_count == bit_count
        }),
        "missing AV2 syntax field {name} at bit {bit_offset} with {bit_count} bit(s)"
    );
}

fn assert_has_field_with_bit_count(
    payload: &Av2SyntaxPayload,
    name: &'static str,
    code: Av2SyntaxCode,
    bit_count: usize,
) {
    assert!(
        payload.fields.iter().any(|field| {
            field.name == name && field.code == code && field.bit_count == bit_count
        }),
        "missing AV2 syntax field {name} with {bit_count} bit(s)"
    );
}

fn assert_no_field(payload: &Av2SyntaxPayload, name: &'static str) {
    assert!(
        payload.fields.iter().all(|field| field.name != name),
        "unexpected AV2 syntax field {name}"
    );
}

fn expected_uvlc_bit_count(value: u32) -> usize {
    let code_num = value + 1;
    let bits = 32 - code_num.leading_zeros();
    (bits * 2 - 1) as usize
}

fn supported_black_444_geometries() -> Vec<Av2VideoGeometry> {
    let mut geometries = Vec::new();
    for height in (8..=64).step_by(8) {
        for width in (8..=64).step_by(8) {
            geometries.push(Av2VideoGeometry { width, height });
        }
    }
    geometries
}
