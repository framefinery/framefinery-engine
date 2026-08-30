fn score_vvc_luma_planar_candidate(
    luma_rd_cache: &mut VvcLumaModeRdCache,
    score_metric: VvcResidualScoreMetric,
    source_frame: &VvcSampledFrame,
    frame_recon: &VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    left_luma_mode: Option<VvcIntraPredictionMode>,
    above_luma_mode: Option<VvcIntraPredictionMode>,
    prediction_scratch: &mut VvcDcPredictionScratch,
    candidate_luma_prediction: &mut Vec<VvcSample>,
    candidate_luma_residuals: &mut Vec<i16>,
    intra_search_stats: &mut VvcIntraSearchStats,
) -> u64 {
    #[cfg(feature = "vvc-stats")]
    let prediction_start = StageStart::now();
    predict_vvc_luma_intra_block_into_with_availability(
        candidate_luma_prediction,
        prediction_scratch,
        VvcIntraPredictionMode::Planar,
        &frame_recon.luma,
        frame_recon.coded_geometry(),
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
    let score_start = StageStart::now();
    let candidate_score = score_luma_mode_candidate(
        luma_rd_cache,
        score_metric,
        VvcIntraPredictionMode::Planar,
        source_frame,
        node,
        candidate_luma_prediction,
        left_luma_mode,
        above_luma_mode,
        candidate_luma_residuals,
        intra_search_stats,
    );
    #[cfg(feature = "vvc-stats")]
    intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
    candidate_score
}

fn vvc_chroma_lossless_speed_skips_near_exact_explicit_search(
    policy: VvcResidualCodingPolicy,
    best_score: u64,
    chroma_width: usize,
    chroma_height: usize,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossless
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
        && best_score <= vvc_chroma_fast_search_near_exact_score(policy, chroma_width, chroma_height)
}

fn vvc_chroma_fast_search_uses_derived_only(policy: VvcResidualCodingPolicy) -> bool {
    // Derived-only chroma is a lossless-speed shortcut for lossless mode.
    // Lossy probes rely on the shared RD selector to reject explicit and CCLM
    // candidates when the derived chroma mode is better.
    policy.fast_search() == VvcFastSearch::LosslessSpeed
        && policy.residual_mode() == VvcResidualCodingMode::Lossless
}

fn vvc_luma_lossless_speed_skips_directional_refinement(
    policy: VvcResidualCodingPolicy,
) -> bool {
    policy.fast_search() == VvcFastSearch::LosslessSpeed
}

fn vvc_luma_lossless_speed_skips_dc(policy: VvcResidualCodingPolicy) -> bool {
    // DC is cheap enough to keep for lossy fast search and improves the RD
    // point on flat or near-flat TUs. Lossless-speed lossless still skips it
    // because transform-skip/BDPCM candidates carry exact reconstruction.
    policy.fast_search() == VvcFastSearch::LosslessSpeed
        && policy.residual_mode() == VvcResidualCodingMode::Lossless
}

fn vvc_luma_lossless_speed_evaluates_planar(
    policy: VvcResidualCodingPolicy,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> bool {
    if policy.fast_search() != VvcFastSearch::LosslessSpeed {
        return true;
    }
    matches!(left, None | Some(VvcIntraPredictionMode::Planar))
        || matches!(above, None | Some(VvcIntraPredictionMode::Planar))
}

fn vvc_chroma_explicit_candidate_allowed_for_search(
    policy: VvcResidualCodingPolicy,
    mode: VvcIntraPredictionMode,
) -> bool {
    if !vvc_residual_chroma_explicit_candidate_allowed(mode) {
        return false;
    }
    if policy.residual_mode() == VvcResidualCodingMode::Lossless
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
    {
        return false;
    }
    if policy.residual_mode() == VvcResidualCodingMode::Lossy
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
        && matches!(mode, VvcIntraPredictionMode::Dc)
    {
        return false;
    }
    true
}

fn vvc_chroma_cclm_fast_search_allowed(
    policy: VvcResidualCodingPolicy,
    best_score: u64,
    chroma_width: usize,
    chroma_height: usize,
) -> bool {
    if policy.residual_mode() == VvcResidualCodingMode::Lossless
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
    {
        return false;
    }
    match policy.fast_search() {
        VvcFastSearch::Off | VvcFastSearch::Conservative => true,
        VvcFastSearch::LosslessSpeed if policy.residual_mode() == VvcResidualCodingMode::Lossy => {
            policy.chroma_sampling() == ChromaSampling::Cs444
        }
        VvcFastSearch::Moderate | VvcFastSearch::LosslessSpeed => {
            best_score > vvc_chroma_cclm_fast_search_score(policy, chroma_width, chroma_height)
        }
        VvcFastSearch::Aggressive => {
            best_score
                > vvc_chroma_fast_search_low_residual_score(policy, chroma_width, chroma_height)
        }
    }
}

fn vvc_chroma_fast_search_near_exact_score(
    policy: VvcResidualCodingPolicy,
    chroma_width: usize,
    chroma_height: usize,
) -> u64 {
    if policy.residual_mode() == VvcResidualCodingMode::Lossless {
        64
    } else {
        (chroma_width as u64)
            .saturating_mul(chroma_height as u64)
            .saturating_mul(2)
            .saturating_mul(64)
    }
}

fn vvc_chroma_cclm_fast_search_score(
    policy: VvcResidualCodingPolicy,
    chroma_width: usize,
    chroma_height: usize,
) -> u64 {
    if policy.residual_mode() == VvcResidualCodingMode::Lossless
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
    {
        256
    } else {
        vvc_chroma_fast_search_near_exact_score(policy, chroma_width, chroma_height)
    }
}

fn vvc_chroma_fast_search_low_residual_score(
    policy: VvcResidualCodingPolicy,
    chroma_width: usize,
    chroma_height: usize,
) -> u64 {
    vvc_chroma_fast_search_near_exact_score(policy, chroma_width, chroma_height)
        .saturating_mul(4)
}
