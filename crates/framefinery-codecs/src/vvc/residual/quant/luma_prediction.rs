fn select_vvc_luma_mrl_prediction(
    policy: VvcResidualCodingPolicy,
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    preselected_residual: Option<VvcScoredSelectedLumaResidual>,
    stats: &mut VvcIntraSearchStats,
    frame_recon: &VvcReconstructionFrame,
    source_frame: &VvcSampledFrame,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_prediction: &mut Vec<VvcSample>,
    selected_residuals: &mut Vec<i16>,
    candidate_prediction: &mut Vec<VvcSample>,
    candidate_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedLumaMrl {
    if !VVC_ENABLE_LUMA_MRL_SELECTION
        || vvc_luma_lossless_speed_skips_mrl(policy)
        || !policy.luma_mrl_candidate_allowed(node, mode)
        || !vvc_luma_intra_mode_is_mpm(mode, left, above)
        || vvc_luma_exact_prediction_skips_rd(selected_residuals)
    {
        return VvcSelectedLumaMrl {
            mrl_index: 0,
            residual: preselected_residual,
        };
    }

    let score_metric = policy.score_metric();
    let mut best_mrl_index = 0u8;
    let mut best_candidate = match preselected_residual {
        Some(residual) => VvcLumaMrlCandidate::from_scored_residual(
            residual,
            u64::from(vvc_luma_mrl_syntax_bin_count(node, 0)),
        ),
        None => {
            #[cfg(feature = "vvc-stats")]
            let score_start = Instant::now();
            let candidate = score_vvc_luma_mrl_candidate(
                score_metric,
                residual_coding,
                requested_mts_index,
                node,
                0,
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
            candidate
        }
    };
    for mrl_index in 1..=2 {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_luma_intra_block_into_with_mrl_and_availability(
            candidate_prediction,
            prediction_scratch,
            mode,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.bit_depth,
            mrl_index,
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(prediction_start);
            stats.add_luma_prediction_nanos(VvcLumaPredictionStatsFamily::Mrl, nanos);
            stats.add_luma_rd_prediction_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
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
            stats.add_luma_residual_build_nanos(nanos);
            stats.add_luma_rd_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let candidate = score_vvc_luma_mrl_candidate(
            score_metric,
            residual_coding,
            requested_mts_index,
            node,
            mrl_index,
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
        if candidate.selects_over(best_candidate) {
            best_candidate = candidate;
            best_mrl_index = mrl_index;
            std::mem::swap(selected_prediction, candidate_prediction);
            std::mem::swap(selected_residuals, candidate_residuals);
        }
    }
    VvcSelectedLumaMrl {
        mrl_index: best_mrl_index,
        residual: best_candidate.residual,
    }
}

fn vvc_luma_lossless_speed_skips_mrl(policy: VvcResidualCodingPolicy) -> bool {
    policy.fast_search() == VvcFastSearch::LosslessSpeed
}

fn select_vvc_luma_bdpcm_prediction(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    selected_mode: VvcIntraPredictionMode,
    selected_decision: VvcLumaTuCodingDecision,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    selected_residual: Option<VvcScoredSelectedLumaResidual>,
    stats: &mut VvcIntraSearchStats,
    frame_recon: &VvcReconstructionFrame,
    source_frame: &VvcSampledFrame,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_prediction: &mut Vec<VvcSample>,
    selected_residuals: &mut Vec<i16>,
    candidate_prediction: &mut Vec<VvcSample>,
    candidate_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcSelectedLumaBdpcm> {
    if !vvc_luma_bdpcm_selection_allowed(policy, node)
        || !vvc_luma_bdpcm_fast_search_allowed(policy, selected_mode, left, above)
    {
        return None;
    }

    let baseline_residual = selected_residual.unwrap_or_else(|| {
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let scored_residual = select_vvc_scored_luma_residual_block_with_mts(
            selected_decision.residual_coding,
            selected_decision.mts_index,
            selected_residuals,
            node.width,
            node.height,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            true,
            VvcLumaResidualQuantizationSearch::Full,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        VvcScoredSelectedLumaResidual::from_scored_block(scored_residual)
    });
    let mut best_score = vvc_scored_luma_quantized_residual_score(
        baseline_residual,
        vvc_luma_regular_prediction_syntax_cost(
            node,
            selected_mode,
            left,
            above,
            selected_decision,
        ),
    );
    let mut best = None;
    let direct_residual = policy.fast_search() == VvcFastSearch::LosslessSpeed;
    let mut best_bdpcm_mode = None;

    for bdpcm_mode in [VvcBdpcmMode::Horizontal, VvcBdpcmMode::Vertical] {
        if direct_residual {
            #[cfg(feature = "vvc-stats")]
            let residual_start = Instant::now();
            residual_vvc_luma_bdpcm_block_into_with_availability(
                candidate_residuals,
                prediction_scratch,
                bdpcm_mode,
                &source_frame.luma,
                &frame_recon.luma,
                source_frame.geometry,
                node,
                source_frame.format.bit_depth,
                Some(frame_recon.luma_availability()),
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        } else {
            #[cfg(feature = "vvc-stats")]
            let prediction_start = Instant::now();
            predict_vvc_luma_bdpcm_block_into_with_availability(
                candidate_prediction,
                prediction_scratch,
                bdpcm_mode,
                &frame_recon.luma,
                source_frame.geometry,
                node,
                source_frame.format.bit_depth,
                Some(frame_recon.luma_availability()),
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_prediction_nanos(
                VvcLumaPredictionStatsFamily::Bdpcm,
                vvc_elapsed_nanos(prediction_start),
            );
            #[cfg(feature = "vvc-stats")]
            let residual_start = Instant::now();
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
            stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        }
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let residual = VvcSelectedLumaResidual {
            block: finalize_vvc_luma_bdpcm_transform_skip_residual_block(
                candidate_residuals,
                node.width,
                node.height,
                luma_ts_quant,
                bdpcm_mode,
            ),
            mts_index: 0,
        };
        let residual = VvcScoredSelectedLumaResidual::new(
            candidate_residuals,
            node,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            residual,
            transform_scratch,
            reconstructed_residual,
        );
        let candidate_score = vvc_scored_luma_quantized_residual_score(
            residual,
            u64::from(vvc_bdpcm_mode_syntax_bin_count(bdpcm_mode)),
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if candidate_score.selects_over(best_score) {
            best_score = candidate_score;
            best_bdpcm_mode = Some(bdpcm_mode);
            let mode = bdpcm_mode
                .inferred_intra_mode()
                .expect("enabled BDPCM mode has an inferred intra mode");
            best = Some(VvcSelectedLumaBdpcm {
                mode,
                coding_decision: VvcLumaTuCodingDecision {
                    residual_coding: VvcTuResidualCodingMode::TransformSkip,
                    mrl_index: 0,
                    mts_index: 0,
                },
                residual,
            });
            if !direct_residual {
                std::mem::swap(selected_prediction, candidate_prediction);
            }
            std::mem::swap(selected_residuals, candidate_residuals);
        }
    }

    if direct_residual {
        let Some(bdpcm_mode) = best_bdpcm_mode else {
            return best;
        };
        #[cfg(feature = "vvc-stats")]
        {
            let prediction_start = Instant::now();
            predict_vvc_luma_bdpcm_block_into_with_availability(
                selected_prediction,
                prediction_scratch,
                bdpcm_mode,
                &frame_recon.luma,
                source_frame.geometry,
                node,
                source_frame.format.bit_depth,
                Some(frame_recon.luma_availability()),
            );
            stats.add_luma_prediction_nanos(
                VvcLumaPredictionStatsFamily::Bdpcm,
                vvc_elapsed_nanos(prediction_start),
            );
        }
        #[cfg(not(feature = "vvc-stats"))]
        if best.as_ref().is_some_and(|selected| {
            !(vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, luma_qp)
                && vvc_luma_transform_skip_score_is_exact(
                    selected.residual.residual.block,
                    node.width,
                    node.height,
                    source_frame.format.bit_depth,
                    luma_qp,
                ))
        }) {
            predict_vvc_luma_bdpcm_block_into_with_availability(
                selected_prediction,
                prediction_scratch,
                bdpcm_mode,
                &frame_recon.luma,
                source_frame.geometry,
                node,
                source_frame.format.bit_depth,
                Some(frame_recon.luma_availability()),
            );
        }
    }

    best
}

fn vvc_luma_bdpcm_fast_search_allowed(
    policy: VvcResidualCodingPolicy,
    selected_mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> bool {
    match policy.fast_search() {
        VvcFastSearch::Off | VvcFastSearch::Conservative => true,
        VvcFastSearch::LosslessSpeed if policy.residual_mode() == VvcResidualCodingMode::Lossy => {
            true
        }
        VvcFastSearch::Moderate | VvcFastSearch::LosslessSpeed => {
            vvc_luma_mode_is_bdpcm_aligned(selected_mode)
                || left.is_some_and(vvc_luma_mode_is_bdpcm_aligned)
                || above.is_some_and(vvc_luma_mode_is_bdpcm_aligned)
        }
        VvcFastSearch::Aggressive => vvc_luma_mode_is_bdpcm_aligned(selected_mode),
    }
}

fn vvc_luma_mode_is_bdpcm_aligned(mode: VvcIntraPredictionMode) -> bool {
    matches!(
        mode,
        VvcIntraPredictionMode::Horizontal | VvcIntraPredictionMode::Vertical
    )
}

fn vvc_luma_bdpcm_selection_allowed(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
) -> bool {
    VVC_ENABLE_BDPCM_SELECTION
        && node.width <= VVC_TRANSFORM_SKIP_MAX_SIZE
        && node.height <= VVC_TRANSFORM_SKIP_MAX_SIZE
        && node.width >= 4
        && node.height >= 4
        && node.width.is_power_of_two()
        && node.height.is_power_of_two()
        && matches!(
            policy.residual_mode(),
            VvcResidualCodingMode::Lossy | VvcResidualCodingMode::Lossless
        )
}

fn vvc_luma_regular_prediction_syntax_cost(
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    decision: VvcLumaTuCodingDecision,
) -> u64 {
    u64::from(vvc_bdpcm_mode_syntax_bin_count(VvcBdpcmMode::None))
        .saturating_add(u64::from(vvc_luma_mrl_syntax_bin_count(
            node,
            decision.mrl_index,
        )))
        .saturating_add(u64::from(vvc_luma_intra_mode_syntax_bin_count(
            mode, left, above,
        )))
}

fn vvc_bdpcm_mode_syntax_bin_count(mode: VvcBdpcmMode) -> u8 {
    match mode {
        VvcBdpcmMode::None => 1,
        VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => 2,
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedLumaBdpcm {
    mode: VvcIntraPredictionMode,
    coding_decision: VvcLumaTuCodingDecision,
    residual: VvcScoredSelectedLumaResidual,
}

#[derive(Debug, Clone, Copy, Default)]
struct VvcSelectedLumaMrl {
    mrl_index: u8,
    residual: Option<VvcScoredSelectedLumaResidual>,
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaMrlCandidate {
    distortion: u64,
    rate_cost: u64,
    residual: Option<VvcScoredSelectedLumaResidual>,
}

impl VvcLumaMrlCandidate {
    fn from_scored_residual(
        residual: VvcScoredSelectedLumaResidual,
        extra_syntax_cost: u64,
    ) -> Self {
        let score = vvc_scored_luma_quantized_residual_score(residual, extra_syntax_cost);
        Self {
            distortion: score.distortion,
            rate_cost: score.rate_cost,
            residual: Some(residual),
        }
    }

    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn score_vvc_luma_mrl_candidate(
    metric: VvcResidualScoreMetric,
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    node: VvcCodingTreeNode,
    mrl_index: u8,
    residuals: &[i16],
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcLumaMrlCandidate {
    if matches!(residual_coding, VvcTuResidualCodingMode::Transformed) {
        let scored_residual = select_vvc_scored_luma_residual_block_with_mts(
            residual_coding,
            requested_mts_index,
            residuals,
            node.width,
            node.height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            false,
            VvcLumaResidualQuantizationSearch::FastModeDecision,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        let mrl_cost = u64::from(vvc_luma_mrl_syntax_bin_count(node, mrl_index));
        let selected_residual = VvcScoredSelectedLumaResidual::from_scored_block(scored_residual);
        return VvcLumaMrlCandidate {
            distortion: scored_residual.score.distortion,
            rate_cost: scored_residual.score.rate_cost.saturating_add(mrl_cost),
            residual: Some(selected_residual),
        };
    }
    VvcLumaMrlCandidate {
        distortion: residual_mode_selection_score(metric, residuals),
        rate_cost: u64::from(vvc_luma_mrl_syntax_bin_count(node, mrl_index)),
        residual: None,
    }
}

fn vvc_scored_luma_quantized_residual_score(
    residual: VvcScoredSelectedLumaResidual,
    extra_syntax_cost: u64,
) -> VvcLumaQuantizedResidualScore {
    VvcLumaQuantizedResidualScore {
        distortion: residual.score.distortion,
        rate_cost: residual.score.rate_cost.saturating_add(extra_syntax_cost),
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaQuantizedResidualScore {
    distortion: u64,
    rate_cost: u64,
}

impl VvcLumaQuantizedResidualScore {
    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn vvc_quality_candidate_selects_over(
    distortion: u64,
    rate_cost: u64,
    best_distortion: u64,
    best_rate_cost: u64,
) -> bool {
    distortion < best_distortion || (distortion == best_distortion && rate_cost < best_rate_cost)
}

fn luma_reconstructed_residual_sse(
    source_residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    qp: i32,
    ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> u64 {
    if residual.transform_skip {
        return luma_transform_skip_residual_sse(
            source_residuals,
            usize::from(width),
            usize::from(height),
            ts_quant,
            residual,
        );
    }
    luma_reconstructed_residual_sse_with_mts_into(
        source_residuals,
        width,
        height,
        bit_depth,
        qp,
        residual.dc_level,
        &residual.ac_levels,
        mts_index,
        transform_scratch,
        reconstructed_residual,
    )
}

fn luma_transform_skip_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
) -> u64 {
    debug_assert!(residual.transform_skip);
    let (active_width, active_height) = vvc_luma_transform_skip_active_extent(width, height);
    let mut reconstructed = [0i16; 64];
    if active_width != 0 && active_height != 0 {
        reconstructed[0] = residual.dc_level;
        for y in 0..active_height {
            for x in 0..active_width {
                if x == 0 && y == 0 {
                    continue;
                }
                reconstructed[y * active_width + x] = residual.ac_levels[y * active_width + x - 1];
            }
        }
        if residual.bdpcm_mode.is_enabled() {
            inverse_bdpcm_quantized_levels_in_place(
                &mut reconstructed,
                active_width,
                active_height,
                residual.bdpcm_mode,
            );
        }
        for y in 0..active_height {
            let row = y * active_width;
            for x in 0..active_width {
                reconstructed[row + x] = ts_quant.reconstructed(reconstructed[row + x]);
            }
        }
    }
    transform_skip_residual_sse(
        source_residuals,
        width,
        height,
        active_width,
        active_height,
        active_width,
        &reconstructed,
    )
}

fn transform_skip_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    active_width: usize,
    active_height: usize,
    reconstructed_stride: usize,
    reconstructed: &[i16],
) -> u64 {
    debug_assert_eq!(source_residuals.len(), width * height);
    let mut sse = 0u64;
    let active_width = active_width.min(width);
    let active_height = active_height.min(height);
    for y in 0..active_height {
        let source_row = &source_residuals[y * width..(y + 1) * width];
        let reconstructed_row =
            &reconstructed[y * reconstructed_stride..y * reconstructed_stride + active_width];
        for x in 0..active_width {
            sse += residual_diff_square(source_row[x], reconstructed_row[x]);
        }
        for &source in &source_row[active_width..] {
            sse += residual_square(source);
        }
    }
    for y in active_height..height {
        let source_row = &source_residuals[y * width..(y + 1) * width];
        for &source in source_row {
            sse += residual_square(source);
        }
    }
    sse
}

#[inline]
fn residual_square(sample: i16) -> u64 {
    let sample = i64::from(sample);
    (sample * sample) as u64
}

#[inline]
fn residual_diff_square(source: i16, reconstructed: i16) -> u64 {
    let diff = i64::from(source) - i64::from(reconstructed);
    (diff * diff) as u64
}

fn vvc_luma_mrl_syntax_bin_count(node: VvcCodingTreeNode, mrl_index: u8) -> u8 {
    if node.y % VVC_CTU_SIZE as u16 == 0 {
        0
    } else if mrl_index == 0 {
        1
    } else {
        2
    }
}

fn finalized_vvc_chroma_sample(
    transform_skip: bool,
    source: VvcSample,
    quantized_remainder: u8,
    bit_depth: SampleBitDepth,
) -> u8 {
    if transform_skip {
        vvc_downshift_sample_to_u8(source, bit_depth)
    } else {
        reconstruct_vvc_chroma(quantized_remainder)
    }
}
