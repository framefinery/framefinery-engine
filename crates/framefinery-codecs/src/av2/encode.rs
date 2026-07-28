pub fn av2_encode_fixed_black_444(
    input: &mut dyn Read,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    request: Av2EncodeRequest,
) -> Result<(), String> {
    av2_encode_fixed_black_444_with_frame_metrics(input, output, recon, request, None)
}

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
        Av2EncodeOptions::default(),
        frame_metrics,
    )
}

pub fn av2_encode_fixed_black_444_with_options_and_frame_metrics(
    input: &mut dyn Read,
    output: &mut dyn Write,
    mut recon: Option<&mut dyn Write>,
    request: Av2EncodeRequest,
    options: Av2EncodeOptions,
    mut frame_metrics: Option<&mut dyn for<'a> FnMut(Av2EncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    let geometry = validate_mvp_request(request)?;
    let stream_format = Av2StreamFormat::from_pixel_format(request.format)
        .expect("validate_mvp_request accepts only supported AV2 stream formats");
    let rgb_identity = request.format.is_rgb();
    let packed_rgb_identity = request.format == PixelFormat::Rgb24;

    let source_expected_len =
        Picture::expected_len(geometry.width, geometry.height, request.format);
    let coded_expected_len = Picture::expected_len(
        geometry.width,
        geometry.height,
        stream_format.pixel_format(),
    );
    debug_assert_eq!(source_expected_len, coded_expected_len);
    let mut predictive_started = false;
    let mut predictive_reference: Option<Vec<u8>> = None;
    let mut predictive_reconstruction: Option<Vec<u8>> = None;
    let frame_limit = FrameLimit::from_frame_count(request.params.frames);
    let mut source_frame = vec![0; source_expected_len];
    let mut frame_index = 0usize;
    while frame_limit.should_read(frame_index) {
        #[cfg(feature = "av2-sb-bit-profile")]
        sb_bits::set_current_frame(frame_index);
        if !read_input_frame(
            input,
            &mut source_frame,
            frame_index,
            frame_limit,
            "AV2 MVP input",
        )? {
            break;
        }
        let coded_frame: Vec<u8>;
        let frame = if packed_rgb_identity {
            coded_frame = rgb24_to_planar_gbr(&source_frame, geometry);
            coded_frame.as_slice()
        } else {
            source_frame.as_slice()
        };
        // The MVP stream keeps each input picture independently decodable.
        // Concatenating one single-picture OBU sequence per frame avoids
        // hidden single-frame tooling assumptions while inter-frame AV2 syntax
        // is still being built out.
        if options.lossless
            && matches!(
                stream_format.chroma_format,
                Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv422 | Av2ChromaFormat::Yuv444
            )
        {
            let (bitstream, reconstruction) = if options.predictive {
                let order_hint = av2_order_hint_for_frame(frame_index);
                if predictive_reference.as_deref() == Some(frame) {
                    av2_lossless_subsampled_regular_sef_bitstream_and_reconstruction_for_frame(
                        frame, order_hint,
                    )
                } else if let Some((bitstream, reconstruction)) = predictive_reference
                    .as_deref()
                    .and_then(|reference| {
                        av2_lossless_subsampled_regular_inter_tiles_bitstream_and_reconstruction_for_frame(
                            geometry,
                            stream_format,
                            frame,
                            reference,
                            order_hint,
                        )
                    })
                {
                    predictive_reference = Some(frame.to_vec());
                    (bitstream, reconstruction)
                } else {
                    let result =
                        av2_lossless_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            geometry,
                            stream_format,
                            frame,
                            !predictive_started,
                            order_hint,
                            rgb_identity,
                        );
                    predictive_started = true;
                    predictive_reference = Some(frame.to_vec());
                    result
                }
            } else {
                av2_lossless_subsampled_bitstream_and_reconstruction_for_frame(
                    geometry,
                    stream_format,
                    frame,
                    rgb_identity,
                )
            };
            output
                .write_all(&bitstream)
                .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
            let public_reconstruction: Vec<u8>;
            let reconstruction = if packed_rgb_identity {
                public_reconstruction = planar_gbr_to_rgb24(&reconstruction, geometry);
                public_reconstruction.as_slice()
            } else {
                reconstruction.as_slice()
            };
            if let Some(recon) = recon.as_deref_mut() {
                recon
                    .write_all(reconstruction)
                    .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
            }
            if let Some(frame_metrics) = frame_metrics.as_deref_mut() {
                frame_metrics(Av2EncodeFrameMetrics {
                    frame_idx: frame_index,
                    frame_count: frame_limit.metric_count(),
                    bitstream_bytes: bitstream.len(),
                    source: &source_frame,
                    reconstruction,
                });
            }
            frame_index += 1;
            continue;
        }
        if stream_format.chroma_format == Av2ChromaFormat::Yuv422 && options.qp.is_none() {
            return Err(format!(
                "AV2 non-lossless encode is not implemented for {}; pass --qp to use the experimental lossy residual path",
                request.format
            ));
        }
        let use_lossy_residual_path =
            options.qp.is_some() || stream_format.chroma_format == Av2ChromaFormat::Yuv420;
        if use_lossy_residual_path {
            let qp = options.qp.unwrap_or(AV2_LOSSY_DEFAULT_QP);
            let (bitstream, reconstruction) = if options.predictive {
                let order_hint = av2_order_hint_for_frame(frame_index);
                if predictive_reference.as_deref() == Some(frame) {
                    if let Some(reference_reconstruction) = predictive_reconstruction.as_deref() {
                        av2_lossy_subsampled_regular_sef_bitstream_and_reconstruction_for_frame(
                            reference_reconstruction,
                            order_hint,
                        )
                    } else {
                        av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            geometry,
                            stream_format,
                            frame,
                            qp,
                            !predictive_started,
                            order_hint,
                            rgb_identity,
                        )
                    }
                } else {
                    if let (Some(reference), Some(reference_reconstruction)) = (
                        predictive_reference.as_deref(),
                        predictive_reconstruction.as_deref(),
                    ) {
                        av2_lossy_subsampled_zero_mv_inter_tiles_bitstream_and_reconstruction_for_frame(
                            geometry,
                            stream_format,
                            frame,
                            reference,
                            reference_reconstruction,
                            qp,
                            order_hint,
                        )
                        .unwrap_or_else(|| {
                            av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                                geometry,
                                stream_format,
                                frame,
                                qp,
                                !predictive_started,
                                order_hint,
                                rgb_identity,
                            )
                        })
                    } else {
                        av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            geometry,
                            stream_format,
                            frame,
                            qp,
                            !predictive_started,
                            order_hint,
                            rgb_identity,
                        )
                    }
                }
            } else {
                av2_lossy_subsampled_bitstream_and_reconstruction_for_frame(
                    geometry,
                    stream_format,
                    frame,
                    qp,
                    rgb_identity,
                )
            };
            if options.predictive {
                predictive_started = true;
                predictive_reference = Some(frame.to_vec());
                predictive_reconstruction = Some(reconstruction.clone());
            }
            output
                .write_all(&bitstream)
                .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
            let public_reconstruction: Vec<u8>;
            let reconstruction = if packed_rgb_identity {
                public_reconstruction = planar_gbr_to_rgb24(&reconstruction, geometry);
                public_reconstruction.as_slice()
            } else {
                reconstruction.as_slice()
            };
            if let Some(recon) = recon.as_deref_mut() {
                recon
                    .write_all(reconstruction)
                    .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
            }
            if let Some(frame_metrics) = frame_metrics.as_deref_mut() {
                frame_metrics(Av2EncodeFrameMetrics {
                    frame_idx: frame_index,
                    frame_count: frame_limit.metric_count(),
                    bitstream_bytes: bitstream.len(),
                    source: &source_frame,
                    reconstruction,
                });
            }
            frame_index += 1;
            continue;
        }
        if options.predictive {
            return Err(format!(
                "AV2 predictive non-lossless encode for {} requires --qp to use the lossy residual path",
                request.format
            ));
        }

        let frame_mode = Av2Mvp444FrameMode::from_frame(frame, geometry, stream_format.bit_depth)?;

        let bitstream = av2_mvp_444_bitstream_for_mode(
            geometry,
            stream_format.bit_depth,
            &frame_mode,
            rgb_identity,
        );
        let reconstruction = frame_mode.reconstruction(geometry, stream_format.bit_depth);
        output
            .write_all(&bitstream)
            .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
        let public_reconstruction: Vec<u8>;
        let reconstruction = if packed_rgb_identity {
            public_reconstruction = planar_gbr_to_rgb24(&reconstruction, geometry);
            public_reconstruction.as_slice()
        } else {
            reconstruction.as_slice()
        };
        if let Some(recon) = recon.as_deref_mut() {
            recon
                .write_all(reconstruction)
                .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
        }
        if let Some(frame_metrics) = frame_metrics.as_deref_mut() {
            frame_metrics(Av2EncodeFrameMetrics {
                frame_idx: frame_index,
                frame_count: frame_limit.metric_count(),
                bitstream_bytes: bitstream.len(),
                source: &source_frame,
                reconstruction,
            });
        }
        frame_index += 1;
    }
    Ok(())
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
    stream_format: Av2StreamFormat,
    frame: &[u8],
    qp: u8,
    rgb_identity: bool,
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
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(
        &mut out,
        Av2ObuType::SequenceHeader,
        &av2_mvp_sequence_header_payload(geometry, Av2Black444MvpProfile::current(), stream_format),
    );
    append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &av2_lossy_subsampled_closed_loop_key_payload(
            geometry,
            stream_format,
            frame,
            &mut reconstruction,
            qp,
        ),
    );
    (out, reconstruction)
}

