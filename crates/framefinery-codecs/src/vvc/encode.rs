#[cfg(test)]
pub fn eos_annex_b() -> Vec<u8> {
    write_annex_b(&[VvcNalUnit::eos()]).expect("hard-coded EOS NAL should be valid")
}

const VVC_PREDICTIVE_SINGLE_SLICE_PPS_ID: u8 = 1;
#[cfg(test)]
const VVC_PREDICTIVE_FRAME_SKIP_PPS_ID: u8 = VVC_PREDICTIVE_SINGLE_SLICE_PPS_ID;
const VVC_LOSSY_PREDICTIVE_SKIP_MAX_ABS_8BIT: u16 = 2;
// Keep production SCC emission off until the complete VTM IBC prediction-unit
// contract is matched. Search and quantizer plumbing remains covered by
// focused tests.
const VVC_SCC_IBC_PRODUCTION_ENABLED: bool = true;

#[cfg(test)]
pub fn vvc_black_yuv420p8_annex_b(params: VvcEncodeParams) -> Result<Vec<u8>, String> {
    validate_vvc_exact_frame_count(params)?;
    let input = vec![0; Picture::expected_len(8, 8, PixelFormat::Yuv420p8) * params.frames];
    vvc_yuv420p8_annex_b_from_input(&input, params)
}

#[cfg(test)]
pub fn vvc_yuv420p8_annex_b_from_input(
    input: &[u8],
    params: VvcEncodeParams,
) -> Result<Vec<u8>, String> {
    vvc_yuv_annex_b_from_input(
        input,
        params,
        VvcVideoGeometry {
            width: 8,
            height: 8,
        },
        PixelFormat::Yuv420p8,
    )
}

#[cfg(test)]
pub fn vvc_yuv420p_annex_b_from_input(
    input: &[u8],
    params: VvcEncodeParams,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    vvc_yuv_annex_b_from_input(
        input,
        params,
        VvcVideoGeometry {
            width: 8,
            height: 8,
        },
        format,
    )
}

#[cfg(test)]
pub fn vvc_default_yuv_annex_b_from_input(
    input: &[u8],
    params: VvcEncodeParams,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    vvc_yuv_annex_b_from_input(
        input,
        params,
        VvcVideoGeometry {
            width: 8,
            height: 8,
        },
        format,
    )
}

#[cfg(test)]
pub fn vvc_yuv_annex_b_from_input(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    vvc_yuv_annex_b_from_input_with_limits(
        input,
        params,
        geometry,
        VvcVideoLimits::unbounded(),
        format,
    )
}

#[cfg(test)]
pub fn vvc_yuv_annex_b_from_input_with_limits(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    Ok(
        vvc_yuv_encode_artifacts_from_input_with_limits(input, params, geometry, limits, format)?
            .bitstream,
    )
}

#[cfg(test)]
pub fn vvc_yuv_encode_artifacts_from_input_with_limits(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
) -> Result<VvcEncodeArtifacts, String> {
    let mut reader = Cursor::new(input);
    let mut bitstream = Vec::new();
    let mut reconstruction = Vec::new();
    vvc_yuv_encode_stream_with_limits(
        &mut reader,
        &mut bitstream,
        Some(&mut reconstruction),
        params,
        geometry,
        limits,
        format,
    )?;
    Ok(VvcEncodeArtifacts {
        bitstream,
        reconstruction,
    })
}

#[cfg(test)]
pub fn vvc_yuv_encode_stream_with_limits<R: Read, W: Write>(
    input: &mut R,
    bitstream: &mut W,
    reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
) -> Result<(), String> {
    vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics(
        input,
        bitstream,
        reconstruction,
        params,
        geometry,
        limits,
        format,
        VvcEncodeOptions::default(),
        None,
    )
}

#[cfg(test)]
pub fn vvc_yuv_encode_stream_with_limits_and_frame_metrics<R: Read, W: Write>(
    input: &mut R,
    bitstream: &mut W,
    reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
    frame_metrics: Option<&mut dyn for<'a> FnMut(VvcEncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics(
        input,
        bitstream,
        reconstruction,
        params,
        geometry,
        limits,
        format,
        VvcEncodeOptions::default(),
        frame_metrics,
    )
}

pub fn vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics<
    R: Read + ?Sized,
    W: Write + ?Sized,
