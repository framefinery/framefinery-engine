use std::borrow::Cow;

#[cfg(any(test, feature = "bench-internals"))]
pub fn av2_encode_fixed_black_444(
    input: &mut dyn Read,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: Av2EncodeRequest,
) -> Result<(), String> {
    av2_encode_fixed_black_444_with_frame_metrics(input, output, recon, request, None)
}

#[cfg(any(test, feature = "bench-internals"))]
pub fn av2_encode_fixed_black_444_with_frame_metrics(
    input: &mut dyn Read,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: Av2EncodeRequest,
    frame_metrics: Option<&mut dyn for<'a> FnMut(Av2EncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    av2_encode_fixed_black_444_with_options_and_frame_metrics(
        input,
        output,
        recon,
        request,
        Av2EncodeOptions {
            gop: crate::settings::GopMode::IntraOnly,
            ..Default::default()
        },
        frame_metrics,
    )
}

pub fn av2_encode_fixed_black_444_with_options_and_frame_metrics(
    input: &mut dyn Read,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: Av2EncodeRequest,
    options: Av2EncodeOptions,
    frame_metrics: Option<&mut dyn for<'a> FnMut(Av2EncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    let mut encoder = Av2StreamEncoder::new(request, options)?;
    let frame_limit = FrameLimit::from_frame_count(request.params.frames);
    let mut source_frame = vec![0; encoder.source_expected_len];
    let mut sinks = Av2FrameSinks {
        output,
        recon,
        frame_metrics,
    };
    while frame_limit.should_read(encoder.frame_index) {
        let mut frame_stats = encoder.frame_stats();
        let stage_start = stats::Av2StageStart::now();
        let frame_was_read = read_input_frame(
            input,
            &mut source_frame,
            encoder.frame_index,
            frame_limit,
            "AV2 MVP input",
        )?;
        frame_stats.add_elapsed("read_frame", stage_start);
        if !frame_was_read {
            break;
        }
        encoder.encode_frame(&source_frame, &mut sinks, frame_stats)?;
    }
    encoder.finish()
}

fn av2_public_reconstruction<'a>(
    reconstruction: &'a [u8],
    coded_geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    stream_pixel_format: PixelFormat,
    packed_rgb_identity: bool,
) -> Cow<'a, [u8]> {
    let visible_planar = if coded_geometry != visible_geometry {
        crop_av2_frame_to_geometry(
            reconstruction,
            coded_geometry,
            visible_geometry,
            stream_pixel_format,
        )
    } else if packed_rgb_identity {
        Vec::new()
    } else {
        return Cow::Borrowed(reconstruction);
    };

    if packed_rgb_identity {
        let planar = if coded_geometry != visible_geometry {
            visible_planar.as_slice()
        } else {
            reconstruction
        };
        Cow::Owned(planar_gbr_to_rgb24(planar, visible_geometry))
    } else {
        Cow::Owned(visible_planar)
    }
}

#[cfg(test)]
fn av2_black_444_bitstream_for_geometry(geometry: Av2VideoGeometry) -> Vec<u8> {
    av2_black_bitstream_for_geometry(geometry, Av2StreamFormat::yuv444_8())
}

#[cfg(test)]
fn av2_black_bitstream_for_geometry(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
) -> Vec<u8> {
    let mut out = Vec::new();
    let profile = Av2Black444MvpProfile::current();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(
        &mut out,
        Av2ObuType::SequenceHeader,
        &av2_mvp_sequence_header_payload(geometry, profile, stream_format),
    );
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &av2_black_closed_loop_key_payload(geometry, stream_format.chroma_format),
    );
    out
}

fn av2_lossy_subsampled_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    qp: u8,
    rgb_identity: bool,
    frame_stats: &mut stats::Av2FrameStats,
) -> (Vec<u8>, Vec<u8>) {
    assert!(qp > 0, "AV2 lossy QP must be non-zero");
    let expected_len = Picture::expected_len(
        geometry.width,
        geometry.height,
        stream_format.pixel_format(),
    );
    assert_eq!(
        frame.len(),
        expected_len,
        "AV2 planar lossy input length must match geometry"
    );
    let mut reconstruction = vec![0; expected_len];
    let mut out = Vec::new();
    let stage_start = stats::Av2StageStart::now();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(
        &mut out,
        Av2ObuType::SequenceHeader,
        &av2_mvp_sequence_header_payload_for_visible(
            geometry,
            visible_geometry,
            Av2Black444MvpProfile::current(),
            stream_format,
        ),
    );
    append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    frame_stats.add_elapsed("lossy_headers", stage_start);
    let stage_start = stats::Av2StageStart::now();
    let payload = av2_lossy_subsampled_closed_loop_key_payload(
        geometry,
        stream_format,
        frame,
        &mut reconstruction,
        qp,
    );
    frame_stats.add_elapsed("lossy_tile_payload", stage_start);
    let stage_start = stats::Av2StageStart::now();
    append_obu(&mut out, Av2ObuType::ClosedLoopKey, &payload);
    frame_stats.add_elapsed("lossy_entropy_pack", stage_start);
    (out, reconstruction)
}

