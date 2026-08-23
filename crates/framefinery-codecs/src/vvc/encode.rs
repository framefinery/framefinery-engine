pub fn eos_annex_b() -> Vec<u8> {
    write_annex_b(&[VvcNalUnit::eos()]).expect("hard-coded EOS NAL should be valid")
}

const VVC_PREDICTIVE_SINGLE_SLICE_PPS_ID: u8 = 1;
#[cfg(test)]
const VVC_PREDICTIVE_FRAME_SKIP_PPS_ID: u8 = VVC_PREDICTIVE_SINGLE_SLICE_PPS_ID;
const VVC_LOSSY_PREDICTIVE_SKIP_MAX_ABS_8BIT: u16 = 2;

pub fn vvc_black_yuv420p8_annex_b(params: VvcEncodeParams) -> Result<Vec<u8>, String> {
    validate_vvc_exact_frame_count(params)?;
    let input = vec![0; Picture::expected_len(8, 8, PixelFormat::Yuv420p8) * params.frames];
    vvc_yuv420p8_annex_b_from_input(&input, params)
}

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
        #[cfg(feature = "vvc-stats")]
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
            !residual_mode.is_lossless() && vvc_lossy_mixed_single_p_slice_supported(stream_format);
        let predictive_ctu_inter_skip_enabled = predictive_frame
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
        let predictive_ctu_inter_skip_frame = predictive_ctu_skip_candidate_count > 0
            && (residual_mode.is_lossless()
                || vvc_lossy_predictive_ctu_skip_candidate_count_allows_mixed_p_slice(
                    predictive_ctu_skip_candidate_count,
                    ctu_count,
                ));
        let predictive_ctu_slice_frame =
            !lossy_mixed_single_p_slice_supported && predictive_ctu_inter_skip_frame;
        let mixed_single_p_slice_frame =
            lossy_mixed_single_p_slice_supported && predictive_ctu_inter_skip_frame;
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
                for region in vvc_ctu_regions(geometry) {
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
                                "predictive_luma_motion_nonzero_exact_8x8_count",
                                motion_analysis.nonzero_exact_count as u64,
                            );
                            frame_stats.add_counter(
                                "predictive_luma_motion_near_8x8_count",
                                motion_analysis.near_count as u64,
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
                            )?
                        }
                    } else {
                        if predictive_ctu_slice_frame {
                            frame_recon.clear_availability();
                            luma_mode_search_state.clear();
                        }
                        let ctu_residual_policy = residual_policy
                            .with_dual_tree_intra(!mixed_single_p_slice_frame);
                        let luma_max_leaf_size = select_vvc_luma_max_leaf_size_for_ctu(
                            ctu_residual_policy,
                            &source_frame,
                            region,
                            luma_qp,
                        );
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
                            } = decision;
                            vvc_intra_ctu_payload_from_quantized(
                                region,
                                quantized,
                                luma_max_leaf_size,
                                slice_config,
                                luma_inter_skip_mask.as_ref(),
                                chroma_inter_skip_mask.as_ref(),
                            )?
                        }
                    };
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

fn write_annex_b_to<W: Write + ?Sized>(
    output: &mut W,
    units: &[VvcNalUnit],
) -> Result<usize, String> {
    let bytes = write_annex_b(units)?;
    let len = bytes.len();
    output
        .write_all(&bytes)
        .map_err(|err| format!("failed to write VVC Annex-B stream: {err}"))?;
    Ok(len)
}

fn vvc_intra_ctu_payload_from_decision(
    region: VvcCtuRegion,
    decision: &VvcQuantizedCtuLeafDecision,
    slice_config: VvcSliceSyntaxConfig,
    luma_inter_skip: Option<&[bool; MAX_VVC_LUMA_TUS]>,
    chroma_inter_skip: Option<&[bool; MAX_VVC_CHROMA_TUS]>,
) -> Result<VvcQuantizedCtuPayload, String> {
    vvc_intra_ctu_payload_from_quantized(
        region,
        decision.quantized.clone(),
        decision.luma_max_leaf_size,
        slice_config,
        luma_inter_skip,
        chroma_inter_skip,
    )
}