>(
    input: &mut R,
    bitstream: &mut W,
    mut reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
    options: VvcEncodeOptions,
    mut frame_metrics: Option<&mut dyn for<'a> FnMut(VvcEncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    let request = VvcEncodeRequest {
        params,
        geometry,
        limits,
        format,
    }
    .validate()?;
    let geometry = request.geometry;
    let frame_limit = request.frame_limit;
    let stream_format = request.format;
    let stream_layout = PlanarYuvGeometry::new(
        geometry.width,
        geometry.height,
        stream_format.chroma_sampling,
        stream_format.bit_depth,
    )?;
    let stream_frame_layout = PlanarYuvFrameLayout::for_validated_shape(
        geometry.width,
        geometry.height,
        stream_format.chroma_sampling,
        stream_format.bit_depth,
    );
    let frame_len = stream_layout.frame_len();
    let ctu_cols = vvc_picture_ctu_cols(geometry);
    let ctu_count = vvc_picture_ctu_count(geometry);
    let residual_mode = VvcResidualCodingMode::for_encode_options(options);
    let residual_policy =
        VvcResidualCodingPolicy::new(stream_format, residual_mode).with_fast_search(options.fast_search);
    let predictive_enabled = options.gop.is_predictive();
    let ctu_sliced_partitioning_supported =
        vvc_one_slice_per_ctu_partitioning_supported(geometry);
    let lossy_ctu_skip_enabled = predictive_enabled
        && !residual_mode.is_lossless()
        && vvc_lossy_predictive_ctu_inter_skip_enabled_for_reference_clean_release();
    let mut slice_config = vvc_slice_config_for_input_format(
        residual_mode.slice_config(stream_format, options.qp, options.fast_search),
        format,
    );
    if residual_mode.is_lossless() && options.fast_search == VvcFastSearch::LosslessSpeed {
        slice_config = slice_config.without_lossless_speed_unused_tools();
    }
    slice_config = slice_config.with_validated_profile_for_format(options.profile, stream_format)?;
    let scc_444_enabled = VVC_SCC_IBC_PRODUCTION_ENABLED
        && !predictive_enabled
        && residual_mode.is_lossless()
        && options.fast_search == VvcFastSearch::LosslessSpeed
        && stream_format.chroma_sampling == ChromaSampling::Cs444
        && !format.is_rgb()
        && slice_config.profile.allows_ibc();
    if scc_444_enabled {
        slice_config = slice_config.with_scc_444_tools();
    }
    let picture_partitioning = if predictive_enabled && ctu_sliced_partitioning_supported {
        VvcPicturePartitioning::OneSlicePerCtu
    } else {
        residual_mode.picture_partitioning()
    };
    if predictive_enabled {
        slice_config = slice_config.with_inter_enabled();
    }
    let predictive_single_slice_config = predictive_enabled.then(|| {
        slice_config
            .with_picture_parameter_set_id(VVC_PREDICTIVE_SINGLE_SLICE_PPS_ID)
            .without_picture_header_slice_state()
    });
    let luma_qp = slice_config.slice_qp;
    let chroma_qp = if residual_mode.is_lossless() {
        slice_config.slice_qp
    } else {
        vvc_lossy_chroma_qp_for_slice_qp(luma_qp)
    };
    let transform_skip_quant_tables =
        VvcTransformSkipQuantTables::new(format.bit_depth(), luma_qp, chroma_qp);
    let explicit_inter_eligible_luma_leaf_count = if stream_format.chroma_sampling
        == ChromaSampling::Cs444
    {
        vvc_explicit_inter_eligible_luma_leaf_count(geometry, stream_format.chroma_sampling)
    } else {
        0
    };
    let emit_base_picture_parameter_set = !predictive_enabled || ctu_sliced_partitioning_supported;
    let mut parameter_sets = Vec::with_capacity(
        1 + usize::from(emit_base_picture_parameter_set)
            + usize::from(predictive_single_slice_config.is_some()),
    );
    parameter_sets.push(vvc_sps_unit(
        geometry,
        slice_config,
        stream_format.bit_depth,
    ));
    if emit_base_picture_parameter_set {
        parameter_sets.push(vvc_pps_unit_with_partitioning_and_config(
            geometry,
            picture_partitioning,
            slice_config,
        ));
    }
    if let Some(single_slice_config) = predictive_single_slice_config {
        parameter_sets.push(vvc_pps_unit_with_partitioning_and_config(
            geometry,
            VvcPicturePartitioning::SingleSlice,
            single_slice_config,
        ));
    }
    let mut total_bitstream_bytes = write_annex_b_to(bitstream, &parameter_sets)?;

    #[cfg(feature = "vvc-stats")]
    let mut vvc_stats = VvcStatsSink::from_env()?;
    #[cfg(feature = "vvc-stats")]
    let mut vvc_ctu_bits = VvcCtuBitSink::from_env()?;

    let mut frame_buf = vec![0; frame_len];
    let mut ctu_inter_skip_slice_payload_cache = VvcCtuInterSkipSlicePayloadCache::default();
    let mut previous_predictive_cache: Option<std::sync::Arc<VvcPredictiveFrameCache>> = None;
    let mut frame_idx = 0usize;
    while frame_limit.should_read(frame_idx) {
        #[cfg(feature = "vvc-stats")]
        let mut frame_stats = VvcFrameStats::new(
            frame_idx,
            geometry,
            stream_format,
            options.lossless,
            slice_config.slice_qp,
            chroma_qp,
        );
        #[cfg(feature = "vvc-stats")]
        let stage_start = StageStart::now();
        let frame_available =
            read_input_frame(input, &mut frame_buf, frame_idx, frame_limit, "VVC input")?;
        #[cfg(feature = "vvc-stats")]
        frame_stats.add_elapsed("read_frame", stage_start);
        if !frame_available {
            break;
        }
        let frame_encode_start = StageStart::now();
        if options.gop.resets_references_before(frame_idx) {
            previous_predictive_cache = None;
        }
        let predictive_frame = options.gop.is_predictive_frame(frame_idx);
        let repeated_predictive_cache = if predictive_frame
            && vvc_predictive_frame_inter_skip_enabled_for_reference_clean_release()
        {
            previous_predictive_cache
                .as_ref()
                .filter(|cache| cache.source.as_slice() == frame_buf.as_slice())
                .cloned()
        } else {
            None
        };
        let (frame_recon_yuv, frame_bitstream_bytes, next_predictive_cache) =
            if let Some(repeated_cache) = repeated_predictive_cache {
                let mut frame_bitstream = CountingWriter::new(bitstream);
                #[cfg(feature = "vvc-stats")]
                let stage_start = StageStart::now();
                #[cfg(feature = "vvc-stats")]
                let entropy_build_start = StageStart::now();
                let frame_ctus: Vec<_> = vvc_ctu_regions(geometry)
                    .map(|region| VvcQuantizedCtu {
                        slice_address: region.slice_address,
                        geometry: region.geometry,
                        payload: VvcQuantizedCtuPayload::InterSkip,
                    })
                    .collect();
                let frame_slice_units = vec![vvc_predictive_frame_slice_unit(
                    frame_idx,
                    geometry,
                    &frame_ctus,
                    predictive_single_slice_config
                        .expect("predictive single-slice config is available in predictive mode"),
                )?];
                #[cfg(feature = "vvc-stats")]
                {
                    frame_stats.add_counter("predictive_reused_ctu_count", ctu_count as u64);
                    frame_stats.add_counter("predictive_exact_ctu_count", ctu_count as u64);
                    frame_stats.add_counter("predictive_inter_skip_ctu_count", ctu_count as u64);
                    frame_stats.add_counter("predictive_frame_skip_count", 1);
                    frame_stats.add_counter("slice_count", frame_slice_units.len() as u64);
                    frame_stats.add_counter(
                        "frame_entropy_build_nanos",
                        entropy_build_start.elapsed().as_nanos() as u64,
                    );
                }
                #[cfg(feature = "vvc-stats")]
                let annexb_write_start = StageStart::now();
                write_annex_b_to(&mut frame_bitstream, &frame_slice_units)?;
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_counter(
                    "frame_annexb_write_nanos",
                    annexb_write_start.elapsed().as_nanos() as u64,
                );
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_elapsed("frame_entropy_write", stage_start);
                #[cfg(feature = "vvc-stats")]
                let stage_start = StageStart::now();
                let yuv = repeated_cache.reconstruction.to_yuv();
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_elapsed("frame_recon_finalize", stage_start);
                (
                    yuv,
                    frame_bitstream.bytes_written(),
                    Some(repeated_cache),
                )
            } else {
        #[cfg(feature = "vvc-stats")]
        let stage_start = StageStart::now();
        let source_frame =
            sample_vvc_yuv_frame(&frame_buf, VvcEncodeParams { frames: 1 }, geometry, format)?;
        #[cfg(feature = "vvc-stats")]
        frame_stats.add_elapsed("sample_frame", stage_start);
        let previous_motion_source_frame = if predictive_frame && !residual_mode.is_lossless() {
            previous_predictive_cache.as_ref().and_then(|cache| {
                sample_vvc_yuv_frame(
                    &cache.source,
                    VvcEncodeParams { frames: 1 },
                    geometry,
                    format,
                )
                .ok()
            })
        } else {
            None
        };
        let lossy_mixed_single_p_slice_supported =
            !residual_mode.is_lossless()
                && vvc_lossy_mixed_single_p_slice_supported(stream_format, geometry);
        let explicit_luma_inter_decisions_by_ctu = if predictive_frame
            && lossy_mixed_single_p_slice_supported
        {
            previous_predictive_cache
                .as_ref()
                .zip(previous_motion_source_frame.as_ref())
                .map(|(cache, previous_source_frame)| {
                    vvc_ctu_regions(geometry)
                        .map(|region| {
                            vvc_predictive_luma_inter_decisions_for_ctu(
                                &source_frame,
                                previous_source_frame,
                                &cache.reconstruction,
                                region,
                                VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
                                stream_format,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let explicit_luma_inter_decision_count = explicit_luma_inter_decisions_by_ctu
            .iter()
            .filter_map(Option::as_ref)
            .map(vvc_luma_inter_decision_count)
            .sum::<usize>();
        let explicit_inter_frame = vvc_explicit_inter_decision_count_allows_mixed_p_slice(
            explicit_luma_inter_decision_count,
            stream_format,
            explicit_inter_eligible_luma_leaf_count,
        );
        let predictive_ctu_inter_skip_enabled = predictive_frame
            && !explicit_inter_frame
            && (lossy_mixed_single_p_slice_supported || ctu_sliced_partitioning_supported)
            && vvc_predictive_ctu_inter_skip_enabled_for_reference_clean_release();
        let lossy_ctu_skip_candidate_distortions = if predictive_ctu_inter_skip_enabled
            && lossy_ctu_skip_enabled
        {
            previous_predictive_cache
                .as_ref()
                .map(|cache| {
                    cache.lossy_inter_skip_candidate_distortions(
                        &frame_buf,
                        stream_frame_layout,
                        &source_frame,
                        geometry,
                    )
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let predictive_ctu_skip_candidate_count = if predictive_ctu_inter_skip_enabled {
            if residual_mode.is_lossless() {
                vvc_predictive_frame_lossless_ctu_inter_skip_candidate_count(
                    previous_predictive_cache.as_deref(),
                    &frame_buf,
                    stream_frame_layout,
                    geometry,
                )
            } else {
                lossy_ctu_skip_candidate_distortions
                    .iter()
                    .filter(|distortion| distortion.is_some())
                    .count()
            }
        } else {
            0
        };
        let predictive_ctu_inter_skip_candidate_frame = predictive_ctu_skip_candidate_count > 0
            && (residual_mode.is_lossless()
                || vvc_lossy_predictive_ctu_skip_candidate_count_allows_frame_reuse(
                    predictive_ctu_skip_candidate_count,
                    ctu_count,
                ));
        let predictive_ctu_inter_skip_single_p_slice =
            predictive_ctu_inter_skip_candidate_frame
                && lossy_mixed_single_p_slice_supported
                && vvc_lossy_predictive_ctu_skip_candidate_count_allows_mixed_p_slice(
                    predictive_ctu_skip_candidate_count,
                    ctu_count,
                    stream_format,
                );
        let mixed_single_p_slice_frame = lossy_mixed_single_p_slice_supported
            && (predictive_ctu_inter_skip_single_p_slice || explicit_inter_frame);
        let predictive_ctu_slice_frame = predictive_ctu_inter_skip_candidate_frame
            && !mixed_single_p_slice_frame
            && ctu_sliced_partitioning_supported;
        let predictive_ctu_inter_skip_frame = predictive_ctu_inter_skip_candidate_frame
            && (mixed_single_p_slice_frame || predictive_ctu_slice_frame);
        {
            let mut frame_bitstream = CountingWriter::new(bitstream);
            let (frame_recon_yuv, next_predictive_cache) = {
                let mut frame_recon =
                    VvcReconstructionFrame::new_neutral(geometry, source_frame.format);
                let mut frame_ctus = Vec::with_capacity(ctu_count);
                let mut frame_ctu_decisions =
                    predictive_enabled.then(|| Vec::with_capacity(ctu_count));
                let mut predictive_reused_ctus = vec![false; ctu_count];
                let mut luma_mode_search_state =
                    VvcLumaModeSearchState::new_for_geometry(geometry);
                let mut ctu_quant_scratch = VvcCtuQuantScratch::default();
                let mut ibc_search = scc_444_enabled.then(VvcIbcHashSearch::new);
                for region in vvc_ctu_regions(geometry) {
                    let luma_scc_decisions = if let Some(ibc_search) = ibc_search.as_mut() {
                        ibc_search.prepare_for_ctu(region.origin_x, region.origin_y);
                        Some(vvc_ibc_decisions_for_region(
                            &source_frame,
                            &frame_recon,
                            ibc_search,
                            region,
                        ))
                    } else {
                        None
                    };
                    #[cfg(feature = "vvc-stats")]
                    let stage_start = StageStart::now();
                    #[cfg(feature = "vvc-stats")]
                    if let Some(previous_source_frame) = previous_motion_source_frame.as_ref() {
                        let motion_start = StageStart::now();
                        let motion_analysis = motion::vvc_luma_motion_analysis_for_region(
                            &source_frame,
                            previous_source_frame,
                            region,
                            16,
                            u64::from(vvc_lossy_predictive_skip_max_abs_delta(
                                stream_format.bit_depth,
                            )),
                        );
                        frame_stats.add_elapsed("predictive_luma_motion_analyze", motion_start);
                        if motion_analysis.block_count > 0 {
                            frame_stats.add_counter(
                                "predictive_luma_motion_8x8_block_count",
                                motion_analysis.block_count as u64,
                            );
                            frame_stats.add_counter(
                                "predictive_luma_motion_exact_8x8_count",
                                motion_analysis.exact_count as u64,
                            );
                            frame_stats.add_counter(
                                "predictive_luma_motion_nonzero_8x8_count",
                                motion_analysis.nonzero_count as u64,
                            );
                            frame_stats.add_counter(
                                "predictive_luma_motion_nonzero_exact_8x8_count",
                                motion_analysis.nonzero_exact_count as u64,
                            );
                            frame_stats.add_counter(
                                "predictive_luma_motion_near_8x8_count",
                                motion_analysis.near_count as u64,
                            );
                            frame_stats.add_counter(
                                "predictive_luma_motion_nonzero_near_8x8_count",
                                motion_analysis.nonzero_near_count as u64,
                            );
                            frame_stats.add_counter(
                                "predictive_luma_motion_total_sad",
                                motion_analysis.total_sad,
                            );
                            add_vvc_luma_motion_aggregate_counters(
                                &mut frame_stats,
                                "predictive_luma_motion_16x16",
                                motion_analysis.aggregate_16x16,
                            );
                            add_vvc_luma_motion_aggregate_counters(
                                &mut frame_stats,
                                "predictive_luma_motion_32x32",
                                motion_analysis.aggregate_32x32,
                            );
                            add_vvc_luma_motion_aggregate_counters(
                                &mut frame_stats,
                                "predictive_luma_motion_64x64",
                                motion_analysis.aggregate_64x64,
                            );
                        }
                    }
                    #[cfg(feature = "vvc-stats")]
                    if slice_config.profile.allows_palette() && slice_config.profile.allows_ibc() {
                        let scc_start = StageStart::now();
                        let scc_analysis = ibc::vvc_scc_analysis_for_region(&source_frame, region);
                        frame_stats.add_elapsed("scc_analyze", scc_start);
                        if scc_analysis.block_count > 0 {
                            add_vvc_scc_analysis_counters(&mut frame_stats, scc_analysis);
                        }
                    }
                    let cached_exact_ctu = if predictive_frame
                        && vvc_predictive_intra_ctu_reuse_enabled_for_mode(residual_mode)
                    {
                        previous_predictive_cache.as_ref().and_then(|cache| {
                            cache.matching_decision(&frame_buf, stream_frame_layout, region)
                        })
                    } else {
                        None
                    };
                    let cached_exact_ctu_available = cached_exact_ctu.is_some();
                    let cached_lossy_skip_ctu = if !predictive_ctu_inter_skip_enabled
                        || !predictive_frame
                        || cached_exact_ctu_available
                        || !lossy_ctu_skip_enabled
                    {
                        None
                    } else {
                        previous_predictive_cache.as_ref().and_then(|cache| {
                            lossy_ctu_skip_candidate_distortions
                                .get(region.slice_address)
                                .copied()
                                .flatten()
                                .and_then(|skip_distortion| {
                                    cache
                                        .reusable_decision(region)
                                        .map(|cached| (cached, skip_distortion))
                                })
                        })
                    };
                    let cached_inter_skip_ctu = if residual_mode.is_lossless() {
                        cached_exact_ctu
                    } else {
                        None
                    };
                    let preselected_lossy_inter_skip_ctu = cached_lossy_skip_ctu.filter(
                        |(_, skip_distortion)| {
                            predictive_ctu_inter_skip_frame
                                && vvc_predictive_inter_skip_region(region)
                                && vvc_lossy_predictive_inter_skip_preselected(
                                    *skip_distortion,
                                    region,
                                    stream_format,
                                )
                        },
                    );
                    let cached_inter_skip_ctu_available = cached_inter_skip_ctu.is_some();
                    let mut inter_skip_ctu =
                        (cached_inter_skip_ctu_available
                            && predictive_ctu_inter_skip_frame
                            && vvc_predictive_inter_skip_region(region))
                            || preselected_lossy_inter_skip_ctu.is_some();
                    let intra_reuse_allowed = cached_exact_ctu_available
                        && (!predictive_frame
                            || cached_exact_ctu
                                .is_some_and(|cached| {
                                    cached.decision.luma_split_kind
                                        == VvcLumaSplitAvailabilityKind::Inter
                                }))
                        && vvc_predictive_ctu_dependencies_reused(
                            region,
                            ctu_cols,
                            &predictive_reused_ctus,
                        );
                    let mut reused_predictive_ctu = false;
                    let temporal_mode_hint_allowed =
                        predictive_frame && options.fast_search == VvcFastSearch::LosslessSpeed;
                    let temporal_mode_hint = if temporal_mode_hint_allowed {
                        previous_predictive_cache
                            .as_ref()
                            .and_then(|cache| cache.ctu_decision(region))
                    } else {
                        None
                    };
                    let reusable_ctu = if let Some((cached, _)) = preselected_lossy_inter_skip_ctu {
                        Some(cached)
                    } else if inter_skip_ctu {
                        cached_inter_skip_ctu
                    } else if intra_reuse_allowed {
                        cached_exact_ctu
                    } else {
                        None
                    };
                    let payload = if let Some(cached) = reusable_ctu {
                        frame_recon.copy_ctu_from(&cached.reconstruction, region);
                        reused_predictive_ctu = true;
                        if let Some(decisions) = frame_ctu_decisions.as_mut() {
                            decisions.push(std::sync::Arc::clone(cached.decision));
                        }
                        let decision = cached.decision.as_ref();
                        #[cfg(feature = "vvc-stats")]
                        if !inter_skip_ctu {
                            add_vvc_quantized_ctu_counters(&mut frame_stats, &decision.quantized);
                            match decision.luma_max_leaf_size {
                                VVC_LOSSLESS_LUMA_LEAF_SIZE => {
                                    frame_stats.add_counter("luma_ctu_leaf4_count", 1);
                                }
                                VVC_CURRENT_MAX_LUMA_LEAF_SIZE => {
                                    frame_stats.add_counter("luma_ctu_leaf8_count", 1);
                                }
                                _ => {}
                            }
                        }
                        #[cfg(feature = "vvc-stats")]
                        if vvc_ctu_bits.is_enabled() && !inter_skip_ctu {
                            vvc_ctu_bits.write_ctu(
                                frame_idx,
                                geometry,
                                region,
                                stream_format,
                                options.lossless,
                                slice_config.slice_qp,
                                chroma_qp,
                                &decision.quantized,
                                decision.luma_max_leaf_size,
                                slice_config,
                            )?;
                        }
                        if inter_skip_ctu {
                            VvcQuantizedCtuPayload::InterSkip
                        } else {
                            vvc_intra_ctu_payload_from_decision(
                                region,
                                decision,
                                slice_config,
                                None,
                                None,
                                predictive_frame && !residual_mode.is_lossless(),
                            )?
                        }
                    } else {
                        if predictive_ctu_slice_frame {
                            frame_recon.clear_availability();
                            luma_mode_search_state.clear();
                        }
                        // CTU-sliced fallback gives each non-skipped CTU its
                        // own intra slice.  Its partition must therefore use
                        // the intra tree even when the surrounding frame is
                        // predictive; only a mixed single-slice P picture
                        // needs the inter partition contract for every CTU.
                        let inter_slice_partition = mixed_single_p_slice_frame;
                        let ctu_residual_policy = residual_policy
                            .with_inter_slice_partition(inter_slice_partition)
                            .with_dual_tree_intra(
                                !inter_slice_partition
                                    && !mixed_single_p_slice_frame
                                    && slice_config.coding_tree.dual_tree_intra,
                            );
                        let luma_max_leaf_size = if explicit_inter_frame {
                            VVC_CURRENT_MAX_LUMA_LEAF_SIZE
                        } else {
                            select_vvc_luma_max_leaf_size_for_ctu(
                                ctu_residual_policy,
                                &source_frame,
                                region,
                                luma_qp,
                            )
                        };
                        let luma_inter_skip_mask = if frame_ctu_decisions.is_some()
                            && predictive_frame
                            && residual_mode.is_lossless()
                            && options.fast_search == VvcFastSearch::LosslessSpeed
                            && vvc_lossless_speed_luma_leaf_inter_skip_allowed(stream_format)
                        {
                            previous_predictive_cache.as_ref().and_then(|cache| {
                                vvc_predictive_luma_leaf_inter_skip_mask(
                                    &frame_buf,
                                    &cache.source,
                                    stream_frame_layout,
                                    region,
                                    luma_max_leaf_size,
                                    stream_format.chroma_sampling,
                                    ctu_residual_policy.dual_tree_intra(),
                                )
                            })
                        } else {
                            None
                        };
                        let chroma_inter_skip_mask = if frame_ctu_decisions.is_some()
                            && predictive_frame
                            && residual_mode.is_lossless()
                            && options.fast_search == VvcFastSearch::LosslessSpeed
                            && vvc_lossless_speed_luma_leaf_inter_skip_allowed(stream_format)
                        {
                            previous_predictive_cache.as_ref().and_then(|cache| {
                                vvc_predictive_chroma_leaf_inter_skip_mask(
                                    &frame_buf,
                                    &cache.source,
                                    stream_frame_layout,
                                    region,
                                    stream_format.chroma_sampling,
                                    ctu_residual_policy.dual_tree_intra(),
                                )
                            })
                        } else {
                            None
                        };
                        let explicit_luma_inter_decisions =
                            if explicit_inter_frame && luma_max_leaf_size == VVC_CURRENT_MAX_LUMA_LEAF_SIZE {
                                explicit_luma_inter_decisions_by_ctu
                                    .get(region.slice_address)
                                    .and_then(Option::as_ref)
                            } else {
                                None
                            };
                        let explicit_inter_reference =
                            explicit_luma_inter_decisions.and_then(|_| {
                                previous_predictive_cache
                                    .as_ref()
                                    .map(|cache| &cache.reconstruction)
                            });
                        let decision = quantize_vvc_ctu_with_luma_leaf_selection(
                            &source_frame,
                            &mut frame_recon,
                            region,
                            ctu_residual_policy,
                            luma_qp,
                            chroma_qp,
                            &mut luma_mode_search_state,
                            &transform_skip_quant_tables,
                            &mut ctu_quant_scratch,
                            luma_max_leaf_size,
                            luma_inter_skip_mask.as_ref(),
                            chroma_inter_skip_mask.as_ref(),
                            explicit_luma_inter_decisions,
                            luma_scc_decisions.as_ref(),
                            explicit_inter_reference,
                            temporal_mode_hint,
                        );
                        #[cfg(feature = "vvc-stats")]
                        if let Some(mask) = luma_inter_skip_mask.as_ref() {
                            frame_stats.add_counter(
                                "predictive_luma_leaf_inter_skip_count",
                                mask.iter().filter(|&&skip| skip).count() as u64,
                            );
                        }
                        #[cfg(feature = "vvc-stats")]
                        if let Some(mask) = chroma_inter_skip_mask.as_ref() {
                            frame_stats.add_counter(
                                "predictive_chroma_leaf_inter_skip_count",
                                mask.iter().filter(|&&skip| skip).count() as u64,
                            );
                        }
                        #[cfg(feature = "vvc-stats")]
                        {
                            let explicit_count =
                                vvc_luma_inter_decision_count(&decision.luma_tu_inter_decisions);
                            if explicit_count > 0 {
                                frame_stats.add_counter(
                                    "predictive_luma_explicit_inter_count",
                                    explicit_count as u64,
                                );
                            }
                        }
                        #[cfg(feature = "vvc-stats")]
                        {
                            add_vvc_quantized_ctu_counters(&mut frame_stats, &decision.quantized);
                            match decision.luma_max_leaf_size {
                                VVC_LOSSLESS_LUMA_LEAF_SIZE => {
                                    frame_stats.add_counter("luma_ctu_leaf4_count", 1);
                                }
                                VVC_CURRENT_MAX_LUMA_LEAF_SIZE => {
                                    frame_stats.add_counter("luma_ctu_leaf8_count", 1);
                                }
                                _ => {}
                            }
                        }
                        #[cfg(feature = "vvc-stats")]
                        if vvc_ctu_bits.is_enabled() {
                            vvc_ctu_bits.write_ctu(
                                frame_idx,
                                geometry,
                                region,
                                stream_format,
                                options.lossless,
                                slice_config.slice_qp,
                                chroma_qp,
                                &decision.quantized,
                                decision.luma_max_leaf_size,
                                slice_config,
                            )?;
                        }
                        if let Some(decisions) = frame_ctu_decisions.as_mut() {
                            let mut payload = vvc_intra_ctu_payload_from_decision(
                                region,
                                &decision,
                                slice_config,
                                luma_inter_skip_mask.as_ref(),
                                chroma_inter_skip_mask.as_ref(),
                                mixed_single_p_slice_frame,
                            )?;
                            if let Some((cached, skip_distortion)) = cached_lossy_skip_ctu {
                                if predictive_ctu_inter_skip_frame
                                    && vvc_predictive_inter_skip_region(region)
                                    && vvc_lossy_predictive_inter_skip_selects_over_intra(
                                        &source_frame,
                                        &frame_recon,
                                        region,
                                        skip_distortion,
                                    )
                                {
                                    frame_recon.copy_ctu_from(&cached.reconstruction, region);
                                    reused_predictive_ctu = true;
                                    inter_skip_ctu = true;
                                    payload = VvcQuantizedCtuPayload::InterSkip;
                                    decisions.push(std::sync::Arc::clone(cached.decision));
                                } else {
                                    decisions.push(std::sync::Arc::new(decision));
                                }
                            } else {
                                decisions.push(std::sync::Arc::new(decision));
                            }
                            payload
                        } else {
                            let VvcQuantizedCtuLeafDecision {
                                quantized,
                                luma_max_leaf_size,
                                luma_tu_inter_decisions: _,
                                ..
                            } = decision;
                            vvc_intra_ctu_payload_from_quantized(
                                region,
                                quantized,
                                luma_max_leaf_size,
                                slice_config,
                                luma_inter_skip_mask.as_ref(),
                                chroma_inter_skip_mask.as_ref(),
                                inter_slice_partition,
                            )?
                        }
                    };
                    if let Some(ibc_search) = ibc_search.as_mut() {
                        if let Some(decisions) = luma_scc_decisions.as_ref() {
                            for decision in decisions.iter().flatten() {
                                ibc_search.record_ibc_decision(*decision);
                            }
                        }
                        let x_end = region
                            .origin_x
                            .saturating_add(region.geometry.width)
                            .min(geometry.width);
                        let y_end = region
                            .origin_y
                            .saturating_add(region.geometry.height)
                            .min(geometry.height);
                        for origin_y in (region.origin_y..y_end).step_by(8) {
                            for origin_x in (region.origin_x..x_end).step_by(8) {
                                ibc_search.record_external_8x8(
                                    &frame_recon,
                                    origin_x,
                                    origin_y,
                                );
                            }
                        }
                    }
                    if region.slice_address < predictive_reused_ctus.len() {
                        predictive_reused_ctus[region.slice_address] = reused_predictive_ctu;
                    }
                    #[cfg(feature = "vvc-stats")]
                    if reused_predictive_ctu {
                        frame_stats.add_counter("predictive_reused_ctu_count", 1);
                    }
                    #[cfg(feature = "vvc-stats")]
                    if cached_exact_ctu.is_some() {
                        frame_stats.add_counter("predictive_exact_ctu_count", 1);
                    }
                    #[cfg(feature = "vvc-stats")]
                    if inter_skip_ctu {
                        frame_stats.add_counter("predictive_inter_skip_ctu_count", 1);
                    }
                    #[cfg(feature = "vvc-stats")]
                    if cached_lossy_skip_ctu.is_some() {
                        frame_stats.add_counter("predictive_lossy_near_skip_ctu_count", 1);
                    }
                    #[cfg(feature = "vvc-stats")]
                    if preselected_lossy_inter_skip_ctu.is_some() {
                        frame_stats.add_counter("predictive_lossy_zero_sse_preskip_ctu_count", 1);
                    }
                    #[cfg(feature = "vvc-stats")]
                    if temporal_mode_hint.is_some() && !reused_predictive_ctu {
                        frame_stats.add_counter("predictive_temporal_mode_hint_ctu_count", 1);
                    }
                    #[cfg(feature = "vvc-stats")]
                    if cached_exact_ctu_available && !inter_skip_ctu && !intra_reuse_allowed {
                        frame_stats.add_counter("predictive_dependency_blocked_ctu_count", 1);
                    }
                    #[cfg(feature = "vvc-stats")]
                    frame_stats.add_elapsed("ctu_quantize", stage_start);
                    frame_ctus.push(VvcQuantizedCtu {
                        slice_address: region.slice_address,
                        geometry: region.geometry,
                        payload,
                    });
                }
                #[cfg(feature = "vvc-stats")]
                let stage_start = StageStart::now();
                #[cfg(feature = "vvc-stats")]
                let entropy_build_start = StageStart::now();
                let predictive_frame_skip = predictive_frame
                    && frame_ctus
                        .iter()
                        .all(|ctu| matches!(ctu.payload, VvcQuantizedCtuPayload::InterSkip));
                #[cfg(feature = "vvc-stats")]
                if predictive_frame_skip {
                    frame_stats.add_counter("predictive_frame_skip_count", 1);
                }
                let frame_slice_units = if predictive_enabled && !predictive_frame {
                    vec![vvc_predictive_frame_slice_unit(
                        frame_idx,
                        geometry,
                        &frame_ctus,
                        predictive_single_slice_config
                            .expect("predictive single-slice config is available in predictive mode"),
                    )?]
                } else if predictive_frame_skip {
                    vec![vvc_predictive_frame_slice_unit(
                        frame_idx,
                        geometry,
                        &frame_ctus,
                        predictive_single_slice_config.expect(
                            "predictive single-slice config is available in predictive mode",
                        ),
                    )?]
                } else if predictive_ctu_slice_frame {
                    vvc_predictive_ctu_slice_units_with_inter_skip_cache(
                        frame_idx,
                        geometry,
                        &frame_ctus,
                        slice_config,
                        &mut ctu_inter_skip_slice_payload_cache,
                    )?
                } else if predictive_enabled {
                    vec![vvc_predictive_frame_slice_unit(
                        frame_idx,
                        geometry,
                        &frame_ctus,
                        predictive_single_slice_config
                            .expect("predictive single-slice config is available in predictive mode"),
                    )?]
                } else {
                    vec![vvc_frame_slice_unit(
                        frame_idx,
                        geometry,
                        &frame_ctus,
                        slice_config,
                    )?]
                };
                #[cfg(feature = "vvc-stats")]
                {
                    frame_stats.add_counter("slice_count", frame_slice_units.len() as u64);
                    frame_stats.add_counter(
                        "single_slice_frame",
                        u64::from(frame_slice_units.len() == 1),
                    );
                }
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_counter(
                    "frame_entropy_build_nanos",
                    entropy_build_start.elapsed().as_nanos() as u64,
                );
                #[cfg(feature = "vvc-stats")]
                let annexb_write_start = StageStart::now();
                write_annex_b_to(&mut frame_bitstream, &frame_slice_units)?;
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_counter(
                    "frame_annexb_write_nanos",
                    annexb_write_start.elapsed().as_nanos() as u64,
                );
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_elapsed("frame_entropy_write", stage_start);
                #[cfg(feature = "vvc-stats")]
                let stage_start = StageStart::now();
                let yuv = frame_recon.to_yuv();
                let next_predictive_cache =
                    frame_ctu_decisions.map(|ctu_decisions| std::sync::Arc::new(VvcPredictiveFrameCache {
                        source: frame_buf.clone(),
                        reconstruction: frame_recon,
                        ctu_decisions,
                    }));
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_elapsed("frame_recon_finalize", stage_start);
                (yuv, next_predictive_cache)
            };
            (
                frame_recon_yuv,
                frame_bitstream.bytes_written(),
                next_predictive_cache,
            )
        }
            };
        previous_predictive_cache = next_predictive_cache;
        total_bitstream_bytes += frame_bitstream_bytes;
        #[cfg(feature = "vvc-stats")]
        frame_stats.set_bitstream_bytes(frame_bitstream_bytes);
        if let Some(writer) = reconstruction.as_deref_mut() {
            #[cfg(feature = "vvc-stats")]
            let stage_start = StageStart::now();
            writer.write_all(&frame_recon_yuv).map_err(|err| {
                format!("failed to write VVC reconstruction frame {frame_idx}: {err}")
            })?;
            #[cfg(feature = "vvc-stats")]
            frame_stats.add_elapsed("write_reconstruction", stage_start);
        }
        if let Some(frame_metrics) = frame_metrics.as_deref_mut() {
            #[cfg(feature = "vvc-stats")]
            let stage_start = StageStart::now();
            frame_metrics(VvcEncodeFrameMetrics {
                frame_idx,
                frame_count: frame_limit.metric_count(),
                bitstream_bytes: frame_bitstream_bytes,
                total_bitstream_bytes,
                encode_elapsed: frame_encode_start.elapsed(),
                source: &frame_buf,
                reconstruction: &frame_recon_yuv,
            });
            #[cfg(feature = "vvc-stats")]
            frame_stats.add_elapsed("frame_metrics", stage_start);
        }
        #[cfg(feature = "vvc-stats")]
        vvc_stats.write_frame(&frame_stats)?;
        frame_idx += 1;
    }

    if let FrameLimit::Exact(frames) = frame_limit {
        let mut extra = [0; 1];
        match input.read(&mut extra) {
            Ok(0) => Ok(()),
            Ok(_) => Err(format!(
                "VVC input contains trailing bytes after {} frame(s)",
                frames
            )),
            Err(err) => Err(format!("failed to check VVC input length: {err}")),
        }
    } else {
        Ok(())
    }
}

fn vvc_intra_ctu_payload_from_decision(
    region: VvcCtuRegion,
    decision: &VvcQuantizedCtuLeafDecision,
    slice_config: VvcSliceSyntaxConfig,
    luma_inter_skip: Option<&[bool; MAX_VVC_LUMA_TUS]>,
    chroma_inter_skip: Option<&[bool; MAX_VVC_CHROMA_TUS]>,
    inter_slice_partition: bool,
) -> Result<VvcQuantizedCtuPayload, String> {
    let mut payload = vvc_intra_ctu_payload_from_quantized(
        region,
        decision.quantized.clone(),
        decision.luma_max_leaf_size,
        slice_config,
        luma_inter_skip,
        chroma_inter_skip,
        inter_slice_partition,
    )?;
    if let VvcQuantizedCtuPayload::Intra(params) = &mut payload {
        params.luma_tu_inter_decisions = decision.luma_tu_inter_decisions;
    }
    Ok(payload)
}

fn vvc_intra_ctu_payload_from_quantized(
    region: VvcCtuRegion,
    quantized: VvcQuantizedColor,
    luma_max_leaf_size: u16,
    slice_config: VvcSliceSyntaxConfig,
    luma_inter_skip: Option<&[bool; MAX_VVC_LUMA_TUS]>,
    chroma_inter_skip: Option<&[bool; MAX_VVC_CHROMA_TUS]>,
    inter_slice_partition: bool,
) -> Result<VvcQuantizedCtuPayload, String> {
    let Some(mut params) = vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma_for_kind(
        region.geometry,
        quantized,
        luma_max_leaf_size,
        slice_config.coding_tree.chroma_sampling,
        slice_config.coding_tree.dual_tree_intra && !inter_slice_partition,
        if inter_slice_partition {
            VvcLumaSplitAvailabilityKind::Inter
        } else {
            VvcLumaSplitAvailabilityKind::Intra
        },
    ) else {
        return Err(format!(
            "VVC frame CABAC CTU {} has unsupported coded geometry {}x{}",
            region.slice_address,
            region.geometry.coded_width(),
            region.geometry.coded_height()
        ));
    };
    if let Some(luma_inter_skip) = luma_inter_skip {
        params.luma_tu_inter_skip = *luma_inter_skip;
    }
    if let Some(chroma_inter_skip) = chroma_inter_skip {
        params.chroma_tu_inter_skip = *chroma_inter_skip;
    }
    Ok(VvcQuantizedCtuPayload::Intra(Box::new(params)))
}