fn av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    qp: u8,
    include_sequence_header: bool,
    order_hint: u16,
    rgb_identity: bool,
    frame_stats: &mut stats::Av2FrameStats,
) -> (Vec<u8>, Vec<u8>) {
    assert!(qp > 0, "AV2 lossy QP must be non-zero");
    let expected_len = Picture::expected_len(
        geometry.width,
        geometry.height,
        stream_format.pixel_format(),
    );
    assert_eq!(
        frame.len(),
        expected_len,
        "AV2 predictive planar lossy input length must match geometry"
    );
    let mut reconstruction = vec![0; expected_len];
    let mut out = Vec::new();
    let stage_start = stats::Av2StageStart::now();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    if include_sequence_header {
        append_obu(
            &mut out,
            Av2ObuType::SequenceHeader,
            &av2_mvp_predictive_sequence_header_payload_for_visible(
                geometry,
                visible_geometry,
                Av2Black444MvpProfile::current(),
                stream_format,
            ),
        );
        append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    }
    frame_stats.add_elapsed("lossy_predictive_headers", stage_start);
    let stage_start = stats::Av2StageStart::now();
    let payload = av2_lossy_subsampled_predictive_closed_loop_key_payload(
        geometry,
        stream_format,
        frame,
        &mut reconstruction,
        qp,
        order_hint,
    );
    frame_stats.add_elapsed("lossy_tile_payload", stage_start);
    let stage_start = stats::Av2StageStart::now();
    append_obu(&mut out, Av2ObuType::ClosedLoopKey, &payload);
    frame_stats.add_elapsed("lossy_entropy_pack", stage_start);
    (out, reconstruction)
}