fn vvc_intra_ctu_payload_from_quantized(
    region: VvcCtuRegion,
    quantized: VvcQuantizedColor,
    luma_max_leaf_size: u16,
    slice_config: VvcSliceSyntaxConfig,
    luma_inter_skip: Option<&[bool; MAX_VVC_LUMA_TUS]>,
    chroma_inter_skip: Option<&[bool; MAX_VVC_CHROMA_TUS]>,
) -> Result<VvcQuantizedCtuPayload, String> {
    let Some(mut params) = vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma(
        region.geometry,
        quantized,
        luma_max_leaf_size,
        slice_config.coding_tree.chroma_sampling,
        slice_config.coding_tree.dual_tree_intra,
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

#[derive(Debug, Clone)]
struct VvcPredictiveFrameCache {
    source: Vec<u8>,
    reconstruction: VvcReconstructionFrame,
    ctu_decisions: Vec<std::sync::Arc<VvcQuantizedCtuLeafDecision>>,
}

#[derive(Debug, Clone, Copy)]
struct VvcReusableCtuDecision<'a> {
    reconstruction: &'a VvcReconstructionFrame,
    decision: &'a std::sync::Arc<VvcQuantizedCtuLeafDecision>,
}

impl VvcPredictiveFrameCache {
    fn ctu_decision(&self, region: VvcCtuRegion) -> Option<&VvcQuantizedCtuLeafDecision> {
        self.ctu_decisions
            .get(region.slice_address)
            .map(std::sync::Arc::as_ref)
    }

    fn reusable_decision(&self, region: VvcCtuRegion) -> Option<VvcReusableCtuDecision<'_>> {
        let decision = self.ctu_decisions.get(region.slice_address)?;
        Some(VvcReusableCtuDecision {
            reconstruction: &self.reconstruction,
            decision,
        })
    }

    fn matching_decision(
        &self,
        current_source: &[u8],
        layout: PlanarYuvFrameLayout,
        region: VvcCtuRegion,
    ) -> Option<VvcReusableCtuDecision<'_>> {
        let reusable = self.reusable_decision(region)?;
        if !layout.regions_equal_between(
            current_source,
            region.origin_x,
            region.origin_y,
            &self.source,
            region.origin_x,
            region.origin_y,
            region.geometry.width,
            region.geometry.height,
        ) {
            return None;
        }
        Some(reusable)
    }

    fn lossy_inter_skip_candidate_distortions(
        &self,
        current_source: &[u8],
        layout: PlanarYuvFrameLayout,
        current_frame: &VvcSampledFrame,
        geometry: VvcVideoGeometry,
    ) -> Vec<Option<u64>> {
        let max_abs_delta =
            vvc_lossy_predictive_skip_max_abs_delta(current_frame.format.bit_depth);
        vvc_ctu_regions(geometry)
            .map(|region| {
                if !vvc_predictive_inter_skip_region(region)
                    || self.ctu_decisions.get(region.slice_address).is_none()
                {
                    return None;
                }
                if layout.regions_equal_between(
                    current_source,
                    region.origin_x,
                    region.origin_y,
                    &self.source,
                    region.origin_x,
                    region.origin_y,
                    region.geometry.width,
                    region.geometry.height,
                ) {
                    return Some(vvc_region_sse_against_reconstruction(
                        current_frame,
                        &self.reconstruction,
                        region,
                    ));
                }
                vvc_predictive_lossy_region_sse_if_within_reconstruction_delta(
                    current_frame,
                    &self.reconstruction,
                    region,
                    max_abs_delta,
                )
            })
            .collect()
    }
}

fn vvc_predictive_frame_lossless_ctu_inter_skip_candidate_count(
    previous_cache: Option<&VvcPredictiveFrameCache>,
    current_source: &[u8],
    layout: PlanarYuvFrameLayout,
    geometry: VvcVideoGeometry,
) -> usize {
    let Some(cache) = previous_cache else {
        return 0;
    };
    vvc_ctu_regions(geometry)
        .filter(|&region| {
            vvc_predictive_inter_skip_region(region)
                && cache
                    .matching_decision(current_source, layout, region)
                    .is_some()
        })
        .count()
}

fn vvc_lossy_predictive_ctu_skip_candidate_count_allows_mixed_p_slice(
    candidate_count: usize,
    ctu_count: usize,
) -> bool {
    candidate_count.saturating_mul(2) >= ctu_count.max(1)
}

fn vvc_lossy_mixed_single_p_slice_supported(format: VvcPictureFormat) -> bool {
    // Mixed InterSkip/Intra P-slices use a single coding tree. The quantizer can
    // follow that partition order for 4:2:0 and 8-bit 4:4:4. Keep 4:2:2 and
    // high-depth 4:4:4 on the CTU-slice fallback until their rectangular/chroma
    // residual tradeoffs are validated.
    format.chroma_sampling == ChromaSampling::Cs420
        || (format.chroma_sampling == ChromaSampling::Cs444 && format.bit_depth.bits() == 8)
}

