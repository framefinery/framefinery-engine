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
    mut recon: Option<&mut dyn Write>,
    request: Av2EncodeRequest,
    options: Av2EncodeOptions,
    mut frame_metrics: Option<&mut dyn for<'a> FnMut(Av2EncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    let mut av2_stats = stats::Av2StatsSink::from_env()?;
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
    let mut predictive_headers_written = false;
    let mut predictive_reference: Option<Vec<u8>> = None;
    let mut predictive_reconstruction: Option<Vec<u8>> = None;
    let frame_limit = FrameLimit::from_frame_count(request.params.frames);
    let mut source_frame = vec![0; source_expected_len];
    let mut frame_index = 0usize;
    let mut total_bitstream_bytes = 0usize;
    while frame_limit.should_read(frame_index) {
        let mut frame_stats = stats::Av2FrameStats::new(
            frame_index,
            geometry,
            request.format,
            stream_format,
            options.lossless,
            options.qp,
            options.gop.as_i32(),
        );
        #[cfg(feature = "av2-sb-bit-profile")]
        sb_bits::set_current_frame(frame_index);
        let stage_start = stats::Av2StageStart::now();
        let frame_was_read = read_input_frame(
            input,
            &mut source_frame,
            frame_index,
            frame_limit,
            "AV2 MVP input",
        )?;
        frame_stats.add_elapsed("read_frame", stage_start);
        if !frame_was_read {
            break;
        }
        let frame_encode_start = crate::timing::StageStart::now();
        if options.gop.resets_references_before(frame_index) {
            predictive_reference = None;
            predictive_reconstruction = None;
        }
        let predictive_enabled = options.gop.is_predictive();
        let predictive_frame = options.gop.is_predictive_frame(frame_index);
        let coded_frame: Vec<u8>;
        let frame = if packed_rgb_identity {
            let stage_start = stats::Av2StageStart::now();
            coded_frame = rgb24_to_planar_gbr(&source_frame, geometry);
            frame_stats.add_elapsed("rgb24_to_planar_gbr", stage_start);
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
            let (bitstream, reconstruction) = if predictive_enabled {
                let order_hint = av2_order_hint_for_frame(frame_index);
                if predictive_frame && predictive_reference.as_deref() == Some(frame) {
                    let stage_start = stats::Av2StageStart::now();
                    let result = av2_lossless_regular_sef_frame(frame, order_hint);
                    frame_stats.add_elapsed("lossless_show_existing_frame", stage_start);
                    result
                } else if predictive_frame {
                    if let Some((bitstream, reconstruction)) = predictive_reference
                    .as_deref()
                    .and_then(|reference| {
                        let stage_start = stats::Av2StageStart::now();
                        let result = av2_lossless_regular_inter_tiles_frame(
                            geometry,
                            stream_format,
                            frame,
                            reference,
                            order_hint,
                        );
                        frame_stats.add_elapsed("lossless_inter_tiles", stage_start);
                        result
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
                                !predictive_headers_written,
                                order_hint,
                                rgb_identity,
                                &mut frame_stats,
                            );
                        predictive_headers_written = true;
                        predictive_reference = Some(frame.to_vec());
                        result
                    }
                } else {
                    let result =
                        av2_lossless_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            geometry,
                            stream_format,
                            frame,
                            !predictive_headers_written,
                            order_hint,
                            rgb_identity,
                            &mut frame_stats,
                        );
                    predictive_headers_written = true;
                    predictive_reference = Some(frame.to_vec());
                    result
                }
            } else {
                av2_lossless_subsampled_bitstream_and_reconstruction_for_frame(
                    geometry,
                    stream_format,
                    frame,
                    rgb_identity,
                    &mut frame_stats,
                )
            };
            frame_stats.set_bitstream_bytes(bitstream.len());
            let stage_start = stats::Av2StageStart::now();
            output
                .write_all(&bitstream)
                .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
            total_bitstream_bytes += bitstream.len();
            frame_stats.add_elapsed("bitstream_write", stage_start);
            let public_reconstruction: Vec<u8>;
            let reconstruction = if packed_rgb_identity {
                let stage_start = stats::Av2StageStart::now();
                public_reconstruction = planar_gbr_to_rgb24(&reconstruction, geometry);
                frame_stats.add_elapsed("planar_gbr_to_rgb24", stage_start);
                public_reconstruction.as_slice()
            } else {
                reconstruction.as_slice()
            };
            if let Some(recon) = recon.as_deref_mut() {
                let stage_start = stats::Av2StageStart::now();
                recon
                    .write_all(reconstruction)
                    .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
                frame_stats.add_elapsed("write_reconstruction", stage_start);
            }
            if let Some(frame_metrics) = frame_metrics.as_deref_mut() {
                let stage_start = stats::Av2StageStart::now();
                frame_metrics(Av2EncodeFrameMetrics {
                    frame_idx: frame_index,
                    frame_count: frame_limit.metric_count(),
                    bitstream_bytes: bitstream.len(),
                    total_bitstream_bytes,
                    encode_elapsed: frame_encode_start.elapsed(),
                    source: &source_frame,
                    reconstruction,
                });
                frame_stats.add_elapsed("frame_metrics", stage_start);
            }
            av2_stats.write_frame(&frame_stats)?;
            frame_index += 1;
            continue;
        }
        if stream_format.chroma_format == Av2ChromaFormat::Yuv422 && options.qp.is_none() {
            return Err(format!(
                "AV2 non-lossless encode is not implemented for {}; pass --set qp=<1..255> to use the experimental lossy residual path",
                request.format
            ));
        }
        let use_lossy_residual_path =
            options.qp.is_some() || stream_format.chroma_format == Av2ChromaFormat::Yuv420;
        if use_lossy_residual_path {
            let qp = options.qp.unwrap_or(AV2_LOSSY_DEFAULT_QP);
            let (bitstream, reconstruction) = if predictive_enabled {
                let order_hint = av2_order_hint_for_frame(frame_index);
                if predictive_frame && predictive_reference.as_deref() == Some(frame) {
                    if let Some(reference_reconstruction) = predictive_reconstruction.as_deref() {
                        let stage_start = stats::Av2StageStart::now();
                        let result = av2_lossy_regular_sef_frame(
                            reference_reconstruction, order_hint,
                        );
                        frame_stats.add_elapsed("lossy_show_existing_frame", stage_start);
                        result
                    } else {
                        av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            geometry,
                            stream_format,
                            frame,
                            qp,
                            !predictive_headers_written,
                            order_hint,
                            rgb_identity,
                            &mut frame_stats,
                        )
                    }
                } else if predictive_frame {
                    if let (Some(reference), Some(reference_reconstruction)) = (
                        predictive_reference.as_deref(),
                        predictive_reconstruction.as_deref(),
                    ) {
                        let stage_start = stats::Av2StageStart::now();
                        let inter_result = av2_lossy_zero_mv_inter_tiles_frame(
                            geometry,
                            stream_format,
                            frame,
                            reference,
                            reference_reconstruction,
                            qp,
                            order_hint,
                        );
                        frame_stats.add_elapsed("lossy_zero_mv_inter_tiles", stage_start);
                        inter_result.unwrap_or_else(|| {
                            av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                                geometry,
                                stream_format,
                                frame,
                                qp,
                                !predictive_headers_written,
                                order_hint,
                                rgb_identity,
                                &mut frame_stats,
                            )
                        })
                    } else {
                        av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            geometry,
                            stream_format,
                            frame,
                            qp,
                            !predictive_headers_written,
                            order_hint,
                            rgb_identity,
                            &mut frame_stats,
                        )
                    }
                } else {
                    av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                        geometry,
                        stream_format,
                        frame,
                        qp,
                        !predictive_headers_written,
                        order_hint,
                        rgb_identity,
                        &mut frame_stats,
                    )
                }
            } else {
                av2_lossy_subsampled_bitstream_and_reconstruction_for_frame(
                    geometry,
                    stream_format,
                    frame,
                    qp,
                    rgb_identity,
                    &mut frame_stats,
                )
            };
            if predictive_enabled {
                predictive_headers_written = true;
                predictive_reference = Some(frame.to_vec());
                predictive_reconstruction = Some(reconstruction.clone());
            }
            frame_stats.set_bitstream_bytes(bitstream.len());
            let stage_start = stats::Av2StageStart::now();
            output
                .write_all(&bitstream)
                .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
            total_bitstream_bytes += bitstream.len();
            frame_stats.add_elapsed("bitstream_write", stage_start);
            let public_reconstruction: Vec<u8>;
            let reconstruction = if packed_rgb_identity {
                let stage_start = stats::Av2StageStart::now();
                public_reconstruction = planar_gbr_to_rgb24(&reconstruction, geometry);
                frame_stats.add_elapsed("planar_gbr_to_rgb24", stage_start);
                public_reconstruction.as_slice()
            } else {
                reconstruction.as_slice()
            };
            if let Some(recon) = recon.as_deref_mut() {
                let stage_start = stats::Av2StageStart::now();
                recon
                    .write_all(reconstruction)
                    .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
                frame_stats.add_elapsed("write_reconstruction", stage_start);
            }
            if let Some(frame_metrics) = frame_metrics.as_deref_mut() {
                let stage_start = stats::Av2StageStart::now();
                frame_metrics(Av2EncodeFrameMetrics {
                    frame_idx: frame_index,
                    frame_count: frame_limit.metric_count(),
                    bitstream_bytes: bitstream.len(),
                    total_bitstream_bytes,
                    encode_elapsed: frame_encode_start.elapsed(),
                    source: &source_frame,
                    reconstruction,
                });
                frame_stats.add_elapsed("frame_metrics", stage_start);
            }
            av2_stats.write_frame(&frame_stats)?;
            frame_index += 1;
            continue;
        }
        if predictive_enabled {
            return Err(format!(
                "AV2 predictive GOP encode for {} requires --set qp=<1..255> to use the lossy residual path; use --set gop=0 for the intra-only legacy path",
                request.format
            ));
        }

        let stage_start = stats::Av2StageStart::now();
        let frame_mode = Av2Mvp444FrameMode::from_frame(frame, geometry, stream_format.bit_depth)?;
        frame_stats.add_elapsed("mvp_444_mode_decision", stage_start);

        let stage_start = stats::Av2StageStart::now();
        let bitstream = av2_mvp_444_bitstream_for_mode(
            geometry,
            stream_format.bit_depth,
            &frame_mode,
            rgb_identity,
        );
        frame_stats.add_elapsed("mvp_444_bitstream", stage_start);
        let stage_start = stats::Av2StageStart::now();
        let reconstruction = frame_mode.reconstruction(geometry, stream_format.bit_depth);
        frame_stats.add_elapsed("mvp_444_reconstruction", stage_start);
        frame_stats.set_bitstream_bytes(bitstream.len());
        let stage_start = stats::Av2StageStart::now();
        output
            .write_all(&bitstream)
            .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
        total_bitstream_bytes += bitstream.len();
        frame_stats.add_elapsed("bitstream_write", stage_start);
        let public_reconstruction: Vec<u8>;
        let reconstruction = if packed_rgb_identity {
            let stage_start = stats::Av2StageStart::now();
            public_reconstruction = planar_gbr_to_rgb24(&reconstruction, geometry);
            frame_stats.add_elapsed("planar_gbr_to_rgb24", stage_start);
            public_reconstruction.as_slice()
        } else {
            reconstruction.as_slice()
        };
        if let Some(recon) = recon.as_deref_mut() {
            let stage_start = stats::Av2StageStart::now();
            recon
                .write_all(reconstruction)
                .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
            frame_stats.add_elapsed("write_reconstruction", stage_start);
        }
        if let Some(frame_metrics) = frame_metrics.as_deref_mut() {
            let stage_start = stats::Av2StageStart::now();
            frame_metrics(Av2EncodeFrameMetrics {
            frame_idx: frame_index,
            frame_count: frame_limit.metric_count(),
            bitstream_bytes: bitstream.len(),
            total_bitstream_bytes,
            encode_elapsed: frame_encode_start.elapsed(),
            source: &source_frame,
                reconstruction,
            });
            frame_stats.add_elapsed("frame_metrics", stage_start);
        }
        av2_stats.write_frame(&frame_stats)?;
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
        &av2_mvp_sequence_header_payload(geometry, Av2Black444MvpProfile::current(), stream_format),
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
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &payload,
    );
    frame_stats.add_elapsed("lossy_entropy_pack", stage_start);
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
            &av2_mvp_predictive_sequence_header_payload(
                geometry,
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
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &payload,
    );
    frame_stats.add_elapsed("lossy_entropy_pack", stage_start);
    (out, reconstruction)
}

fn av2_lossless_subsampled_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
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
        &av2_mvp_sequence_header_payload(geometry, profile, stream_format),
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
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &payload,
    );
    frame_stats.add_elapsed("lossless_entropy_pack", stage_start);
    (out, reconstruction)
}

fn av2_lossless_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
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
    if include_sequence_header {
        append_obu(
            &mut out,
            Av2ObuType::SequenceHeader,
            &av2_mvp_predictive_sequence_header_payload(geometry, profile, stream_format),
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
    append_obu(
        &mut out,
        Av2ObuType::ClosedLoopKey,
        &payload,
    );
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

fn av2_lossless_regular_sef_frame(
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
