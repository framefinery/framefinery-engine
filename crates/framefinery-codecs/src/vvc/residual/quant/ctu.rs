pub fn quantize_vvc_color(color: VvcSampledColor) -> VvcQuantizedColor {
    quantize_vvc_frame(&VvcSampledFrame::solid(color))
}

pub(in crate::vvc) fn quantize_vvc_frame(frame: &VvcSampledFrame) -> VvcQuantizedColor {
    quantize_vvc_frame_with_reconstruction(frame).quantized
}

pub(in crate::vvc) fn quantize_vvc_frame_with_reconstruction(
    frame: &VvcSampledFrame,
) -> VvcQuantizedResidualFrame {
    let mut reconstruction = VvcReconstructionFrame::new_neutral(frame.geometry, frame.format);
    let region = VvcCtuRegion {
        slice_address: 0,
        origin_x: 0,
        origin_y: 0,
        geometry: frame.geometry,
    };
    let quantized = quantize_vvc_residual_ctu_into_frame_reconstruction(
        frame,
        &mut reconstruction,
        region,
        VvcResidualCodingMode::Lossy,
    );
    let mut reconstruction_yuv =
        Vec::with_capacity(frame.geometry.luma_samples() + frame.chroma_len * 2);
    reconstruction_yuv.extend_from_slice(&reconstruction.luma);
    reconstruction_yuv.extend_from_slice(&reconstruction.cb);
    reconstruction_yuv.extend_from_slice(&reconstruction.cr);
    VvcQuantizedResidualFrame {
        quantized,
        reconstruction_yuv,
    }
}

pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    residual_mode: VvcResidualCodingMode,
) -> VvcQuantizedColor {
    let policy = VvcResidualCodingPolicy::new(source_frame.format, residual_mode);
    let (luma_qp, chroma_qp) = match residual_mode {
        VvcResidualCodingMode::Lossless => {
            let qp = super::super::vvc_lossless_slice_qp(source_frame.format.bit_depth);
            (qp, qp)
        }
        VvcResidualCodingMode::Lossy => (
            super::VVC_DEFAULT_LOSSY_LUMA_QP,
            super::VVC_DEFAULT_LOSSY_CHROMA_QP,
        ),
    };
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp(
        source_frame,
        frame_recon,
        region,
        policy,
        luma_qp,
        chroma_qp,
    )
}

pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    luma_qp: i32,
    chroma_qp: i32,
) -> VvcQuantizedColor {
    let mut luma_mode_search_state =
        VvcLumaModeSearchState::new_for_geometry(source_frame.geometry);
    let transform_skip_quant_tables =
        VvcTransformSkipQuantTables::new(source_frame.format.bit_depth, luma_qp, chroma_qp);
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes(
        source_frame,
        frame_recon,
        region,
        policy,
        luma_qp,
        chroma_qp,
        &mut luma_mode_search_state,
        &transform_skip_quant_tables,
    )
}

pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    luma_qp: i32,
    chroma_qp: i32,
    luma_mode_search_state: &mut VvcLumaModeSearchState,
    transform_skip_quant_tables: &VvcTransformSkipQuantTables,
) -> VvcQuantizedColor {
    let mut luma_tu_remainders = [0; MAX_VVC_LUMA_TUS];
    let mut luma_tu_negative = [false; MAX_VVC_LUMA_TUS];
    let mut luma_tu_dc_levels = [0; MAX_VVC_LUMA_TUS];
    let mut luma_tu_intra_modes = [VvcIntraPredictionMode::Dc; MAX_VVC_LUMA_TUS];
    let mut luma_tu_ac_levels = [[0; VVC_LUMA_AC_COEFFS_PER_TU]; MAX_VVC_LUMA_TUS];
    let mut luma_tu_has_ac = [false; MAX_VVC_LUMA_TUS];
    let mut luma_tu_transform_skip = [false; MAX_VVC_LUMA_TUS];
    let mut luma_tu_bdpcm_modes = [VvcBdpcmMode::None; MAX_VVC_LUMA_TUS];
    let mut luma_tu_mrl_index = [0; MAX_VVC_LUMA_TUS];
    let mut luma_tu_mts_index = [0; MAX_VVC_LUMA_TUS];
    let mut cb_tu_dc_levels = [0; MAX_VVC_CHROMA_TUS];
    let mut cr_tu_dc_levels = [0; MAX_VVC_CHROMA_TUS];
    let mut cb_tu_ac_levels = [[0; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS];
    let mut cr_tu_ac_levels = [[0; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS];
    let mut cb_tu_has_ac = [false; MAX_VVC_CHROMA_TUS];
    let mut cr_tu_has_ac = [false; MAX_VVC_CHROMA_TUS];
    let mut cb_tu_transform_skip = [false; MAX_VVC_CHROMA_TUS];
    let mut cr_tu_transform_skip = [false; MAX_VVC_CHROMA_TUS];
    let mut chroma_tu_bdpcm_modes = [VvcBdpcmMode::None; MAX_VVC_CHROMA_TUS];
    let mut chroma_tu_intra_modes = [VvcChromaIntraPredictionMode::Derived; MAX_VVC_CHROMA_TUS];
    let mut prediction_scratch = VvcDcPredictionScratch::default();
    let mut predicted_luma = Vec::new();
    let mut predicted_cb = Vec::new();
    let mut predicted_cr = Vec::new();
    let mut transform_scratch = VvcInverseTransformScratch::default();
    let mut reconstructed_residual = Vec::new();
    let mut luma_residuals = Vec::new();
    let mut candidate_luma_prediction = Vec::new();
    let mut candidate_luma_residuals = Vec::new();
    let mut luma_rd_cache = VvcLumaModeRdCache::new();
    let mut cb_residuals = Vec::new();
    let mut cr_residuals = Vec::new();
    let mut candidate_cb_prediction = Vec::new();
    let mut candidate_cr_prediction = Vec::new();
    let mut candidate_cb_residuals = Vec::new();
    let mut candidate_cr_residuals = Vec::new();
    let mut chroma_rd_cache = VvcChromaModeRdCache::new();
    #[cfg(feature = "vvc-stats")]
    let mut intra_search_stats = VvcIntraSearchStats::default();
    #[cfg(not(feature = "vvc-stats"))]
    let mut intra_search_stats = VvcIntraSearchStats;
    #[cfg(feature = "vvc-stats")]
    let mut residual_energy_stats = VvcResidualEnergyStats::default();
    #[cfg(feature = "vvc-stats")]
    let mut tu_trace_sink = vvc_tu_trace_sink();

    let score_metric = policy.score_metric();
    let chroma_syntax_tie_breaker = policy.chroma_syntax_tie_breaker();
    let luma_max_leaf_size = policy.luma_max_leaf_size();
    let luma_ts_quant = transform_skip_quant_tables.luma();
    let chroma_ts_quant = transform_skip_quant_tables.chroma();
    let ctu_shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: region.geometry.coded_width() as u16,
        visible_height: region.geometry.coded_height() as u16,
        chroma_sampling: source_frame.format.chroma_sampling,
        dual_tree_intra: true,
    };

    let mut luma_tu_count = 0usize;
    let luma_nodes = vvc_luma_transform_nodes(ctu_shape, luma_max_leaf_size);
    for local_node in luma_nodes.iter().copied() {
        if luma_tu_count >= MAX_VVC_LUMA_TUS {
            break;
        }
        let node = vvc_global_ctu_node(local_node, region);
        luma_rd_cache.reset(policy, node);
        let left_luma_mode = luma_mode_search_state.left_of(node);
        let above_luma_mode = luma_mode_search_state.above_of(node);
        #[cfg(feature = "vvc-stats")]
        let luma_mode_search_start = Instant::now();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_luma_intra_block_into_with_availability(
            &mut predicted_luma,
            &mut prediction_scratch,
            VvcIntraPredictionMode::Dc,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.bit_depth,
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_prediction_nanos(
            VvcLumaPredictionStatsFamily::Dc,
            vvc_elapsed_nanos(prediction_start),
        );
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let dc_score = score_luma_mode_candidate(
            &mut luma_rd_cache,
            score_metric,
            VvcIntraPredictionMode::Dc,
            source_frame,
            node,
            &predicted_luma,
            left_luma_mode,
            above_luma_mode,
            &mut candidate_luma_residuals,
            &mut intra_search_stats,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
        let mut best_luma_mode = VvcIntraPredictionMode::Dc;
        let mut best_luma_score = dc_score;
        let mut luma_candidate_costs = VvcLumaIntraCandidateCosts::new(dc_score);
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_dc();
        if policy.luma_planar_candidate_allowed(node) {
            #[cfg(feature = "vvc-stats")]
            let prediction_start = Instant::now();
            predict_vvc_luma_intra_block_into_with_availability(
                &mut candidate_luma_prediction,
                &mut prediction_scratch,
                VvcIntraPredictionMode::Planar,
                &frame_recon.luma,
                source_frame.geometry,
                node,
                source_frame.format.bit_depth,
                Some(frame_recon.luma_availability()),
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_prediction_nanos(
                VvcLumaPredictionStatsFamily::Planar,
                vvc_elapsed_nanos(prediction_start),
            );
            #[cfg(feature = "vvc-stats")]
            let score_start = Instant::now();
            let candidate_score = score_luma_mode_candidate(
                &mut luma_rd_cache,
                score_metric,
                VvcIntraPredictionMode::Planar,
                source_frame,
                node,
                &candidate_luma_prediction,
                left_luma_mode,
                above_luma_mode,
                &mut candidate_luma_residuals,
                &mut intra_search_stats,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_planar();
            luma_candidate_costs = luma_candidate_costs
                .with_candidate(VvcIntraPredictionMode::Planar, Some(candidate_score));
            if candidate_score < best_luma_score {
                best_luma_score = candidate_score;
                best_luma_mode = VvcIntraPredictionMode::Planar;
                std::mem::swap(&mut predicted_luma, &mut candidate_luma_prediction);
            }
        }
        if policy.luma_directional_candidate_allowed(node)
            && !vvc_luma_exact_min_syntax_mode_search_done(best_luma_score)
        {
            let mut luma_directional_candidates = vvc_luma_directional_search_candidates(
                policy,
                source_frame,
                &luma_mode_search_state,
                node,
            );
            for mode in luma_directional_candidates.iter() {
                #[cfg(feature = "vvc-stats")]
                let prediction_start = Instant::now();
                predict_vvc_luma_intra_block_into_with_availability(
                    &mut candidate_luma_prediction,
                    &mut prediction_scratch,
                    mode,
                    &frame_recon.luma,
                    source_frame.geometry,
                    node,
                    source_frame.format.bit_depth,
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_luma_prediction_nanos(
                    VvcLumaPredictionStatsFamily::Directional,
                    vvc_elapsed_nanos(prediction_start),
                );
                #[cfg(feature = "vvc-stats")]
                let score_start = Instant::now();
                let candidate_score = score_luma_mode_candidate(
                    &mut luma_rd_cache,
                    score_metric,
                    mode,
                    source_frame,
                    node,
                    &candidate_luma_prediction,
                    left_luma_mode,
                    above_luma_mode,
                    &mut candidate_luma_residuals,
                    &mut intra_search_stats,
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_luma_directional_coarse();
                luma_candidate_costs =
                    luma_candidate_costs.with_candidate(mode, Some(candidate_score));
                if candidate_score < best_luma_score {
                    best_luma_score = candidate_score;
                    best_luma_mode = mode;
                    std::mem::swap(&mut predicted_luma, &mut candidate_luma_prediction);
                }
                if vvc_luma_exact_min_syntax_mode_search_done(best_luma_score) {
                    break;
                }
            }
            if (2..=66).contains(&best_luma_mode.luma_mode_index())
                && !vvc_luma_exact_min_syntax_mode_search_done(best_luma_score)
            {
                let refinement_start = luma_directional_candidates.count();
                luma_directional_candidates.add_refinement(best_luma_mode.luma_mode_index());
                for mode in luma_directional_candidates.iter_from(refinement_start) {
                    #[cfg(feature = "vvc-stats")]
                    let prediction_start = Instant::now();
                    predict_vvc_luma_intra_block_into_with_availability(
                        &mut candidate_luma_prediction,
                        &mut prediction_scratch,
                        mode,
                        &frame_recon.luma,
                        source_frame.geometry,
                        node,
                        source_frame.format.bit_depth,
                        Some(frame_recon.luma_availability()),
                    );
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats.add_luma_prediction_nanos(
                        VvcLumaPredictionStatsFamily::Directional,
                        vvc_elapsed_nanos(prediction_start),
                    );
                    #[cfg(feature = "vvc-stats")]
                    let score_start = Instant::now();
                    let candidate_score = score_luma_mode_candidate(
                        &mut luma_rd_cache,
                        score_metric,
                        mode,
                        source_frame,
                        node,
                        &candidate_luma_prediction,
                        left_luma_mode,
                        above_luma_mode,
                        &mut candidate_luma_residuals,
                        &mut intra_search_stats,
                    );
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats.add_luma_directional_refinement();
                    luma_candidate_costs =
                        luma_candidate_costs.with_candidate(mode, Some(candidate_score));
                    if candidate_score < best_luma_score {
                        best_luma_score = candidate_score;
                        best_luma_mode = mode;
                        std::mem::swap(&mut predicted_luma, &mut candidate_luma_prediction);
                    }
                    if vvc_luma_exact_min_syntax_mode_search_done(best_luma_score) {
                        break;
                    }
                }
            }
        }
        let raw_luma_mode = policy.select_luma_intra_mode(node, luma_candidate_costs);
        debug_assert_eq!(raw_luma_mode, best_luma_mode);
        let _best_luma_score = best_luma_score;
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_luma_mode_search_nanos(luma_mode_search_start.elapsed().as_nanos() as u64);
        if let Some(cached) = luma_rd_cache.get(raw_luma_mode) {
            luma_residuals.clear();
            luma_residuals.extend_from_slice(&cached.residuals);
        } else {
            #[cfg(feature = "vvc-stats")]
            let residual_start = Instant::now();
            residual_luma_tu_at_into(
                &mut luma_residuals,
                source_frame,
                usize::from(node.x),
                usize::from(node.y),
                usize::from(node.width),
                usize::from(node.height),
                &predicted_luma,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        }
        #[cfg(feature = "vvc-stats")]
        let luma_rd_start = Instant::now();
        let selected_luma_mode = select_vvc_luma_mode_with_rd_refinement(
            policy,
            node,
            raw_luma_mode,
            luma_candidate_costs,
            &luma_rd_cache,
            &mut intra_search_stats,
            left_luma_mode,
            above_luma_mode,
            source_frame,
            frame_recon,
            luma_qp,
            luma_ts_quant,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_rd_refinement_nanos(luma_rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_luma_mode.residual.is_some() {
            intra_search_stats.add_luma_rd_refinement_attempt();
            if selected_luma_mode.mode != raw_luma_mode {
                intra_search_stats.add_luma_rd_refinement_switch();
            }
        }
        let mut luma_mode = selected_luma_mode.mode;
        let mut luma_coding_decision = policy.select_luma_tu_coding_decision(node, luma_mode);
        #[cfg(feature = "vvc-stats")]
        let luma_mrl_start = Instant::now();
        let selected_luma_mrl = select_vvc_luma_mrl_prediction(
            policy,
            luma_coding_decision.residual_coding,
            luma_coding_decision.mts_index,
            node,
            luma_mode,
            left_luma_mode,
            above_luma_mode,
            luma_qp,
            luma_ts_quant,
            selected_luma_mode.residual,
            &mut intra_search_stats,
            frame_recon,
            source_frame,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_mrl_nanos(luma_mrl_start.elapsed().as_nanos() as u64);
        luma_coding_decision.mrl_index = selected_luma_mrl.mrl_index;
        let mut selected_luma_residual = selected_luma_mrl.residual;
        #[cfg(feature = "vvc-stats")]
        let luma_bdpcm_start = Instant::now();
        if let Some(selected_bdpcm) = select_vvc_luma_bdpcm_prediction(
            policy,
            node,
            luma_mode,
            luma_coding_decision,
            left_luma_mode,
            above_luma_mode,
            luma_qp,
            luma_ts_quant,
            selected_luma_residual,
            &mut intra_search_stats,
            frame_recon,
            source_frame,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        ) {
            luma_mode = selected_bdpcm.mode;
            luma_coding_decision = selected_bdpcm.coding_decision;
            selected_luma_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_bdpcm_nanos(luma_bdpcm_start.elapsed().as_nanos() as u64);
        luma_tu_intra_modes[luma_tu_count] = luma_mode;
        luma_mode_search_state.mark_node(node, luma_mode);
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats.add_luma_residuals(
            &luma_residuals,
            usize::from(node.width),
            usize::from(node.height),
        );
        #[cfg(feature = "vvc-stats")]
        let luma_finalize_start = Instant::now();
        let luma_tu = finalize_vvc_luma_tu(
            luma_coding_decision,
            source_frame,
            frame_recon,
            node,
            &predicted_luma,
            &luma_residuals,
            luma_qp,
            luma_ts_quant,
            selected_luma_residual,
            &mut intra_search_stats,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_finalize_nanos(luma_finalize_start.elapsed().as_nanos() as u64);
        luma_tu_remainders[luma_tu_count] = luma_tu.abs_remainder;
        luma_tu_negative[luma_tu_count] = luma_tu.negative;
        luma_tu_dc_levels[luma_tu_count] = luma_tu.dc_level;
        luma_tu_ac_levels[luma_tu_count] = luma_tu.ac_levels;
        luma_tu_has_ac[luma_tu_count] = luma_tu.has_ac;
        luma_tu_transform_skip[luma_tu_count] = luma_tu.transform_skip;
        luma_tu_bdpcm_modes[luma_tu_count] = luma_tu.bdpcm_mode;
        luma_tu_mrl_index[luma_tu_count] = luma_tu.mrl_index;
        luma_tu_mts_index[luma_tu_count] = luma_tu.mts_index;
        #[cfg(feature = "vvc-stats")]
        write_vvc_luma_tu_trace(
            tu_trace_sink.as_mut(),
            region,
            luma_tu_count,
            node,
            luma_mode,
            luma_tu,
            &predicted_luma,
            &luma_residuals,
        );
        luma_tu_count += 1;
    }

    let mut chroma_tu_count = 0usize;
    for local_node in vvc_chroma_transform_nodes(ctu_shape) {
        if chroma_tu_count >= MAX_VVC_CHROMA_TUS {
            break;
        }
        let node = vvc_global_ctu_node(local_node, region);
        chroma_rd_cache.reset(policy, node);
        let subsample_x = chroma_subsample_x(source_frame.format.chroma_sampling);
        let subsample_y = chroma_subsample_y(source_frame.format.chroma_sampling);
        let chroma_x = usize::from(node.x) / subsample_x;
        let chroma_y = usize::from(node.y) / subsample_y;
        let chroma_width = usize::from(node.width) / subsample_x;
        let chroma_height = usize::from(node.height) / subsample_y;
        let co_located_luma_mode = luma_mode_search_state.co_located_mode_for_chroma_node(node);
        let cclm_syntax_enabled = vvc_chroma_cclm_node_allowed(node);
        let initial_chroma_mode = VvcChromaIntraPredictionMode::Derived;
        #[cfg(feature = "vvc-stats")]
        let chroma_mode_search_start = Instant::now();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
            &mut predicted_cb,
            &mut predicted_cr,
            &mut prediction_scratch,
            initial_chroma_mode,
            co_located_luma_mode,
            &frame_recon.cb,
            &frame_recon.cr,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            Some(frame_recon.cb_availability()),
            Some(frame_recon.cr_availability()),
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_prediction_nanos(
            VvcChromaPredictionStatsFamily::Derived,
            vvc_elapsed_nanos(prediction_start),
        );
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let initial_score = score_chroma_mode_candidate(
            &mut chroma_rd_cache,
            score_metric,
            initial_chroma_mode,
            source_frame,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            &predicted_cb,
            &predicted_cr,
            cclm_syntax_enabled,
            chroma_syntax_tie_breaker,
            &mut candidate_cb_residuals,
            &mut candidate_cr_residuals,
            &mut intra_search_stats,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
        let mut best_chroma_mode = initial_chroma_mode;
        let mut best_chroma_score = initial_score;
        let mut chroma_candidate_costs = VvcChromaIntraCandidateCosts::new(initial_score);
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_derived();
        if !vvc_chroma_lossy_exact_mode_search_done(chroma_syntax_tie_breaker, best_chroma_score) {
            for explicit_mode in vvc_chroma_explicit_candidates(co_located_luma_mode) {
                if !vvc_residual_chroma_explicit_candidate_allowed(explicit_mode) {
                    continue;
                }
                let chroma_mode = VvcChromaIntraPredictionMode::Explicit(explicit_mode);
                #[cfg(feature = "vvc-stats")]
                let prediction_start = Instant::now();
                predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                    &mut candidate_cb_prediction,
                    &mut candidate_cr_prediction,
                    &mut prediction_scratch,
                    chroma_mode,
                    co_located_luma_mode,
                    &frame_recon.cb,
                    &frame_recon.cr,
                    &frame_recon.luma,
                    source_frame.geometry,
                    node,
                    source_frame.format.chroma_sampling,
                    source_frame.format.bit_depth,
                    Some(frame_recon.cb_availability()),
                    Some(frame_recon.cr_availability()),
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_prediction_nanos(
                    VvcChromaPredictionStatsFamily::Explicit,
                    vvc_elapsed_nanos(prediction_start),
                );
                #[cfg(feature = "vvc-stats")]
                let score_start = Instant::now();
                let candidate_score = score_chroma_mode_candidate(
                    &mut chroma_rd_cache,
                    score_metric,
                    chroma_mode,
                    source_frame,
                    chroma_x,
                    chroma_y,
                    chroma_width,
                    chroma_height,
                    &candidate_cb_prediction,
                    &candidate_cr_prediction,
                    cclm_syntax_enabled,
                    chroma_syntax_tie_breaker,
                    &mut candidate_cb_residuals,
                    &mut candidate_cr_residuals,
                    &mut intra_search_stats,
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_explicit();
                chroma_candidate_costs =
                    chroma_candidate_costs.with_candidate(chroma_mode, Some(candidate_score));
                if candidate_score < best_chroma_score {
                    best_chroma_score = candidate_score;
                    best_chroma_mode = chroma_mode;
                    std::mem::swap(&mut predicted_cb, &mut candidate_cb_prediction);
                    std::mem::swap(&mut predicted_cr, &mut candidate_cr_prediction);
                }
                if vvc_chroma_lossy_exact_mode_search_done(
                    chroma_syntax_tie_breaker,
                    best_chroma_score,
                ) {
                    break;
                }
            }
        }
        if policy.chroma_cclm_candidate_allowed(node, source_frame.geometry)
            && !vvc_chroma_lossy_exact_mode_search_done(
                chroma_syntax_tie_breaker,
                best_chroma_score,
            )
        {
            for cclm_mode in [
                VvcChromaCclmMode::Linear,
                VvcChromaCclmMode::MdlmLeft,
                VvcChromaCclmMode::MdlmTop,
            ] {
                let chroma_mode = VvcChromaIntraPredictionMode::Cclm(cclm_mode);
                #[cfg(feature = "vvc-stats")]
                let prediction_start = Instant::now();
                predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                    &mut candidate_cb_prediction,
                    &mut candidate_cr_prediction,
                    &mut prediction_scratch,
                    chroma_mode,
                    co_located_luma_mode,
                    &frame_recon.cb,
                    &frame_recon.cr,
                    &frame_recon.luma,
                    source_frame.geometry,
                    node,
                    source_frame.format.chroma_sampling,
                    source_frame.format.bit_depth,
                    Some(frame_recon.cb_availability()),
                    Some(frame_recon.cr_availability()),
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_prediction_nanos(
                    VvcChromaPredictionStatsFamily::Cclm,
                    vvc_elapsed_nanos(prediction_start),
                );
                #[cfg(feature = "vvc-stats")]
                let score_start = Instant::now();
                let candidate_score = score_chroma_mode_candidate(
                    &mut chroma_rd_cache,
                    score_metric,
                    chroma_mode,
                    source_frame,
                    chroma_x,
                    chroma_y,
                    chroma_width,
                    chroma_height,
                    &candidate_cb_prediction,
                    &candidate_cr_prediction,
                    cclm_syntax_enabled,
                    chroma_syntax_tie_breaker,
                    &mut candidate_cb_residuals,
                    &mut candidate_cr_residuals,
                    &mut intra_search_stats,
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_cclm();
                chroma_candidate_costs =
                    chroma_candidate_costs.with_candidate(chroma_mode, Some(candidate_score));
                if candidate_score < best_chroma_score {
                    best_chroma_score = candidate_score;
                    best_chroma_mode = chroma_mode;
                    std::mem::swap(&mut predicted_cb, &mut candidate_cb_prediction);
                    std::mem::swap(&mut predicted_cr, &mut candidate_cr_prediction);
                }
            }
        }
        let raw_chroma_mode = policy.select_chroma_intra_mode(node, chroma_candidate_costs);
        debug_assert_eq!(raw_chroma_mode, best_chroma_mode);
        let _best_chroma_score = best_chroma_score;
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_mode_search_nanos(chroma_mode_search_start.elapsed().as_nanos() as u64);
        if let Some(cached) = chroma_rd_cache.get(raw_chroma_mode) {
            cb_residuals.clear();
            cb_residuals.extend_from_slice(&cached.cb_residuals);
            cr_residuals.clear();
            cr_residuals.extend_from_slice(&cached.cr_residuals);
        } else {
            #[cfg(feature = "vvc-stats")]
            let residual_start = Instant::now();
            residual_chroma_tu_at_into(
                &mut cb_residuals,
                &source_frame.cb,
                source_frame.geometry,
                source_frame.format,
                chroma_x,
                chroma_y,
                chroma_width,
                chroma_height,
                &predicted_cb,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
            #[cfg(feature = "vvc-stats")]
            let residual_start = Instant::now();
            residual_chroma_tu_at_into(
                &mut cr_residuals,
                &source_frame.cr,
                source_frame.geometry,
                source_frame.format,
                chroma_x,
                chroma_y,
                chroma_width,
                chroma_height,
                &predicted_cr,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        }
        #[cfg(feature = "vvc-stats")]
        let chroma_rd_start = Instant::now();
        let selected_chroma_mode = select_vvc_chroma_mode_with_rd_refinement(
            policy,
            node,
            raw_chroma_mode,
            chroma_candidate_costs,
            &chroma_rd_cache,
            &mut intra_search_stats,
            co_located_luma_mode,
            cclm_syntax_enabled,
            source_frame,
            frame_recon,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            &mut prediction_scratch,
            &mut predicted_cb,
            &mut predicted_cr,
            &mut cb_residuals,
            &mut cr_residuals,
            &mut candidate_cb_prediction,
            &mut candidate_cr_prediction,
            &mut candidate_cb_residuals,
            &mut candidate_cr_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_rd_refinement_nanos(chroma_rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_chroma_mode.residual.is_some() {
            intra_search_stats.add_chroma_rd_refinement_attempt();
            if selected_chroma_mode.mode != raw_chroma_mode {
                intra_search_stats.add_chroma_rd_refinement_switch();
            }
        }
        let mut chroma_mode = selected_chroma_mode.mode;
        let mut selected_chroma_residual = selected_chroma_mode.residual;
        #[cfg(feature = "vvc-stats")]
        let chroma_bdpcm_start = Instant::now();
        if let Some(selected_bdpcm) = select_vvc_chroma_bdpcm_prediction(
            policy,
            node,
            chroma_mode,
            cclm_syntax_enabled,
            source_frame,
            frame_recon,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            selected_chroma_residual,
            &mut intra_search_stats,
            &mut prediction_scratch,
            &mut predicted_cb,
            &mut predicted_cr,
            &mut cb_residuals,
            &mut cr_residuals,
            &mut candidate_cb_prediction,
            &mut candidate_cr_prediction,
            &mut candidate_cb_residuals,
            &mut candidate_cr_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        ) {
            chroma_mode = selected_bdpcm.mode;
            selected_chroma_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_bdpcm_nanos(chroma_bdpcm_start.elapsed().as_nanos() as u64);
        chroma_tu_intra_modes[chroma_tu_count] = chroma_mode;
        let chroma_coding_decision = policy.select_chroma_tu_coding_decision(node, chroma_mode);
        #[cfg(feature = "vvc-stats")]
        {
            residual_energy_stats.add_chroma_residuals(&cb_residuals, chroma_width, chroma_height);
            residual_energy_stats.add_chroma_residuals(&cr_residuals, chroma_width, chroma_height);
        }
        #[cfg(feature = "vvc-stats")]
        let chroma_finalize_start = Instant::now();
        let chroma_tu = finalize_vvc_chroma_tu(
            chroma_coding_decision,
            source_frame,
            frame_recon,
            node,
            &predicted_cb,
            &predicted_cr,
            &cb_residuals,
            &cr_residuals,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            selected_chroma_residual,
            &mut intra_search_stats,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_finalize_nanos(chroma_finalize_start.elapsed().as_nanos() as u64);
        cb_tu_dc_levels[chroma_tu_count] = chroma_tu.cb_dc_level;
        cr_tu_dc_levels[chroma_tu_count] = chroma_tu.cr_dc_level;
        cb_tu_ac_levels[chroma_tu_count] = chroma_tu.cb_ac_levels;
        cr_tu_ac_levels[chroma_tu_count] = chroma_tu.cr_ac_levels;
        cb_tu_has_ac[chroma_tu_count] = chroma_tu.cb_has_ac;
        cr_tu_has_ac[chroma_tu_count] = chroma_tu.cr_has_ac;
        cb_tu_transform_skip[chroma_tu_count] = chroma_tu.cb_transform_skip;
        cr_tu_transform_skip[chroma_tu_count] = chroma_tu.cr_transform_skip;
        chroma_tu_bdpcm_modes[chroma_tu_count] = chroma_tu.bdpcm_mode;
        #[cfg(feature = "vvc-stats")]
        write_vvc_chroma_tu_trace(
            tu_trace_sink.as_mut(),
            region,
            chroma_tu_count,
            node,
            chroma_mode,
            co_located_luma_mode,
            chroma_tu,
            chroma_width,
            chroma_height,
            &predicted_cb,
            &predicted_cr,
            &cb_residuals,
            &cr_residuals,
        );
        chroma_tu_count += 1;
    }

    let color = source_frame.sampled_color();
    let cb_rem = quantize_vvc_chroma_sample(vvc_downshift_sample_to_u8(
        color.u,
        source_frame.format.bit_depth,
    ));
    let cr_rem = quantize_vvc_chroma_sample(vvc_downshift_sample_to_u8(
        color.v,
        source_frame.format.bit_depth,
    ));
    VvcQuantizedColor {
        y: vvc_downshift_sample_to_u8(color.y, source_frame.format.bit_depth),
        u: finalized_vvc_chroma_sample(
            cb_tu_transform_skip.first().copied().unwrap_or(false),
            color.u,
            cb_rem,
            source_frame.format.bit_depth,
        ),
        v: finalized_vvc_chroma_sample(
            cr_tu_transform_skip.first().copied().unwrap_or(false),
            color.v,
            cr_rem,
            source_frame.format.bit_depth,
        ),
        luma_tu_intra_modes,
        luma_tu_remainders,
        luma_tu_negative,
        luma_tu_dc_levels,
        luma_tu_ac_levels,
        luma_tu_has_ac,
        luma_tu_transform_skip,
        luma_tu_bdpcm_modes,
        luma_tu_mrl_index,
        luma_tu_mts_index,
        luma_tu_count,
        chroma_tu_count,
        chroma_tu_intra_modes,
        cb_tu_dc_levels,
        cr_tu_dc_levels,
        cb_tu_ac_levels,
        cr_tu_ac_levels,
        cb_tu_has_ac,
        cr_tu_has_ac,
        cb_tu_transform_skip,
        cr_tu_transform_skip,
        chroma_tu_bdpcm_modes,
        cb_rem,
        cr_rem,
        #[cfg(feature = "vvc-stats")]
        intra_search_stats,
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats,
    }
}