fn vvc_predictive_ctu_dependencies_reused(
    region: VvcCtuRegion,
    ctu_cols: usize,
    reused_ctus: &[bool],
) -> bool {
    let left_reused = region.origin_x == 0
        || region
            .slice_address
            .checked_sub(1)
            .and_then(|idx| reused_ctus.get(idx))
            .copied()
            .unwrap_or(false);
    let above_reused = region.origin_y == 0
        || region
            .slice_address
            .checked_sub(ctu_cols)
            .and_then(|idx| reused_ctus.get(idx))
            .copied()
            .unwrap_or(false);
    left_reused && above_reused
}

fn vvc_lossless_speed_luma_leaf_inter_skip_allowed(format: VvcPictureFormat) -> bool {
    let _format = format;
    // Leaf-level predictive skip needs legal inter-slice local-separate-tree
    // handling for small 4:2:0 intra leaves before it can share the mixed
    // P-slice CTU path. Keep the release path reference-clean by limiting VVC
    // predictive reuse to complete CTUs.
    false
}

fn vvc_predictive_intra_ctu_reuse_enabled_for_mode(mode: VvcResidualCodingMode) -> bool {
    // Exact-source CTU reuse is reference-clean for lossless streams because
    // the reconstructed CTU equals the source CTU. For lossy streams, reusing
    // a previous frame's quantized intra decisions can drift from VTM once
    // the same source CTU is coded in a later predictive picture; keep lossy
    // predictive GOP syntax enabled but re-quantize each CTU until that path
    // is modelled and covered.
    mode.is_lossless()
}

fn vvc_predictive_frame_inter_skip_enabled_for_reference_clean_release() -> bool {
    // Full-frame repeated P pictures use a single all-skip slice and
    // initialize the CABAC state as a P slice before emitting split/skip
    // syntax.
    true
}

fn vvc_predictive_ctu_inter_skip_enabled_for_reference_clean_release() -> bool {
    // Lossy CTU-level InterSkip now emits skipped and non-skipped CTUs in one
    // P-slice, using inter-slice split constraints plus single-tree intra
    // transform_unit syntax for the non-skipped CTUs. Lossless mixed CTU skip
    // still uses CTU slices because the 4x4 4:2:0 local-separate-tree branch is
    // not wired yet.
    true
}

fn vvc_lossy_predictive_ctu_inter_skip_enabled_for_reference_clean_release() -> bool {
    // Lossy CTU InterSkip is only enabled after normal CTU quantization and a
    // conservative RD gate. The pre-scan still requires enough skip candidates
    // to justify switching the frame to mixed P-slice output.
    true
}

fn vvc_lossy_predictive_inter_skip_selects_over_intra(
    source_frame: &VvcSampledFrame,
    intra_reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
    skip_distortion: u64,
) -> bool {
    let intra_distortion =
        vvc_region_sse_against_reconstruction(source_frame, intra_reconstruction, region);
    skip_distortion <= intra_distortion
}

fn vvc_lossy_predictive_inter_skip_preselected(
    skip_distortion: u64,
    region: VvcCtuRegion,
    format: VvcPictureFormat,
) -> bool {
    skip_distortion <= vvc_lossy_predictive_preskip_max_sse(region, format)
}

const VVC_LOSSY_PREDICTIVE_PRESKIP_AVG_SSE_8BIT: u64 = 8;

fn vvc_lossy_predictive_preskip_max_sse(
    region: VvcCtuRegion,
    format: VvcPictureFormat,
) -> u64 {
    let sample_count =
        vvc_region_visible_luma_sample_count(region) + vvc_region_visible_chroma_sample_count(region, format);
    let scale = 1u64 << u32::from(format.bit_depth.bits().saturating_sub(8));
    sample_count.saturating_mul(
        scale
            .saturating_mul(scale)
            .saturating_mul(VVC_LOSSY_PREDICTIVE_PRESKIP_AVG_SSE_8BIT),
    )
}

fn vvc_region_visible_luma_sample_count(region: VvcCtuRegion) -> u64 {
    (region.geometry.width as u64).saturating_mul(region.geometry.height as u64)
}

fn vvc_region_visible_chroma_sample_count(
    region: VvcCtuRegion,
    format: VvcPictureFormat,
) -> u64 {
    let subsample_x = chroma_subsample_x(format.chroma_sampling) as u64;
    let subsample_y = chroma_subsample_y(format.chroma_sampling) as u64;
    let chroma_width = (region.geometry.width as u64) / subsample_x;
    let chroma_height = (region.geometry.height as u64) / subsample_y;
    chroma_width.saturating_mul(chroma_height).saturating_mul(2)
}

