fn select_vvc_chroma_mode_with_rd_refinement(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    raw_mode: VvcChromaIntraPredictionMode,
    candidate_costs: VvcChromaIntraCandidateCosts,
    rd_cache: &VvcChromaModeRdCache,
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
    let score_start = Instant::now();
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
            let score_start = Instant::now();
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
                let prediction_start = Instant::now();
                predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                    selected_cb_prediction,
                    selected_cr_prediction,
                    prediction_scratch,
                    mode,
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
                stats.add_chroma_rd_prediction_nanos(vvc_elapsed_nanos(prediction_start));
                selected_cb_residuals.clear();
                selected_cb_residuals.extend_from_slice(&cached.cb_residuals);
                selected_cr_residuals.clear();
                selected_cr_residuals.extend_from_slice(&cached.cr_residuals);
            }
            continue;
        }
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_generated_candidate();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
            candidate_cb_prediction,
            candidate_cr_prediction,
            prediction_scratch,
            mode,
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
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cb_residuals,
            &source_frame.cb,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cb_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(residual_start);
            stats.add_chroma_residual_build_nanos(nanos);
            stats.add_chroma_rd_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cr_residuals,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cr_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(residual_start);
            stats.add_chroma_residual_build_nanos(nanos);
            stats.add_chroma_rd_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
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
        || !vvc_chroma_bdpcm_fast_search_allowed(policy, selected_mode)
    {
        return None;
    }

    if let Some(bdpcm_mode) = vvc_chroma_lossy_speed_direct_bdpcm_mode(
        policy,
        source_frame.format.chroma_sampling,
        source_frame.format.bit_depth,
        selected_mode,
        co_located_luma_mode,
    ) {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_chroma_bdpcm_block_into_with_availability(
            candidate_cb_prediction,
            prediction_scratch,
            bdpcm_mode,
            &frame_recon.cb,
            source_frame.geometry,
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
            source_frame.geometry,
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
        let chroma_x =
            usize::from(node.x) / chroma_subsample_x(source_frame.format.chroma_sampling);
        let chroma_y =
            usize::from(node.y) / chroma_subsample_y(source_frame.format.chroma_sampling);
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cb_residuals,
            &source_frame.cb,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cb_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cr_residuals,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cr_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        if vvc_chroma_direct_bdpcm_residual_is_safe(
            selected_cb_residuals,
            selected_cr_residuals,
            candidate_cb_residuals,
            candidate_cr_residuals,
        ) {
            std::mem::swap(selected_cb_prediction, candidate_cb_prediction);
            std::mem::swap(selected_cr_prediction, candidate_cr_prediction);
            std::mem::swap(selected_cb_residuals, candidate_cb_residuals);
            std::mem::swap(selected_cr_residuals, candidate_cr_residuals);
            let residual = VvcSelectedChromaResidual {
                cb: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                    selected_cb_residuals,
                    chroma_width,
                    chroma_height,
                    chroma_ts_quant,
                    bdpcm_mode,
                ),
                cr: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                    selected_cr_residuals,
                    chroma_width,
                    chroma_height,
                    chroma_ts_quant,
                    bdpcm_mode,
                ),
            };
            let mode = VvcChromaIntraPredictionMode::Explicit(
                bdpcm_mode
                    .inferred_intra_mode()
                    .expect("enabled BDPCM mode has an inferred intra mode"),
            );
            return Some(VvcSelectedChromaBdpcm {
                mode,
                residual: VvcScoredSelectedChromaResidual {
                    residual,
                    // The direct path has already selected BDPCM; finalization
                    // only consumes the residual payload from this wrapper.
                    score: VvcResidualBlockScore {
                        distortion: 0,
                        rate_cost: 0,
                    },
                },
            });
        }
    }

    let baseline_decision = policy.select_chroma_tu_coding_decision(node, selected_mode);
    let baseline_residual = selected_residual.unwrap_or_else(|| {
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
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
    let mut best = None;

    for bdpcm_mode in vvc_chroma_bdpcm_candidate_modes(policy, co_located_luma_mode)
        .into_iter()
        .flatten()
    {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_chroma_bdpcm_block_into_with_availability(
            candidate_cb_prediction,
            prediction_scratch,
            bdpcm_mode,
            &frame_recon.cb,
            source_frame.geometry,
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
            source_frame.geometry,
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
        let chroma_x =
            usize::from(node.x) / chroma_subsample_x(source_frame.format.chroma_sampling);
        let chroma_y =
            usize::from(node.y) / chroma_subsample_y(source_frame.format.chroma_sampling);
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cb_residuals,
            &source_frame.cb,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cb_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cr_residuals,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cr_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let residual = VvcSelectedChromaResidual {
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
            source_frame.format.bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual,
            transform_scratch,
            reconstructed_residual,
        );
        let candidate_score = vvc_scored_chroma_quantized_residual_score(
            residual,
            u64::from(vvc_bdpcm_mode_syntax_bin_count(bdpcm_mode)),
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if candidate_score.selects_over(best_score) {
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

fn vvc_chroma_bdpcm_candidate_modes(
    policy: VvcResidualCodingPolicy,
    co_located_luma_mode: VvcIntraPredictionMode,
) -> [Option<VvcBdpcmMode>; 2] {
    if policy.residual_mode() == VvcResidualCodingMode::Lossy
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
    {
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

fn vvc_chroma_lossy_speed_direct_bdpcm_mode(
    policy: VvcResidualCodingPolicy,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    selected_mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
) -> Option<VvcBdpcmMode> {
    if policy.residual_mode() != VvcResidualCodingMode::Lossy
        || policy.fast_search() != VvcFastSearch::LosslessSpeed
        || chroma_sampling != ChromaSampling::Cs444
        || bit_depth.bits() != 8
        || !matches!(selected_mode, VvcChromaIntraPredictionMode::Derived)
    {
        return None;
    }
    match co_located_luma_mode {
        VvcIntraPredictionMode::Horizontal => Some(VvcBdpcmMode::Horizontal),
        VvcIntraPredictionMode::Vertical => Some(VvcBdpcmMode::Vertical),
        VvcIntraPredictionMode::Planar
        | VvcIntraPredictionMode::Dc
        | VvcIntraPredictionMode::Angular(_) => None,
    }
}

fn vvc_chroma_direct_bdpcm_residual_is_safe(
    selected_cb_residuals: &[i16],
    selected_cr_residuals: &[i16],
    candidate_cb_residuals: &[i16],
    candidate_cr_residuals: &[i16],
) -> bool {
    let selected_sse = vvc_chroma_pair_residual_sse(selected_cb_residuals, selected_cr_residuals);
    let candidate_sse = vvc_chroma_pair_residual_sse(candidate_cb_residuals, candidate_cr_residuals);
    // Bypass the RD check only when BDPCM clearly improves raw prediction SSE.
    candidate_sse.saturating_mul(4) <= selected_sse.saturating_mul(3)
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
        return false;
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
        && chroma_width <= usize::from(VVC_TRANSFORM_SKIP_MAX_SIZE)
        && chroma_height <= usize::from(VVC_TRANSFORM_SKIP_MAX_SIZE)
        && chroma_width >= 4
        && chroma_height >= 4
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

#[derive(Debug, Clone, Copy)]
struct VvcChromaModeRdCandidate {
    distortion: u64,
    rate_cost: u64,
    residual: VvcScoredSelectedChromaResidual,
}

impl VvcChromaModeRdCandidate {
    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn score_vvc_chroma_mode_rd_candidate(
    policy: VvcResidualCodingPolicy,
    coding_decision: VvcChromaTuCodingDecision,
    mode: VvcChromaIntraPredictionMode,
    cclm_syntax_enabled: bool,
    cb_residuals: &[i16],
    cr_residuals: &[i16],
    chroma_width: usize,
    chroma_height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcChromaModeRdCandidate {
    let cb = select_vvc_scored_chroma_residual_block_with_transform_skip(
        policy,
        coding_decision.residual_coding,
        cb_residuals,
        chroma_width,
        chroma_height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let cr = select_vvc_scored_chroma_residual_block_with_transform_skip(
        policy,
        coding_decision.residual_coding,
        cr_residuals,
        chroma_width,
        chroma_height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let residual = VvcSelectedChromaResidual {
        cb: cb.block,
        cr: cr.block,
    };
    let rd_score = VvcResidualBlockScore {
        distortion: cb.score.distortion.saturating_add(cr.score.distortion),
        rate_cost: cb.score.rate_cost.saturating_add(cr.score.rate_cost),
    };
    let residual_score = VvcResidualBlockScore {
        distortion: rd_score.distortion,
        rate_cost: chroma_coeff_syntax_cost_estimate(chroma_width, chroma_height, residual.cb)
            .saturating_add(chroma_coeff_syntax_cost_estimate(
                chroma_width,
                chroma_height,
                residual.cr,
            )),
    };
    let residual = VvcScoredSelectedChromaResidual {
        residual,
        score: residual_score,
    };
    let mode_cost = u64::from(vvc_chroma_intra_mode_syntax_bin_count(
        mode,
        cclm_syntax_enabled,
    ));
    VvcChromaModeRdCandidate {
        distortion: rd_score.distortion,
        rate_cost: rd_score.rate_cost.saturating_add(mode_cost),
        residual,
    }
}

fn vvc_scored_chroma_quantized_residual_score(
    residual: VvcScoredSelectedChromaResidual,
    extra_syntax_cost: u64,
) -> VvcChromaQuantizedResidualScore {
    VvcChromaQuantizedResidualScore {
        distortion: residual.score.distortion,
        rate_cost: residual.score.rate_cost.saturating_add(extra_syntax_cost),
    }
}

fn chroma_reconstructed_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> u64 {
    if residual.transform_skip {
        return chroma_transform_skip_residual_sse(
            source_residuals,
            width,
            height,
            chroma_ts_quant,
            residual,
        );
    }
    reconstruct_vvc_chroma_residual_block_into(
        residual,
        reconstructed_residual,
        transform_scratch,
        width,
        height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
    );
    reconstructed_residual_sse(source_residuals, reconstructed_residual)
}

fn reconstructed_residual_sse(source_residuals: &[i16], reconstructed_residuals: &[i16]) -> u64 {
    let mut sse = 0u64;
    for (&source, &reconstructed) in source_residuals.iter().zip(reconstructed_residuals.iter()) {
        sse += residual_diff_square(source, reconstructed);
    }
    sse
}

fn chroma_transform_skip_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
) -> u64 {
    debug_assert!(residual.transform_skip);
    let mut reconstructed = [0i16; 16];
    let active_width = width.min(4);
    let active_height = height.min(4);
    if active_width != 0 && active_height != 0 {
        reconstructed[0] = residual.dc_level;
        for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
            if x < active_width && y < active_height {
                reconstructed[y * 4 + x] = residual.ac_levels[slot];
            }
        }
        if residual.bdpcm_mode.is_enabled() {
            inverse_bdpcm_quantized_levels_in_place(
                &mut reconstructed,
                4,
                active_height,
                residual.bdpcm_mode,
            );
        }
        for y in 0..active_height {
            let row = y * 4;
            for x in 0..active_width {
                reconstructed[row + x] = chroma_ts_quant.reconstructed(reconstructed[row + x]);
            }
        }
    }
    transform_skip_residual_sse(
        source_residuals,
        width,
        height,
        active_width,
        active_height,
        4,
        &reconstructed,
    )
}

fn chroma_coeff_syntax_cost_estimate(
    width: usize,
    height: usize,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
) -> u64 {
    let mut nonzero = u64::from(residual.dc_level != 0);
    let mut abs_sum = u64::from(residual.dc_level.unsigned_abs());
    let mut last_pos = 0u64;
    let active_width = width.min(4);
    let active_height = height.min(4);
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x >= active_width || y >= active_height {
            continue;
        }
        let abs_level = u64::from(residual.ac_levels[slot].unsigned_abs());
        if abs_level != 0 {
            nonzero += 1;
            abs_sum += abs_level;
            last_pos = (y * active_width + x) as u64;
        }
    }
    nonzero
        .saturating_mul(18)
        .saturating_add(abs_sum.saturating_mul(4))
        .saturating_add(last_pos.saturating_mul(2))
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaQuantizedResidualScore {
    distortion: u64,
    rate_cost: u64,
}

impl VvcChromaQuantizedResidualScore {
    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaModeRdShortlist {
    candidates: [VvcChromaIntraCandidateCost; VVC_CHROMA_INTRA_CANDIDATE_CAPACITY],
    count: usize,
}

impl VvcChromaModeRdShortlist {
    fn from_candidate_costs(
        policy: VvcResidualCodingPolicy,
        costs: VvcChromaIntraCandidateCosts,
    ) -> Self {
        let mut shortlist = Self {
            candidates: [VvcChromaIntraCandidateCost::new(
                VvcChromaIntraPredictionMode::Derived,
                u64::MAX,
            ); VVC_CHROMA_INTRA_CANDIDATE_CAPACITY],
            count: 0,
        };
        for candidate in costs.iter() {
            shortlist.add(candidate);
        }
        shortlist.apply_policy_limit(policy);
        shortlist
    }

    fn add(&mut self, candidate: VvcChromaIntraCandidateCost) {
        if let Some(existing) = self
            .candidates
            .iter()
            .take(self.count)
            .position(|entry| entry.mode() == candidate.mode())
        {
            if candidate.score() < self.candidates[existing].score() {
                self.candidates[existing] = candidate;
                self.sort();
            }
            return;
        }
        if self.count < self.candidates.len() {
            self.candidates[self.count] = candidate;
            self.count += 1;
            self.sort();
            return;
        }
        let worst = self.count - 1;
        if candidate.score() < self.candidates[worst].score() {
            self.candidates[worst] = candidate;
            self.sort();
        }
    }

    fn sort(&mut self) {
        self.candidates[..self.count].sort_by_key(|candidate| candidate.score());
    }

    fn apply_policy_limit(&mut self, policy: VvcResidualCodingPolicy) {
        self.count = self
            .count
            .min(vvc_chroma_mode_rd_shortlist_limit(policy).min(self.candidates.len()));
    }

    fn iter(self) -> impl Iterator<Item = VvcChromaIntraCandidateCost> {
        self.candidates.into_iter().take(self.count)
    }
}

fn vvc_chroma_mode_rd_shortlist_limit(policy: VvcResidualCodingPolicy) -> usize {
    match policy.residual_mode() {
        VvcResidualCodingMode::Lossless => VVC_CHROMA_INTRA_CANDIDATE_CAPACITY,
        VvcResidualCodingMode::Lossy => match policy.fast_search() {
            VvcFastSearch::Off | VvcFastSearch::Conservative | VvcFastSearch::LosslessSpeed => {
                VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES
            }
            VvcFastSearch::Moderate => 3,
            VvcFastSearch::Aggressive => 2,
        },
    }
}
