fn select_vvc_chroma_mode_with_rd_refinement(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    raw_mode: VvcChromaIntraPredictionMode,
    candidate_costs: VvcChromaIntraCandidateCosts,
    rd_cache: &mut VvcChromaModeRdCache,
    stats: &mut VvcIntraSearchStats,
    co_located_luma_mode: VvcIntraPredictionMode,
    cclm_syntax_enabled: bool,
    source_frame: &VvcSampledFrame,
    frame_recon: &VvcReconstructionFrame,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_cb_prediction: &mut Vec<VvcSample>,
    selected_cr_prediction: &mut Vec<VvcSample>,
    selected_cb_residuals: &mut Vec<i16>,
    selected_cr_residuals: &mut Vec<i16>,
    candidate_cb_prediction: &mut Vec<VvcSample>,
    candidate_cr_prediction: &mut Vec<VvcSample>,
    candidate_cb_residuals: &mut Vec<i16>,
    candidate_cr_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedChromaMode {
    let raw_decision = policy.select_chroma_tu_coding_decision(node, raw_mode);
    if !vvc_chroma_lossy_rd_refinement_allowed(policy, node, raw_decision) {
        return VvcSelectedChromaMode {
            mode: raw_mode,
            residual: None,
        };
    }
    if vvc_chroma_exact_prediction_skips_rd(selected_cb_residuals, selected_cr_residuals) {
        return VvcSelectedChromaMode {
            mode: raw_mode,
            residual: None,
        };
    }

    let mut best_mode = raw_mode;
    #[cfg(feature = "vvc-stats")]
    let score_start = StageStart::now();
    let mut best_candidate = score_vvc_chroma_mode_rd_candidate(
        policy,
        raw_decision,
        raw_mode,
        cclm_syntax_enabled,
        selected_cb_residuals,
        selected_cr_residuals,
        chroma_width,
        chroma_height,
        source_frame.format.bit_depth,
        chroma_qp,
        chroma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    let shortlist = VvcChromaModeRdShortlist::from_candidate_costs(policy, candidate_costs);
    for candidate in shortlist.iter() {
        if policy.residual_mode() == VvcResidualCodingMode::Lossy
            && policy.fast_search() == VvcFastSearch::LosslessSpeed
            && policy.chroma_sampling() == ChromaSampling::Cs420
            && !shortlist.admits_lossless_speed_rd(candidate)
        {
            continue;
        }
        let mode = candidate.mode();
        if mode == raw_mode {
            continue;
        }
        let coding_decision = policy.select_chroma_tu_coding_decision(node, mode);
        if !matches!(
            coding_decision.residual_coding,
            VvcTuResidualCodingMode::Transformed
        ) {
            continue;
        }
        if let Some(cached) = rd_cache.get(mode) {
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_rd_cached_candidate();
            #[cfg(feature = "vvc-stats")]
            let score_start = StageStart::now();
            let rd_candidate = score_vvc_chroma_mode_rd_candidate(
                policy,
                coding_decision,
                mode,
                cclm_syntax_enabled,
                &cached.cb_residuals,
                &cached.cr_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
            if rd_candidate.selects_over(best_candidate) {
                best_mode = mode;
                best_candidate = rd_candidate;
                #[cfg(feature = "vvc-stats")]
                let prediction_start = StageStart::now();
                predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                    selected_cb_prediction,
                    selected_cr_prediction,
                    prediction_scratch,
                    mode,
                    co_located_luma_mode,
                    &frame_recon.cb,
                    &frame_recon.cr,
                    &frame_recon.luma,
                    frame_recon.coded_geometry(),
                    node,
                    source_frame.format.chroma_sampling,
                    source_frame.format.bit_depth,
                    Some(frame_recon.cb_availability()),
                    Some(frame_recon.cr_availability()),
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                stats.add_chroma_rd_prediction_nanos(vvc_elapsed_nanos(prediction_start));
                assert!(
                    rd_cache.take_residuals_if_present(
                        mode,
                        selected_cb_residuals,
                        selected_cr_residuals,
                    ),
                    "cached chroma mode disappeared before residual transfer",
                );
            }
            continue;
        }
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_generated_candidate();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
            candidate_cb_prediction,
            candidate_cr_prediction,
            prediction_scratch,
            mode,
            co_located_luma_mode,
            &frame_recon.cb,
            &frame_recon.cr,
            &frame_recon.luma,
            frame_recon.coded_geometry(),
            node,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            Some(frame_recon.cb_availability()),
            Some(frame_recon.cr_availability()),
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(prediction_start);
            stats.add_chroma_rd_prediction_nanos(nanos);
            stats.add_chroma_prediction_nanos(vvc_chroma_prediction_stats_family(mode), nanos);
        }
        let chroma_x =
            usize::from(node.x) / chroma_subsample_x(source_frame.format.chroma_sampling);
        let chroma_y =
            usize::from(node.y) / chroma_subsample_y(source_frame.format.chroma_sampling);
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_pair_tu_at_into(
            candidate_cb_residuals,
            candidate_cr_residuals,
            &source_frame.cb,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cb_prediction,
            candidate_cr_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(residual_start);
            stats.add_chroma_residual_build_nanos(nanos);
            stats.add_chroma_rd_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let score_start = StageStart::now();
        let rd_candidate = score_vvc_chroma_mode_rd_candidate(
            policy,
            coding_decision,
            mode,
            cclm_syntax_enabled,
            candidate_cb_residuals,
            candidate_cr_residuals,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
            chroma_ts_quant,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if rd_candidate.selects_over(best_candidate) {
            best_mode = mode;
            best_candidate = rd_candidate;
            std::mem::swap(selected_cb_prediction, candidate_cb_prediction);
            std::mem::swap(selected_cr_prediction, candidate_cr_prediction);
            std::mem::swap(selected_cb_residuals, candidate_cb_residuals);
            std::mem::swap(selected_cr_residuals, candidate_cr_residuals);
        }
    }

    VvcSelectedChromaMode {
        mode: best_mode,
        residual: Some(best_candidate.residual),
    }
}

fn vvc_chroma_lossy_rd_refinement_allowed(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    decision: VvcChromaTuCodingDecision,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && matches!(
            decision.residual_coding,
            VvcTuResidualCodingMode::Transformed
        )
        && [4, 8, 16, 32].contains(&node.width)
        && [4, 8, 16, 32].contains(&node.height)
}

fn vvc_chroma_exact_prediction_skips_rd(cb_residuals: &[i16], cr_residuals: &[i16]) -> bool {
    cb_residuals.iter().all(|residual| *residual == 0)
        && cr_residuals.iter().all(|residual| *residual == 0)
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedChromaMode {
    mode: VvcChromaIntraPredictionMode,
    residual: Option<VvcScoredSelectedChromaResidual>,
}