fn av2_lossless_subsampled_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    rgb_identity: bool,
    frame_stats: &mut stats::Av2FrameStats,
) -> (Vec<u8>, Vec<u8>) {
    debug_assert!(matches!(
        stream_format.chroma_format,
        Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv422 | Av2ChromaFormat::Yuv444
    ));
    let expected_len = Picture::expected_len(
        geometry.width,
        geometry.height,
        stream_format.pixel_format(),
    );
    assert_eq!(
        frame.len(),
        expected_len,
        "AV2 planar lossless input length must match geometry"
    );
    let tile_layout = Av2TileLayout::lossless_subsampled_ibc_for_geometry(geometry);
    let ibc_tile_bounds = tile_layout.local_ibc_tile_bounds();
    let ibc = if AV2_ENABLE_LOSSLESS_SUBSAMPLED_IBC {
        let stage_start = stats::Av2StageStart::now();
        let ibc = ibc::build_local_ibc_subsampled(
            frame,
            geometry,
            stream_format.chroma_format,
            stream_format.bit_depth,
            &ibc_tile_bounds,
        )
        .ok()
        .filter(|ibc| ibc.stats().selected_copy_blocks() > 0);
        frame_stats.add_elapsed("lossless_ibc_search", stage_start);
        ibc
    } else {
        None
    };
    let profile = if ibc.is_some() {
        Av2Black444MvpProfile::current().with_local_ibc_candidates()
    } else {
        Av2Black444MvpProfile::current()
    };
    let stage_start = stats::Av2StageStart::now();
    let palette = palette::build_luma_palette_lossless(
        frame,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .ok();
    frame_stats.add_elapsed("lossless_palette_build", stage_start);
    let mut reconstruction = vec![0; expected_len];
    let mut out = Vec::new();
    let stage_start = stats::Av2StageStart::now();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(
        &mut out,
        Av2ObuType::SequenceHeader,
        &av2_mvp_sequence_header_payload_for_visible(
            geometry,
            visible_geometry,
            profile,
            stream_format,
        ),
    );
    append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    frame_stats.add_elapsed("lossless_headers", stage_start);
    let stage_start = stats::Av2StageStart::now();
    let payload = av2_lossless_subsampled_closed_loop_key_payload(
        geometry,
        stream_format,
        frame,
        &mut reconstruction,
        profile,
        palette.as_ref(),
        ibc.as_ref(),
    );
    frame_stats.add_elapsed("lossless_tile_payload", stage_start);
    let stage_start = stats::Av2StageStart::now();
    append_obu(&mut out, Av2ObuType::ClosedLoopKey, &payload);
    frame_stats.add_elapsed("lossless_entropy_pack", stage_start);
    (out, reconstruction)
}

fn av2_lossless_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    include_sequence_header: bool,
    order_hint: u16,
    rgb_identity: bool,
    frame_stats: &mut stats::Av2FrameStats,
) -> (Vec<u8>, Vec<u8>) {
    debug_assert!(matches!(
        stream_format.chroma_format,
        Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv422 | Av2ChromaFormat::Yuv444
    ));
    let expected_len = Picture::expected_len(
        geometry.width,
        geometry.height,
        stream_format.pixel_format(),
    );
    assert_eq!(
        frame.len(),
        expected_len,
        "AV2 predictive lossless input length must match geometry"
    );
    let tile_layout = Av2TileLayout::lossless_subsampled_ibc_for_geometry(geometry);
    let ibc_tile_bounds = tile_layout.local_ibc_tile_bounds();
    let ibc = if AV2_ENABLE_LOSSLESS_SUBSAMPLED_IBC {
        let stage_start = stats::Av2StageStart::now();
        let ibc = ibc::build_local_ibc_subsampled(
            frame,
            geometry,
            stream_format.chroma_format,
            stream_format.bit_depth,
            &ibc_tile_bounds,
        )
        .ok()
        .filter(|ibc| ibc.stats().selected_copy_blocks() > 0);
        frame_stats.add_elapsed("lossless_ibc_search", stage_start);
        ibc
    } else {
        None
    };
    // The sequence header fixes BVP capacity and disables frame-level
    // overrides. Later pictures may find IntraBC matches even when the first
    // picture did not, so advertise the enabled tool's capacity consistently.
    // Actual IntraBC selection still follows this picture's search results.
    let profile = if AV2_ENABLE_LOSSLESS_SUBSAMPLED_IBC {
        Av2Black444MvpProfile::current().with_local_ibc_candidates()
    } else {
        Av2Black444MvpProfile::current()
    };
    let stage_start = stats::Av2StageStart::now();
    let palette = palette::build_luma_palette_lossless(
        frame,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .ok();
    frame_stats.add_elapsed("lossless_palette_build", stage_start);
    let mut reconstruction = vec![0; expected_len];
    let mut out = Vec::new();
    let stage_start = stats::Av2StageStart::now();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    if include_sequence_header {
        append_obu(
            &mut out,
            Av2ObuType::SequenceHeader,
            &av2_mvp_predictive_sequence_header_payload_for_visible(
                geometry,
                visible_geometry,
                profile,
                stream_format,
            ),
        );
        append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    }
    frame_stats.add_elapsed("lossless_predictive_headers", stage_start);
    let stage_start = stats::Av2StageStart::now();
    let payload = av2_lossless_subsampled_predictive_closed_loop_key_payload(
        geometry,
        stream_format,
        frame,
        &mut reconstruction,
        profile,
        palette.as_ref(),
        ibc.as_ref(),
        order_hint,
    );
    frame_stats.add_elapsed("lossless_tile_payload", stage_start);
    let stage_start = stats::Av2StageStart::now();
    append_obu(&mut out, Av2ObuType::ClosedLoopKey, &payload);
    frame_stats.add_elapsed("lossless_entropy_pack", stage_start);
    (out, reconstruction)
}

fn av2_lossless_regular_inter_tiles_frame(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reference: &[u8],
    order_hint: u16,
) -> Option<(Vec<u8>, Vec<u8>)> {
    av2_lossless_subsampled_regular_inter_tiles_bitstream_and_reconstruction_for_frame(
        geometry,
        stream_format,
        frame,
        reference,
        order_hint,
    )
}

fn av2_lossy_zero_mv_inter_tiles_frame(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reference_source: &[u8],
    reference_reconstruction: &[u8],
    qp: u8,
    order_hint: u16,
) -> Option<(Vec<u8>, Vec<u8>)> {
    av2_lossy_subsampled_zero_mv_inter_tiles_bitstream_and_reconstruction_for_frame(
        geometry,
        stream_format,
        frame,
        reference_source,
        reference_reconstruction,
        qp,
        order_hint,
    )
}

fn av2_lossless_regular_sef_frame(frame: &[u8], order_hint: u16) -> (Vec<u8>, Vec<u8>) {
    let mut out = Vec::new();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(
        &mut out,
        Av2ObuType::RegularSef,
        &av2_regular_sef_payload(order_hint),
    );
    (out, frame.to_vec())
}

fn av2_lossy_regular_sef_frame(
    reference_reconstruction: &[u8],
    order_hint: u16,
) -> (Vec<u8>, Vec<u8>) {
    let mut out = Vec::new();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(
        &mut out,
        Av2ObuType::RegularSef,
        &av2_regular_sef_payload(order_hint),
    );
    (out, reference_reconstruction.to_vec())
}

fn av2_mvp_444_bitstream_for_mode(
    geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
    frame_mode: &Av2Mvp444FrameMode,
    rgb_identity: bool,
) -> Vec<u8> {
    let mut out = Vec::new();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(
        &mut out,
        Av2ObuType::SequenceHeader,
        &av2_mvp_444_sequence_header_payload_for_visible(
            geometry,
            visible_geometry,
            bit_depth,
            frame_mode.profile(),
        ),
    );
    append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &av2_mvp_444_closed_loop_key_payload(geometry, bit_depth, frame_mode),
    );
    out
}
