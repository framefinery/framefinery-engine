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

fn select_vvc_chroma_bdpcm_prediction(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    selected_mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
    cclm_syntax_enabled: bool,
    source_frame: &VvcSampledFrame,
    frame_recon: &VvcReconstructionFrame,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    selected_residual: Option<VvcScoredSelectedChromaResidual>,
    stats: &mut VvcIntraSearchStats,
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
) -> Option<VvcSelectedChromaBdpcm> {
    if !vvc_chroma_bdpcm_selection_allowed(policy, chroma_width, chroma_height)
        || !vvc_chroma_lossless_speed_bdpcm_format_allowed(policy, source_frame.format)
        || !vvc_chroma_bdpcm_fast_search_allowed(policy, selected_mode)
    {
        return None;
    }

    let baseline_decision = policy.select_chroma_tu_coding_decision(node, selected_mode);
    let baseline_residual = selected_residual.unwrap_or_else(|| {
        #[cfg(feature = "vvc-stats")]
        let score_start = StageStart::now();
        let residual = VvcSelectedChromaResidual {
            cb: finalize_vvc_chroma_residual_block(
                baseline_decision.residual_coding,
                selected_cb_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            ),
            cr: finalize_vvc_chroma_residual_block(
                baseline_decision.residual_coding,
                selected_cr_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            ),
        };
        let residual = VvcScoredSelectedChromaResidual::new(
            selected_cb_residuals,
            selected_cr_residuals,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        residual
    });
    let mut best_score = vvc_scored_chroma_quantized_residual_score(
        baseline_residual,
        u64::from(vvc_bdpcm_mode_syntax_bin_count(VvcBdpcmMode::None)).saturating_add(u64::from(
            vvc_chroma_intra_mode_syntax_bin_count(selected_mode, cclm_syntax_enabled),
        )),
    );

    for bdpcm_mode in vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
        policy,
        source_frame.format.chroma_sampling,
        source_frame.format.bit_depth,
        selected_mode,
        co_located_luma_mode,
    )
    .into_iter()
    .flatten()
    {
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_bdpcm_direct_candidate();
        build_vvc_chroma_bdpcm_candidate(
            node,
            bdpcm_mode,
            source_frame,
            frame_recon,
            chroma_width,
            chroma_height,
            stats,
            prediction_scratch,
            candidate_cb_prediction,
            candidate_cr_prediction,
            candidate_cb_residuals,
            candidate_cr_residuals,
        );
        let direct_bdpcm_safe = vvc_chroma_direct_bdpcm_residual_is_safe(
            selected_cb_residuals,
            selected_cr_residuals,
            candidate_cb_residuals,
            candidate_cr_residuals,
        );
        #[cfg(feature = "vvc-stats")]
        if direct_bdpcm_safe {
            stats.add_chroma_bdpcm_direct_safe_candidate();
        }
        if !direct_bdpcm_safe {
            continue;
        }
        let (residual, candidate_score) = score_vvc_chroma_bdpcm_candidate(
            bdpcm_mode,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            source_frame.format.bit_depth,
            stats,
            candidate_cb_residuals,
            candidate_cr_residuals,
            transform_scratch,
            reconstructed_residual,
        );
        if candidate_score.selects_over(best_score) {
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_bdpcm_direct_selected();
            std::mem::swap(selected_cb_prediction, candidate_cb_prediction);
            std::mem::swap(selected_cr_prediction, candidate_cr_prediction);
            std::mem::swap(selected_cb_residuals, candidate_cb_residuals);
            std::mem::swap(selected_cr_residuals, candidate_cr_residuals);
            let mode = VvcChromaIntraPredictionMode::Explicit(
                bdpcm_mode
                    .inferred_intra_mode()
                    .expect("enabled BDPCM mode has an inferred intra mode"),
            );
            return Some(VvcSelectedChromaBdpcm { mode, residual });
        }
    }

    let mut best = None;

    for bdpcm_mode in vvc_chroma_bdpcm_candidate_modes(policy, co_located_luma_mode)
        .into_iter()
        .flatten()
    {
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_bdpcm_regular_candidate();
        build_vvc_chroma_bdpcm_candidate(
            node,
            bdpcm_mode,
            source_frame,
            frame_recon,
            chroma_width,
            chroma_height,
            stats,
            prediction_scratch,
            candidate_cb_prediction,
            candidate_cr_prediction,
            candidate_cb_residuals,
            candidate_cr_residuals,
        );
        let (residual, candidate_score) = score_vvc_chroma_bdpcm_candidate(
            bdpcm_mode,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            source_frame.format.bit_depth,
            stats,
            candidate_cb_residuals,
            candidate_cr_residuals,
            transform_scratch,
            reconstructed_residual,
        );
        if candidate_score.selects_over(best_score) {
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_bdpcm_regular_best_update();
            best_score = candidate_score;
            let mode = VvcChromaIntraPredictionMode::Explicit(
                bdpcm_mode
                    .inferred_intra_mode()
                    .expect("enabled BDPCM mode has an inferred intra mode"),
            );
            best = Some(VvcSelectedChromaBdpcm { mode, residual });
            std::mem::swap(selected_cb_prediction, candidate_cb_prediction);
            std::mem::swap(selected_cr_prediction, candidate_cr_prediction);
            std::mem::swap(selected_cb_residuals, candidate_cb_residuals);
            std::mem::swap(selected_cr_residuals, candidate_cr_residuals);
        }
    }

    best
}