fn vvc_region_sse_against_reconstruction(
    source_frame: &VvcSampledFrame,
    reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
) -> u64 {
    if source_frame.geometry != reconstruction.geometry || source_frame.format != reconstruction.format {
        return u64::MAX;
    }
    let width = region
        .geometry
        .width
        .min(source_frame.geometry.width.saturating_sub(region.origin_x));
    let height = region
        .geometry
        .height
        .min(source_frame.geometry.height.saturating_sub(region.origin_y));
    let mut sse = vvc_plane_region_sse(
        &source_frame.luma,
        source_frame.geometry.width,
        &reconstruction.luma,
        reconstruction.luma_width(),
        region.origin_x,
        region.origin_y,
        width,
        height,
    );
    let subsample_x = chroma_subsample_x(source_frame.format.chroma_sampling);
    let subsample_y = chroma_subsample_y(source_frame.format.chroma_sampling);
    let chroma_x = region.origin_x / subsample_x;
    let chroma_y = region.origin_y / subsample_y;
    let chroma_width = width / subsample_x;
    let chroma_height = height / subsample_y;
    sse = sse.saturating_add(vvc_plane_region_sse(
        &source_frame.cb,
        source_frame.geometry.width / subsample_x,
        &reconstruction.cb,
        reconstruction.chroma_width(),
        chroma_x,
        chroma_y,
        chroma_width,
        chroma_height,
    ));
    sse.saturating_add(vvc_plane_region_sse(
        &source_frame.cr,
        source_frame.geometry.width / subsample_x,
        &reconstruction.cr,
        reconstruction.chroma_width(),
        chroma_x,
        chroma_y,
        chroma_width,
        chroma_height,
    ))
}

fn vvc_plane_region_sse(
    source: &[VvcSample],
    source_stride: usize,
    reconstruction: &[VvcSample],
    reconstruction_stride: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
) -> u64 {
    let mut sse = 0u64;
    for y in 0..height {
        let source_row = (start_y + y) * source_stride + start_x;
        let reconstruction_row = (start_y + y) * reconstruction_stride + start_x;
        for x in 0..width {
            let delta = i64::from(source[source_row + x]) - i64::from(reconstruction[reconstruction_row + x]);
            sse = sse.saturating_add((delta * delta) as u64);
        }
    }
    sse
}

fn vvc_predictive_luma_leaf_inter_skip_mask(
    current_source: &[u8],
    previous_source: &[u8],
    layout: PlanarYuvFrameLayout,
    region: VvcCtuRegion,
    luma_max_leaf_size: u16,
    chroma_sampling: ChromaSampling,
    dual_tree_intra: bool,
) -> Option<[bool; MAX_VVC_LUMA_TUS]> {
    if luma_max_leaf_size < VVC_CURRENT_MAX_LUMA_LEAF_SIZE {
        return None;
    }

    let shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: region.geometry.coded_width() as u16,
        visible_height: region.geometry.coded_height() as u16,
        chroma_sampling,
        dual_tree_intra,
    };
    let nodes = vvc_luma_transform_nodes(shape, luma_max_leaf_size);
    if nodes.is_empty() || nodes.len() > MAX_VVC_LUMA_TUS {
        return None;
    }

    let mut mask = [false; MAX_VVC_LUMA_TUS];
    let mut skipped = 0usize;
    for (idx, node) in nodes.into_iter().enumerate() {
        let origin_x = region.origin_x + usize::from(node.x);
        let origin_y = region.origin_y + usize::from(node.y);
        let width = usize::from(node.width).min(region.geometry.width.saturating_sub(node.x as usize));
        let height =
            usize::from(node.height).min(region.geometry.height.saturating_sub(node.y as usize));
        if width != 0
            && height != 0
            && layout.luma_regions_equal_between(
                current_source,
                origin_x,
                origin_y,
                previous_source,
                origin_x,
                origin_y,
                width,
                height,
            )
        {
            mask[idx] = true;
            skipped += 1;
        }
    }
    (skipped > 0).then_some(mask)
}