fn av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    qp: u8,
    include_sequence_header: bool,
    order_hint: u16,
    rgb_identity: bool,
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
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    if include_sequence_header {
        append_obu(
            &mut out,
            Av2ObuType::SequenceHeader,
            &av2_mvp_predictive_sequence_header_payload(
                geometry,
                Av2Black444MvpProfile::current(),
                stream_format,
            ),
        );
        append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    }
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &av2_lossy_subsampled_predictive_closed_loop_key_payload(
            geometry,
            stream_format,
            frame,
            &mut reconstruction,
            qp,
            order_hint,
        ),
    );
    (out, reconstruction)
}

fn av2_lossless_subsampled_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    rgb_identity: bool,
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
        ibc::build_local_ibc_subsampled(
            frame,
            geometry,
            stream_format.chroma_format,
            stream_format.bit_depth,
            &ibc_tile_bounds,
        )
        .ok()
        .filter(|ibc| ibc.stats().selected_copy_blocks() > 0)
    } else {
        None
    };
    let profile = if ibc.is_some() {
        Av2Black444MvpProfile::current().with_local_ibc_candidates()
    } else {
        Av2Black444MvpProfile::current()
    };
    let palette = palette::build_luma_palette_lossless(
        frame,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .ok();
    let mut reconstruction = vec![0; expected_len];
    let mut out = Vec::new();
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
    append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &av2_lossless_subsampled_closed_loop_key_payload(
            geometry,
            stream_format,
            frame,
            &mut reconstruction,
            profile,
            palette.as_ref(),
            ibc.as_ref(),
        ),
    );
    (out, reconstruction)
}