fn build_vvc_chroma_bdpcm_candidate(
    node: VvcCodingTreeNode,
    bdpcm_mode: VvcBdpcmMode,
    source_frame: &VvcSampledFrame,
    frame_recon: &VvcReconstructionFrame,
    chroma_width: usize,
    chroma_height: usize,
    stats: &mut VvcIntraSearchStats,
    prediction_scratch: &mut VvcDcPredictionScratch,
    candidate_cb_prediction: &mut Vec<VvcSample>,
    candidate_cr_prediction: &mut Vec<VvcSample>,
    candidate_cb_residuals: &mut Vec<i16>,
    candidate_cr_residuals: &mut Vec<i16>,
) {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    #[cfg(feature = "vvc-stats")]
    let prediction_start = StageStart::now();
    predict_vvc_chroma_bdpcm_block_into_with_availability(
        candidate_cb_prediction,
        prediction_scratch,
        bdpcm_mode,
        &frame_recon.cb,
        frame_recon.coded_geometry(),
        node,
        source_frame.format.chroma_sampling,
        source_frame.format.bit_depth,
        Some(frame_recon.cb_availability()),
    );
    predict_vvc_chroma_bdpcm_block_into_with_availability(
        candidate_cr_prediction,
        prediction_scratch,
        bdpcm_mode,
        &frame_recon.cr,
        frame_recon.coded_geometry(),
        node,
        source_frame.format.chroma_sampling,
        source_frame.format.bit_depth,
        Some(frame_recon.cr_availability()),
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_prediction_nanos(
        VvcChromaPredictionStatsFamily::Bdpcm,
        vvc_elapsed_nanos(prediction_start),
    );

    let chroma_x = usize::from(node.x) / chroma_subsample_x(source_frame.format.chroma_sampling);
    let chroma_y = usize::from(node.y) / chroma_subsample_y(source_frame.format.chroma_sampling);
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
    stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));

}

fn score_vvc_chroma_bdpcm_candidate(
    bdpcm_mode: VvcBdpcmMode,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    bit_depth: SampleBitDepth,
    stats: &mut VvcIntraSearchStats,
    candidate_cb_residuals: &[i16],
    candidate_cr_residuals: &[i16],
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> (VvcScoredSelectedChromaResidual, VvcChromaQuantizedResidualScore) {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    #[cfg(feature = "vvc-stats")]
    let score_start = StageStart::now();
    let finalized = VvcSelectedChromaResidual {
        cb: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
            candidate_cb_residuals,
            chroma_width,
            chroma_height,
            chroma_ts_quant,
            bdpcm_mode,
        ),
        cr: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
            candidate_cr_residuals,
            chroma_width,
            chroma_height,
            chroma_ts_quant,
            bdpcm_mode,
        ),
    };
    let residual = VvcScoredSelectedChromaResidual::new(
        candidate_cb_residuals,
        candidate_cr_residuals,
        chroma_width,
        chroma_height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        finalized,
        transform_scratch,
        reconstructed_residual,
    );
    let score = vvc_scored_chroma_quantized_residual_score(
        residual,
        u64::from(vvc_bdpcm_mode_syntax_bin_count(bdpcm_mode)),
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    (residual, score)
}

fn vvc_chroma_bdpcm_candidate_modes(
    policy: VvcResidualCodingPolicy,
    co_located_luma_mode: VvcIntraPredictionMode,
) -> [Option<VvcBdpcmMode>; 2] {
    if policy.fast_search() == VvcFastSearch::LosslessSpeed {
        match co_located_luma_mode {
            VvcIntraPredictionMode::Horizontal => [Some(VvcBdpcmMode::Horizontal), None],
            VvcIntraPredictionMode::Vertical => [Some(VvcBdpcmMode::Vertical), None],
            VvcIntraPredictionMode::Planar
            | VvcIntraPredictionMode::Dc
            | VvcIntraPredictionMode::Angular(_) => {
                [Some(VvcBdpcmMode::Horizontal), Some(VvcBdpcmMode::Vertical)]
            }
        }
    } else {
        [Some(VvcBdpcmMode::Horizontal), Some(VvcBdpcmMode::Vertical)]
    }
}

