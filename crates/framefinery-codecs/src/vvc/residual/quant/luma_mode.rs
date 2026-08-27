fn score_luma_mode_candidate(
    cache: &mut VvcLumaModeRdCache,
    metric: VvcResidualScoreMetric,
    mode: VvcIntraPredictionMode,
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    residuals: &mut Vec<i16>,
    stats: &mut VvcIntraSearchStats,
) -> u64 {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if cache.materializes_mode_search_residuals() {
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_luma_tu_at_into(
            residuals,
            source_frame,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
            predicted,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        let score = luma_residual_mode_selection_score(metric, residuals, left, above, mode);
        cache.consider(mode, score, residuals);
        score
    } else {
        luma_prediction_mode_selection_score(
            metric,
            source_frame,
            node,
            predicted,
            left,
            above,
            mode,
        )
    }
}

fn score_chroma_mode_candidate(
    cache: &mut VvcChromaModeRdCache,
    metric: VvcResidualScoreMetric,
    mode: VvcChromaIntraPredictionMode,
    source_frame: &VvcSampledFrame,
    chroma_x: usize,
    chroma_y: usize,
    chroma_width: usize,
    chroma_height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    cclm_enabled: bool,
    syntax_tie_breaker_enabled: bool,
    cb_residuals: &mut Vec<i16>,
    cr_residuals: &mut Vec<i16>,
    stats: &mut VvcIntraSearchStats,
) -> u64 {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if cache.materializes_mode_search_residuals() {
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_pair_tu_at_into(
            cb_residuals,
            cr_residuals,
            &source_frame.cb,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cb,
            predicted_cr,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        let score = chroma_residual_mode_selection_score(
            metric,
            cb_residuals,
            cr_residuals,
            mode,
            cclm_enabled,
            syntax_tie_breaker_enabled,
        );
        cache.consider(mode, score, cb_residuals, cr_residuals);
        score
    } else {
        chroma_prediction_mode_selection_score(
            metric,
            source_frame,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cb,
            predicted_cr,
            mode,
            cclm_enabled,
            syntax_tie_breaker_enabled,
        )
    }
}

fn select_vvc_luma_mode_with_rd_refinement(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    raw_mode: VvcIntraPredictionMode,
    candidate_costs: VvcLumaIntraCandidateCosts,
    rd_cache: &VvcLumaModeRdCache,
    stats: &mut VvcIntraSearchStats,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    source_frame: &VvcSampledFrame,
    frame_recon: &VvcReconstructionFrame,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_prediction: &mut Vec<VvcSample>,
    selected_residuals: &mut Vec<i16>,
    candidate_prediction: &mut Vec<VvcSample>,
    candidate_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedLumaMode {
    let raw_decision = policy.select_luma_tu_coding_decision(node, raw_mode);
    if !vvc_luma_lossy_rd_refinement_allowed(policy, node, raw_decision) {
        return VvcSelectedLumaMode {
            mode: raw_mode,
            residual: None,
        };
    }
    if vvc_luma_exact_prediction_skips_rd(selected_residuals) {
        return VvcSelectedLumaMode {
            mode: raw_mode,
            residual: None,
        };
    }

    let mut best_mode = raw_mode;
    #[cfg(feature = "vvc-stats")]
    let score_start = StageStart::now();
    let mut best_candidate = score_vvc_luma_mode_rd_candidate(
        policy,
        raw_decision,
        node,
        raw_mode,
        left,
        above,
        selected_residuals,
        source_frame.format.bit_depth,
        luma_qp,
        luma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    if vvc_luma_zero_coded_residual_skips_rd(best_candidate.residual, selected_residuals.len()) {
        return VvcSelectedLumaMode {
            mode: raw_mode,
            residual: Some(best_candidate.residual),
        };
    }
    let shortlist = VvcLumaModeRdShortlist::from_candidate_costs(policy, node, candidate_costs);
    for candidate in shortlist.iter() {
        if policy.residual_mode() == VvcResidualCodingMode::Lossy
            && policy.fast_search() == VvcFastSearch::LosslessSpeed
            && !shortlist.admits_lossless_speed_rd(candidate)
        {
            continue;
        }
        let mode = candidate.mode();
        if mode.luma_mode_index() == raw_mode.luma_mode_index() {
            continue;
        }
        let coding_decision = policy.select_luma_tu_coding_decision(node, mode);
        if !matches!(
            coding_decision.residual_coding,
            VvcTuResidualCodingMode::Transformed
        ) {
            continue;
        }
        if let Some(cached) = rd_cache.get(mode) {
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_rd_cached_candidate();
            #[cfg(feature = "vvc-stats")]
            let score_start = StageStart::now();
            let rd_candidate = score_vvc_luma_mode_rd_candidate(
                policy,
                coding_decision,
                node,
                mode,
                left,
                above,
                &cached.residuals,
                source_frame.format.bit_depth,
                luma_qp,
                luma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
            if rd_candidate.selects_over(best_candidate) {
                best_mode = mode;
                best_candidate = rd_candidate;
                #[cfg(feature = "vvc-stats")]
                let prediction_start = StageStart::now();
                predict_vvc_luma_intra_block_into_with_availability(
                    selected_prediction,
                    prediction_scratch,
                    mode,
                    &frame_recon.luma,
                    frame_recon.coded_geometry(),
                    node,
                    source_frame.format.bit_depth,
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                stats.add_luma_rd_prediction_nanos(vvc_elapsed_nanos(prediction_start));
                selected_residuals.clear();
                selected_residuals.extend_from_slice(&cached.residuals);
            }
            continue;
        }
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_generated_candidate();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        predict_vvc_luma_intra_block_into_with_availability(
            candidate_prediction,
            prediction_scratch,
            mode,
            &frame_recon.luma,
            frame_recon.coded_geometry(),
            node,
            source_frame.format.bit_depth,
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(prediction_start);
            stats.add_luma_rd_prediction_nanos(nanos);
            stats.add_luma_prediction_nanos(vvc_luma_prediction_stats_family(mode), nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_luma_tu_at_into(
            candidate_residuals,
            source_frame,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
            candidate_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(residual_start);
            stats.add_luma_rd_residual_build_nanos(nanos);
            stats.add_luma_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let score_start = StageStart::now();
        let rd_candidate = score_vvc_luma_mode_rd_candidate(
            policy,
            coding_decision,
            node,
            mode,
            left,
            above,
            candidate_residuals,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if rd_candidate.selects_over(best_candidate) {
            best_mode = mode;
            best_candidate = rd_candidate;
            std::mem::swap(selected_prediction, candidate_prediction);
            std::mem::swap(selected_residuals, candidate_residuals);
        }
    }

    VvcSelectedLumaMode {
        mode: best_mode,
        residual: Some(best_candidate.residual),
    }
}

fn vvc_luma_lossy_rd_refinement_allowed(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    decision: VvcLumaTuCodingDecision,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && matches!(
            decision.residual_coding,
            VvcTuResidualCodingMode::Transformed
        )
        && [4, 8, 16, 32].contains(&node.width)
        && [4, 8, 16, 32].contains(&node.height)
}

fn vvc_luma_exact_prediction_skips_rd(residuals: &[i16]) -> bool {
    residuals.iter().all(|residual| *residual == 0)
}

fn vvc_luma_zero_coded_residual_skips_rd(
    residual: VvcScoredSelectedLumaResidual,
    sample_count: usize,
) -> bool {
    residual.residual.block.dc_level == 0
        && !residual.residual.block.has_ac
        && residual.score.distortion <= sample_count as u64
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedLumaMode {
    mode: VvcIntraPredictionMode,
    residual: Option<VvcScoredSelectedLumaResidual>,
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaModeRdCandidate {
    distortion: u64,
    rate_cost: u64,
    residual: VvcScoredSelectedLumaResidual,
}

impl VvcLumaModeRdCandidate {
    fn selects_over(self, best: Self) -> bool {
        vvc_rd_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn score_vvc_luma_mode_rd_candidate(
    policy: VvcResidualCodingPolicy,
    coding_decision: VvcLumaTuCodingDecision,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    residuals: &[i16],
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcLumaModeRdCandidate {
    let quantization_search =
        if vvc_luma_fast_search_prefers_transform_skip_candidate(policy) {
            VvcLumaResidualQuantizationSearch::TransformSkipFirstModeDecision
        } else {
            VvcLumaResidualQuantizationSearch::FastModeDecision
        };
    let scored_residual = select_vvc_scored_luma_residual_block_with_mts(
        coding_decision.residual_coding,
        coding_decision.mts_index,
        residuals,
        node.width,
        node.height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        false,
        quantization_search,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let residual = VvcScoredSelectedLumaResidual::from_scored_block(scored_residual);
    let mode_cost = u64::from(vvc_luma_intra_mode_syntax_bin_count(mode, left, above));
    VvcLumaModeRdCandidate {
        distortion: scored_residual.score.distortion,
        rate_cost: scored_residual.score.rate_cost.saturating_add(mode_cost),
        residual,
    }
}

fn vvc_luma_fast_search_prefers_transform_skip_candidate(
    policy: VvcResidualCodingPolicy,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaModeRdShortlist {
    candidates: [VvcLumaIntraCandidateCost; VVC_LUMA_INTRA_CANDIDATE_CAPACITY],
    count: usize,
}

impl VvcLumaModeRdShortlist {
    fn from_candidate_costs(
        policy: VvcResidualCodingPolicy,
        node: VvcCodingTreeNode,
        costs: VvcLumaIntraCandidateCosts,
    ) -> Self {
        let mut shortlist = Self {
            candidates: [VvcLumaIntraCandidateCost::new(VvcIntraPredictionMode::Dc, u64::MAX);
                VVC_LUMA_INTRA_CANDIDATE_CAPACITY],
            count: 0,
        };
        for candidate in costs.iter() {
            shortlist.add(candidate);
        }
        shortlist.apply_policy_limit(policy, node);
        shortlist
    }

    fn add(&mut self, candidate: VvcLumaIntraCandidateCost) {
        if let Some(existing) =
            self.candidates.iter().take(self.count).position(|entry| {
                entry.mode().luma_mode_index() == candidate.mode().luma_mode_index()
            })
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

    fn apply_policy_limit(&mut self, policy: VvcResidualCodingPolicy, _node: VvcCodingTreeNode) {
        self.count = self
            .count
            .min(vvc_luma_mode_rd_shortlist_limit(policy).min(self.candidates.len()));
    }

    fn iter(&self) -> impl Iterator<Item = VvcLumaIntraCandidateCost> + '_ {
        self.candidates[..self.count].iter().copied()
    }

    fn admits_lossless_speed_rd(self, candidate: VvcLumaIntraCandidateCost) -> bool {
        self.count <= 1
            || candidate.score()
                <= self.candidates[0]
                    .score()
                    .saturating_mul(2)
    }
}

fn vvc_luma_mode_rd_shortlist_limit(policy: VvcResidualCodingPolicy) -> usize {
    match policy.residual_mode() {
        VvcResidualCodingMode::Lossless => VVC_LUMA_INTRA_CANDIDATE_CAPACITY,
        VvcResidualCodingMode::Lossy => match policy.fast_search() {
            VvcFastSearch::Off | VvcFastSearch::Conservative => VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES,
            VvcFastSearch::LosslessSpeed => vvc_luma_lossless_speed_rd_shortlist_limit(policy),
            VvcFastSearch::Moderate => 3,
            VvcFastSearch::Aggressive => 2,
        },
    }
}

fn vvc_luma_lossless_speed_rd_shortlist_limit(policy: VvcResidualCodingPolicy) -> usize {
    if policy.chroma_sampling() == ChromaSampling::Cs444 && policy.bit_depth().bits() == 8 {
        1
    } else {
        2
    }
}