fn av2_lossless_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    include_sequence_header: bool,
    order_hint: u16,
    rgb_identity: bool,
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
        ibc::build_local_ibc_subsampled(
            frame,
            geometry,
            stream_format.chroma_format,
            stream_format.bit_depth,
            &ibc_tile_bounds,
        )
        .ok()
        .filter(|ibc| ibc.stats().selected_copy_blocks() > 0)
    } else {
        None
    };
    let profile = if ibc.is_some() {
        Av2Black444MvpProfile::current().with_local_ibc_candidates()
    } else {
        Av2Black444MvpProfile::current()
    };
    let palette = palette::build_luma_palette_lossless(
        frame,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .ok();
    let mut reconstruction = vec![0; expected_len];
    let mut out = Vec::new();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    if include_sequence_header {
        append_obu(
            &mut out,
            Av2ObuType::SequenceHeader,
            &av2_mvp_predictive_sequence_header_payload(geometry, profile, stream_format),
        );
        append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    }
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &av2_lossless_subsampled_predictive_closed_loop_key_payload(
            geometry,
            stream_format,
            frame,
            &mut reconstruction,
            profile,
            palette.as_ref(),
            ibc.as_ref(),
            order_hint,
        ),
    );
    (out, reconstruction)
}

fn av2_lossless_subsampled_regular_sef_bitstream_and_reconstruction_for_frame(
    frame: &[u8],
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
    (out, frame.to_vec())
}

fn av2_lossy_subsampled_regular_sef_bitstream_and_reconstruction_for_frame(
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
        &av2_mvp_444_sequence_header_payload(geometry, bit_depth, frame_mode.profile()),
    );
    append_rgb_content_interpretation_if_needed(&mut out, rgb_identity);
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &av2_mvp_444_closed_loop_key_payload(geometry, bit_depth, frame_mode),
    );
    out
}