fn vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
    policy: VvcResidualCodingPolicy,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    selected_mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
) -> [Option<VvcBdpcmMode>; 2] {
    if !vvc_chroma_lossy_speed_direct_bdpcm_candidates_allowed(
        policy,
        chroma_sampling,
        bit_depth,
        selected_mode,
    ) {
        return [None, None];
    }
    match co_located_luma_mode {
        VvcIntraPredictionMode::Horizontal => [Some(VvcBdpcmMode::Horizontal), None],
        VvcIntraPredictionMode::Vertical => [Some(VvcBdpcmMode::Vertical), None],
        VvcIntraPredictionMode::Planar
        | VvcIntraPredictionMode::Dc
        | VvcIntraPredictionMode::Angular(_) => {
            [Some(VvcBdpcmMode::Horizontal), Some(VvcBdpcmMode::Vertical)]
        }
    }
}

fn vvc_chroma_lossy_speed_direct_bdpcm_candidates_allowed(
    policy: VvcResidualCodingPolicy,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    selected_mode: VvcChromaIntraPredictionMode,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
        && chroma_sampling == ChromaSampling::Cs444
        && bit_depth.bits() == 8
        && matches!(selected_mode, VvcChromaIntraPredictionMode::Derived)
}

fn vvc_chroma_direct_bdpcm_residual_is_safe(
    selected_cb_residuals: &[i16],
    selected_cr_residuals: &[i16],
    candidate_cb_residuals: &[i16],
    candidate_cr_residuals: &[i16],
) -> bool {
    let selected_sse = vvc_chroma_pair_residual_sse(selected_cb_residuals, selected_cr_residuals);
    let candidate_sse = vvc_chroma_pair_residual_sse(candidate_cb_residuals, candidate_cr_residuals);
    // Bypass the RD check only when BDPCM materially improves raw prediction SSE.
    candidate_sse.saturating_mul(16) <= selected_sse.saturating_mul(15)
}

fn vvc_chroma_pair_residual_sse(cb_residuals: &[i16], cr_residuals: &[i16]) -> u64 {
    cb_residuals
        .iter()
        .chain(cr_residuals.iter())
        .fold(0u64, |sse, residual| {
            let residual = i64::from(*residual);
            sse.saturating_add((residual * residual) as u64)
        })
}

fn vvc_chroma_bdpcm_fast_search_allowed(
    policy: VvcResidualCodingPolicy,
    selected_mode: VvcChromaIntraPredictionMode,
) -> bool {
    if policy.residual_mode() == VvcResidualCodingMode::Lossless
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
    {
        return matches!(selected_mode, VvcChromaIntraPredictionMode::Derived);
    }
    match policy.fast_search() {
        VvcFastSearch::Off | VvcFastSearch::Conservative => true,
        VvcFastSearch::LosslessSpeed if policy.residual_mode() == VvcResidualCodingMode::Lossy => {
            true
        }
        VvcFastSearch::Moderate | VvcFastSearch::LosslessSpeed => {
            matches!(selected_mode, VvcChromaIntraPredictionMode::Derived)
                || vvc_chroma_mode_is_bdpcm_aligned(selected_mode)
        }
        VvcFastSearch::Aggressive => vvc_chroma_mode_is_bdpcm_aligned(selected_mode),
    }
}

fn vvc_chroma_lossless_speed_bdpcm_format_allowed(
    policy: VvcResidualCodingPolicy,
    format: VvcPictureFormat,
) -> bool {
    if policy.residual_mode() != VvcResidualCodingMode::Lossless
        || policy.fast_search() != VvcFastSearch::LosslessSpeed
    {
        return true;
    }
    format.bit_depth.bits() == 8 || format.chroma_sampling != ChromaSampling::Cs420
}

fn vvc_chroma_mode_is_bdpcm_aligned(mode: VvcChromaIntraPredictionMode) -> bool {
    matches!(
        mode,
        VvcChromaIntraPredictionMode::Explicit(
            VvcIntraPredictionMode::Horizontal | VvcIntraPredictionMode::Vertical
        )
    )
}

fn vvc_chroma_bdpcm_selection_allowed(
    policy: VvcResidualCodingPolicy,
    chroma_width: usize,
    chroma_height: usize,
) -> bool {
    VVC_ENABLE_BDPCM_SELECTION
        && chroma_width == 4
        && chroma_height == 4
        && matches!(
            policy.residual_mode(),
            VvcResidualCodingMode::Lossy | VvcResidualCodingMode::Lossless
        )
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedChromaBdpcm {
    mode: VvcChromaIntraPredictionMode,
    residual: VvcScoredSelectedChromaResidual,
}