fn vvc_predictive_chroma_leaf_inter_skip_mask(
    current_source: &[u8],
    previous_source: &[u8],
    layout: PlanarYuvFrameLayout,
    region: VvcCtuRegion,
    chroma_sampling: ChromaSampling,
    dual_tree_intra: bool,
) -> Option<[bool; MAX_VVC_CHROMA_TUS]> {
    let shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: region.geometry.coded_width() as u16,
        visible_height: region.geometry.coded_height() as u16,
        chroma_sampling,
        dual_tree_intra,
    };
    let nodes = vvc_chroma_transform_nodes(shape);
    if nodes.is_empty() || nodes.len() > MAX_VVC_CHROMA_TUS {
        return None;
    }

    let mut mask = [false; MAX_VVC_CHROMA_TUS];
    let mut skipped = 0usize;
    for (idx, node) in nodes.into_iter().enumerate() {
        let origin_x = region.origin_x + usize::from(node.x);
        let origin_y = region.origin_y + usize::from(node.y);
        let width =
            usize::from(node.width).min(region.geometry.width.saturating_sub(node.x as usize));
        let height =
            usize::from(node.height).min(region.geometry.height.saturating_sub(node.y as usize));
        if width != 0
            && height != 0
            && layout.chroma_regions_equal_between(
                current_source,
                origin_x,
                origin_y,
                previous_source,
                origin_x,
                origin_y,
                width,
                height,
            )
        {
            mask[idx] = true;
            skipped += 1;
        }
    }
    (skipped > 0).then_some(mask)
}

fn vvc_predictive_inter_skip_region(region: VvcCtuRegion) -> bool {
    let coded_width = region.geometry.coded_width();
    let coded_height = region.geometry.coded_height();
    (1..=VVC_CTU_SIZE).contains(&coded_width) && (1..=VVC_CTU_SIZE).contains(&coded_height)
}

fn vvc_lossy_predictive_skip_max_abs_delta(bit_depth: SampleBitDepth) -> u16 {
    VVC_LOSSY_PREDICTIVE_SKIP_MAX_ABS_8BIT << u32::from(bit_depth.bits().saturating_sub(8))
}

fn vvc_predictive_lossy_region_within_reconstruction_delta(
    current_source: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
    max_abs_delta: u16,
) -> bool {
    vvc_predictive_lossy_region_sse_if_within_reconstruction_delta(
        current_source,
        previous_reconstruction,
        region,
        max_abs_delta,
    )
    .is_some()
}

fn vvc_predictive_lossy_region_sse_if_within_reconstruction_delta(
    current_source: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
    max_abs_delta: u16,
) -> Option<u64> {
    if current_source.geometry != previous_reconstruction.geometry
        || current_source.format != previous_reconstruction.format
    {
        return None;
    }

    let width = region
        .geometry
        .width
        .min(current_source.geometry.width.saturating_sub(region.origin_x));
    let height = region
        .geometry
        .height
        .min(current_source.geometry.height.saturating_sub(region.origin_y));
    if width == 0 || height == 0 {
        return None;
    }
    let mut sse = vvc_predictive_plane_region_sse_if_within_delta(
        &current_source.luma,
        &previous_reconstruction.luma,
        current_source.geometry.width,
        previous_reconstruction.luma_width(),
        region.origin_x,
        region.origin_y,
        width,
        height,
        max_abs_delta,
    )?;

    let subsample_x = chroma_subsample_x(current_source.format.chroma_sampling);
    let subsample_y = chroma_subsample_y(current_source.format.chroma_sampling);
    let chroma_x = region.origin_x / subsample_x;
    let chroma_y = region.origin_y / subsample_y;
    let chroma_width = width / subsample_x;
    let chroma_height = height / subsample_y;
    let chroma_stride = current_source.geometry.width / subsample_x;
    let reference_chroma_stride = previous_reconstruction.chroma_width();
    sse = sse.saturating_add(vvc_predictive_plane_region_sse_if_within_delta(
        &current_source.cb,
        &previous_reconstruction.cb,
        chroma_stride,
        reference_chroma_stride,
        chroma_x,
        chroma_y,
        chroma_width,
        chroma_height,
        max_abs_delta,
    )?);
    sse = sse.saturating_add(vvc_predictive_plane_region_sse_if_within_delta(
        &current_source.cr,
        &previous_reconstruction.cr,
        chroma_stride,
        reference_chroma_stride,
        chroma_x,
        chroma_y,
        chroma_width,
        chroma_height,
        max_abs_delta,
    )?);
    Some(sse)
}

fn vvc_predictive_plane_region_sse_if_within_delta(
    current: &[VvcSample],
    reference: &[VvcSample],
    current_stride: usize,
    reference_stride: usize,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    max_abs_delta: u16,
) -> Option<u64> {
    let mut sse = 0u64;
    for y in origin_y..origin_y + height {
        let current_row = y * current_stride;
        let reference_row = y * reference_stride;
        for x in origin_x..origin_x + width {
            let current_sample = current[current_row + x];
            let reference_sample = reference[reference_row + x];
            if current_sample.abs_diff(reference_sample) > max_abs_delta {
                return None;
            }
            let delta = i64::from(current_sample) - i64::from(reference_sample);
            sse = sse.saturating_add((delta * delta) as u64);
        }
    }
    Some(sse)
}
