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
    VvcQuantizedResidualFrame {
        quantized,
        reconstruction_yuv: reconstruction.to_sample_yuv(),
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
    let mut scratch = VvcCtuQuantScratch::default();
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch(
        source_frame,
        frame_recon,
        region,
        policy,
        luma_qp,
        chroma_qp,
        &mut luma_mode_search_state,
        &transform_skip_quant_tables,
        &mut scratch,
    )
}

pub(in crate::vvc) struct VvcCtuQuantScratch {
    luma_nodes: Vec<VvcCodingTreeNode>,
    chroma_nodes: Vec<VvcCodingTreeNode>,
    prediction_scratch: VvcDcPredictionScratch,
    predicted_luma: Vec<VvcSample>,
    predicted_cb: Vec<VvcSample>,
    predicted_cr: Vec<VvcSample>,
    transform_scratch: VvcInverseTransformScratch,
    reconstructed_residual: Vec<i16>,
    luma_residuals: Vec<i16>,
    candidate_luma_prediction: Vec<VvcSample>,
    candidate_luma_residuals: Vec<i16>,
    luma_rd_cache: VvcLumaModeRdCache,
    cb_residuals: Vec<i16>,
    cr_residuals: Vec<i16>,
    candidate_cb_prediction: Vec<VvcSample>,
    candidate_cr_prediction: Vec<VvcSample>,
    candidate_cb_residuals: Vec<i16>,
    candidate_cr_residuals: Vec<i16>,
    chroma_rd_cache: VvcChromaModeRdCache,
}

impl Default for VvcCtuQuantScratch {
    fn default() -> Self {
        Self {
            luma_nodes: Vec::new(),
            chroma_nodes: Vec::new(),
            prediction_scratch: VvcDcPredictionScratch::default(),
            predicted_luma: Vec::new(),
            predicted_cb: Vec::new(),
            predicted_cr: Vec::new(),
            transform_scratch: VvcInverseTransformScratch::default(),
            reconstructed_residual: Vec::new(),
            luma_residuals: Vec::new(),
            candidate_luma_prediction: Vec::new(),
            candidate_luma_residuals: Vec::new(),
            luma_rd_cache: VvcLumaModeRdCache::new(),
            cb_residuals: Vec::new(),
            cr_residuals: Vec::new(),
            candidate_cb_prediction: Vec::new(),
            candidate_cr_prediction: Vec::new(),
            candidate_cb_residuals: Vec::new(),
            candidate_cr_residuals: Vec::new(),
            chroma_rd_cache: VvcChromaModeRdCache::new(),
        }
    }
}

// Wider temporal reuse avoids intra-search work on changed predictive CTUs.
// The 50-frame screen-content sweep kept lossless PSNR exact and showed a
// better speed/byte tradeoff at 16 than adjacent wider thresholds.
const VVC_TEMPORAL_MODE_HINT_MAX_AVG_ABS_RESIDUAL_8BIT: u64 = 16;
const VVC_LOSSY_TEMPORAL_MODE_HINT_MAX_AVG_ABS_RESIDUAL_8BIT: u64 = 0;

#[cfg(feature = "bench-internals")]
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
    let mut scratch = VvcCtuQuantScratch::default();
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch(
        source_frame,
        frame_recon,
        region,
        policy,
        luma_qp,
        chroma_qp,
        luma_mode_search_state,
        transform_skip_quant_tables,
        &mut scratch,
    )
}

pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    luma_qp: i32,
    chroma_qp: i32,
    luma_mode_search_state: &mut VvcLumaModeSearchState,
    transform_skip_quant_tables: &VvcTransformSkipQuantTables,
    scratch: &mut VvcCtuQuantScratch,
) -> VvcQuantizedColor {
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch_with_mode_hints(
        source_frame,
        frame_recon,
        region,
        policy,
        luma_qp,
        chroma_qp,
        luma_mode_search_state,
        transform_skip_quant_tables,
        scratch,
        None,
        None,
        None,
        None,
        None,
        None,
    )
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaTemporalModeHint {
    mode: VvcIntraPredictionMode,
    bdpcm_mode: VvcBdpcmMode,
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaTemporalModeHint {
    mode: VvcChromaIntraPredictionMode,
    bdpcm_mode: VvcBdpcmMode,
}

fn vvc_luma_temporal_mode_hint(
    hints: Option<&VvcQuantizedColor>,
    tu_idx: usize,
    expected_tu_count: usize,
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
) -> Option<VvcLumaTemporalModeHint> {
    let hints = hints?;
    if !vvc_temporal_mode_hints_allowed(policy)
        || hints.luma_tu_count != expected_tu_count
        || tu_idx >= hints.luma_tu_count
    {
        return None;
    }
    let mut bdpcm_mode = hints.luma_tu_bdpcm_modes[tu_idx];
    if bdpcm_mode.is_enabled() && !vvc_luma_bdpcm_selection_allowed(policy, node) {
        bdpcm_mode = VvcBdpcmMode::None;
    }
    let mode = bdpcm_mode
        .inferred_intra_mode()
        .unwrap_or(hints.luma_tu_intra_modes[tu_idx]);
    Some(VvcLumaTemporalModeHint { mode, bdpcm_mode })
}

fn vvc_chroma_temporal_mode_hint(
    hints: Option<&VvcQuantizedColor>,
    tu_idx: usize,
    expected_tu_count: usize,
    policy: VvcResidualCodingPolicy,
    source_geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    co_located_luma_mode: VvcIntraPredictionMode,
    chroma_width: usize,
    chroma_height: usize,
) -> Option<VvcChromaTemporalModeHint> {
    let hints = hints?;
    if !vvc_temporal_mode_hints_allowed(policy)
        || (policy.residual_mode() == VvcResidualCodingMode::Lossy
            && policy.chroma_sampling() == ChromaSampling::Cs444)
        || hints.chroma_tu_count != expected_tu_count
        || tu_idx >= hints.chroma_tu_count
    {
        return None;
    }
    let mut bdpcm_mode = hints.chroma_tu_bdpcm_modes[tu_idx];
    if bdpcm_mode.is_enabled()
        && !vvc_chroma_bdpcm_selection_allowed(policy, chroma_width, chroma_height)
    {
        bdpcm_mode = VvcBdpcmMode::None;
    }
    let mode = if let Some(mode) = bdpcm_mode.inferred_intra_mode() {
        VvcChromaIntraPredictionMode::Explicit(mode)
    } else {
        vvc_supported_temporal_chroma_mode_hint(
            hints.chroma_tu_intra_modes[tu_idx],
            policy,
            source_geometry,
            node,
            co_located_luma_mode,
        )
    };
    Some(VvcChromaTemporalModeHint { mode, bdpcm_mode })
}

fn vvc_supported_temporal_chroma_mode_hint(
    mode: VvcChromaIntraPredictionMode,
    policy: VvcResidualCodingPolicy,
    source_geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    co_located_luma_mode: VvcIntraPredictionMode,
) -> VvcChromaIntraPredictionMode {
    match mode {
        VvcChromaIntraPredictionMode::Cclm(_)
            if !policy.chroma_cclm_candidate_allowed(node, source_geometry) =>
        {
            VvcChromaIntraPredictionMode::Derived
        }
        VvcChromaIntraPredictionMode::Explicit(mode)
            if !vvc_chroma_explicit_candidate_allowed_for_search(policy, mode)
                || vvc_chroma_explicit_candidate_index(mode, co_located_luma_mode).is_none() =>
        {
            VvcChromaIntraPredictionMode::Derived
        }
        mode => mode,
    }
}

fn vvc_temporal_mode_hint_residual_is_cheap(
    residuals: &[i16],
    sample_count: usize,
    bit_depth: SampleBitDepth,
    policy: VvcResidualCodingPolicy,
) -> bool {
    if sample_count == 0 || residuals.len() != sample_count {
        return false;
    }
    let Some(max_avg_abs_residual_8bit) = vvc_temporal_mode_hint_max_avg_abs_residual_8bit(policy)
    else {
        return false;
    };
    let scale = 1u64 << u32::from(bit_depth.bits().saturating_sub(8));
    let budget = (sample_count as u64)
        .saturating_mul(max_avg_abs_residual_8bit)
        .saturating_mul(scale);
    residuals
        .iter()
        .map(|sample| u64::from(sample.unsigned_abs()))
        .try_fold(0u64, |sum, abs| {
            let sum = sum.saturating_add(abs);
            (sum <= budget).then_some(sum)
        })
        .is_some()
}

fn vvc_temporal_mode_hints_allowed(policy: VvcResidualCodingPolicy) -> bool {
    vvc_temporal_mode_hint_max_avg_abs_residual_8bit(policy).is_some()
}

fn vvc_temporal_mode_hint_max_avg_abs_residual_8bit(
    policy: VvcResidualCodingPolicy,
) -> Option<u64> {
    if policy.fast_search() != VvcFastSearch::LosslessSpeed {
        return None;
    }
    match policy.residual_mode() {
        VvcResidualCodingMode::Lossless => {
            Some(VVC_TEMPORAL_MODE_HINT_MAX_AVG_ABS_RESIDUAL_8BIT)
        }
        VvcResidualCodingMode::Lossy => {
            Some(VVC_LOSSY_TEMPORAL_MODE_HINT_MAX_AVG_ABS_RESIDUAL_8BIT)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finalize_vvc_luma_exact_explicit_inter_candidate(
    decision: VvcLumaInterDecision,
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    inter_reference: &VvcReconstructionFrame,
    frame_recon: &mut VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    inter_prediction: &mut Vec<VvcSample>,
    inter_residuals: &mut Vec<i16>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcFinalizedLumaTu> {
    if source_frame.format.chroma_sampling != ChromaSampling::Cs444 {
        return None;
    }
    if decision.mv_x == 0 && decision.mv_y == 0 {
        return None;
    }
    if !VvcReconstructionFrame::predict_luma_node_from_inter_motion_into(
        inter_reference,
        inter_prediction,
        node,
        decision,
    ) {
        return None;
    }
    if !vvc_luma_prediction_matches_source(source_frame, node, inter_prediction) {
        return None;
    }
    let residual_len = usize::from(node.width) * usize::from(node.height);
    inter_residuals.clear();
    inter_residuals.resize(residual_len, 0);
    let zero_residual = VvcScoredSelectedLumaResidual {
        residual: VvcSelectedLumaResidual {
            block: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: true,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            mts_index: 0,
        },
        score: VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 0,
        },
    };
    Some(finalize_vvc_luma_tu(
        policy.select_luma_tu_coding_decision(node, VvcIntraPredictionMode::Dc),
        source_frame,
        frame_recon,
        node,
        inter_prediction,
        inter_residuals,
        luma_qp,
        luma_ts_quant,
        vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, luma_qp),
        Some(zero_residual),
        stats,
        transform_scratch,
        reconstructed_residual,
    ))
}

fn vvc_luma_prediction_matches_source(
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
    predicted_luma: &[VvcSample],
) -> bool {
    let node_width = usize::from(node.width);
    let node_height = usize::from(node.height);
    if predicted_luma.len() < node_width.saturating_mul(node_height) {
        return false;
    }
    let start_x = usize::from(node.x);
    let start_y = usize::from(node.y);
    let visible_width = node_width.min(source_frame.geometry.width.saturating_sub(start_x));
    let visible_height = node_height.min(source_frame.geometry.height.saturating_sub(start_y));
    if visible_width == 0 || visible_height == 0 {
        return false;
    }
    for row in 0..visible_height {
        let source_start = (start_y + row) * source_frame.geometry.width + start_x;
        let predicted_start = row * node_width;
        if source_frame.luma[source_start..source_start + visible_width]
            != predicted_luma[predicted_start..predicted_start + visible_width]
        {
            return false;
        }
    }
    true
}

fn vvc_zero_chroma_preselected_residual() -> VvcScoredSelectedChromaResidual {
    VvcScoredSelectedChromaResidual {
        residual: VvcSelectedChromaResidual {
            cb: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: true,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            cr: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: true,
                bdpcm_mode: VvcBdpcmMode::None,
            },
        },
        score: VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 0,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn select_vvc_luma_explicit_inter_candidate(
    decision: VvcLumaInterDecision,
    intra_mode: VvcIntraPredictionMode,
    intra_coding_decision: VvcLumaTuCodingDecision,
    intra_residual: Option<VvcScoredSelectedLumaResidual>,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    inter_reference: &VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    inter_prediction: &mut Vec<VvcSample>,
    inter_residuals: &mut Vec<i16>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcScoredSelectedLumaResidual> {
    let intra_residual = intra_residual?;
    if !VvcReconstructionFrame::predict_luma_node_from_inter_motion_into(
        inter_reference,
        inter_prediction,
        node,
        decision,
    ) {
        return None;
    }
    #[cfg(feature = "vvc-stats")]
    let residual_start = StageStart::now();
    residual_luma_tu_at_into(
        inter_residuals,
        source_frame,
        usize::from(node.x),
        usize::from(node.y),
        usize::from(node.width),
        usize::from(node.height),
        inter_prediction,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));

    let inter_coding_decision =
        policy.select_luma_tu_coding_decision(node, VvcIntraPredictionMode::Dc);
    #[cfg(feature = "vvc-stats")]
    let score_start = StageStart::now();
    let inter_residual = select_vvc_scored_luma_residual_block_with_mts(
        inter_coding_decision.residual_coding,
        inter_coding_decision.mts_index,
        inter_residuals,
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
    let inter_residual = VvcScoredSelectedLumaResidual::from_scored_block(inter_residual);
    let intra_score = vvc_scored_luma_quantized_residual_score(
        intra_residual,
        vvc_luma_regular_prediction_syntax_cost(
            node,
            intra_mode,
            left,
            above,
            intra_coding_decision,
        ),
    );
    let inter_score = vvc_scored_luma_quantized_residual_score(
        inter_residual,
        vvc_luma_explicit_inter_syntax_cost(decision),
    );
    if policy.chroma_sampling() == ChromaSampling::Cs444 && inter_score.distortion != 0 {
        return None;
    }
    inter_score
        .selects_over(intra_score)
        .then_some(inter_residual)
}

fn vvc_luma_explicit_inter_syntax_cost(decision: VvcLumaInterDecision) -> u64 {
    // Conservative local estimate for explicit inter leaf signalling:
    // pred_mode_flag/inter-mode prefix, merge flag, MVP flag, coded flag, and
    // two signed MVD components. Residual coefficient cost is already carried
    // by VvcScoredSelectedLumaResidual.
    4 + vvc_explicit_inter_mvd_syntax_cost(decision.mv_x)
        + vvc_explicit_inter_mvd_syntax_cost(decision.mv_y)
}

fn vvc_explicit_inter_mvd_syntax_cost(value: i16) -> u64 {
    let magnitude = u64::from(value.unsigned_abs());
    if magnitude == 0 {
        return 1;
    }
    2 + vvc_unsigned_magnitude_syntax_cost(magnitude)
}

fn vvc_unsigned_magnitude_syntax_cost(mut value: u64) -> u64 {
    let mut bits = 1;
    while value > 1 {
        value >>= 1;
        bits += 2;
    }
    bits
}

#[allow(clippy::too_many_arguments)]
fn finalize_vvc_luma_tu_with_temporal_mode_hint(
    hint: VvcLumaTemporalModeHint,
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    prediction_scratch: &mut VvcDcPredictionScratch,
    predicted_luma: &mut Vec<VvcSample>,
    luma_residuals: &mut Vec<i16>,
    intra_search_stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcFinalizedLumaTu> {
    let coding_decision = if hint.bdpcm_mode.is_enabled() {
        VvcLumaTuCodingDecision {
            residual_coding: VvcTuResidualCodingMode::TransformSkip,
            mrl_index: 0,
            mts_index: 0,
        }
    } else {
        policy.select_luma_tu_coding_decision(node, hint.mode)
    };
    let preselected_residual = if hint.bdpcm_mode.is_enabled() {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        predict_vvc_luma_bdpcm_block_into_with_availability(
            predicted_luma,
            prediction_scratch,
            hint.bdpcm_mode,
            &frame_recon.luma,
            frame_recon.coded_geometry(),
            node,
            source_frame.format.bit_depth,
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_prediction_nanos(
            VvcLumaPredictionStatsFamily::Bdpcm,
            vvc_elapsed_nanos(prediction_start),
        );
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_luma_tu_at_into(
            luma_residuals,
            source_frame,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
            predicted_luma,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        if !vvc_temporal_mode_hint_residual_is_cheap(
            luma_residuals,
            usize::from(node.width) * usize::from(node.height),
            source_frame.format.bit_depth,
            policy,
        ) {
            return None;
        }
        Some(VvcScoredSelectedLumaResidual {
            residual: VvcSelectedLumaResidual {
                block: finalize_vvc_luma_bdpcm_transform_skip_residual_block(
                    luma_residuals,
                    node.width,
                    node.height,
                    luma_ts_quant,
                    hint.bdpcm_mode,
                ),
                mts_index: 0,
            },
            score: VvcResidualBlockScore {
                distortion: 0,
                rate_cost: 0,
            },
        })
    } else {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        if coding_decision.mrl_index == 0 {
            predict_vvc_luma_intra_block_into_with_availability(
                predicted_luma,
                prediction_scratch,
                hint.mode,
                &frame_recon.luma,
                frame_recon.coded_geometry(),
                node,
                source_frame.format.bit_depth,
                Some(frame_recon.luma_availability()),
            );
        } else {
            predict_vvc_luma_intra_block_into_with_mrl_and_availability(
                predicted_luma,
                prediction_scratch,
                hint.mode,
                &frame_recon.luma,
                frame_recon.coded_geometry(),
                node,
                source_frame.format.bit_depth,
                coding_decision.mrl_index,
                Some(frame_recon.luma_availability()),
            );
        }
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_prediction_nanos(
            vvc_luma_prediction_stats_family(hint.mode),
            vvc_elapsed_nanos(prediction_start),
        );
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_luma_tu_at_into(
            luma_residuals,
            source_frame,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
            predicted_luma,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        if !vvc_temporal_mode_hint_residual_is_cheap(
            luma_residuals,
            usize::from(node.width) * usize::from(node.height),
            source_frame.format.bit_depth,
            policy,
        ) {
            return None;
        }
        None
    };
    Some(finalize_vvc_luma_tu(
        coding_decision,
        source_frame,
        frame_recon,
        node,
        predicted_luma,
        luma_residuals,
        luma_qp,
        luma_ts_quant,
        vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, luma_qp),
        preselected_residual,
        intra_search_stats,
        transform_scratch,
        reconstructed_residual,
    ))
}

#[allow(clippy::too_many_arguments)]
fn finalize_vvc_chroma_tu_with_temporal_mode_hint(
    hint: VvcChromaTemporalModeHint,
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    co_located_luma_mode: VvcIntraPredictionMode,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    prediction_scratch: &mut VvcDcPredictionScratch,
    predicted_cb: &mut Vec<VvcSample>,
    predicted_cr: &mut Vec<VvcSample>,
    cb_residuals: &mut Vec<i16>,
    cr_residuals: &mut Vec<i16>,
    intra_search_stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcFinalizedChromaTu> {
    let preselected_residual = if hint.bdpcm_mode.is_enabled() {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        predict_vvc_chroma_bdpcm_block_into_with_availability(
            predicted_cb,
            prediction_scratch,
            hint.bdpcm_mode,
            &frame_recon.cb,
            frame_recon.coded_geometry(),
            node,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            Some(frame_recon.cb_availability()),
        );
        predict_vvc_chroma_bdpcm_block_into_with_availability(
            predicted_cr,
            prediction_scratch,
            hint.bdpcm_mode,
            &frame_recon.cr,
            frame_recon.coded_geometry(),
            node,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            Some(frame_recon.cr_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_prediction_nanos(
            VvcChromaPredictionStatsFamily::Explicit,
            vvc_elapsed_nanos(prediction_start),
        );
        let chroma_x = usize::from(node.x) / chroma_subsample_x(source_frame.format.chroma_sampling);
        let chroma_y = usize::from(node.y) / chroma_subsample_y(source_frame.format.chroma_sampling);
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_tu_at_into(
            cb_residuals,
            &source_frame.cb,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cb,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_tu_at_into(
            cr_residuals,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cr,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        if !vvc_temporal_mode_hint_residual_is_cheap(
            cb_residuals,
            chroma_width * chroma_height,
            source_frame.format.bit_depth,
            policy,
        ) || !vvc_temporal_mode_hint_residual_is_cheap(
            cr_residuals,
            chroma_width * chroma_height,
            source_frame.format.bit_depth,
            policy,
        ) {
            return None;
        }
        Some(VvcScoredSelectedChromaResidual {
            residual: VvcSelectedChromaResidual {
                cb: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                    cb_residuals,
                    chroma_width,
                    chroma_height,
                    chroma_ts_quant,
                    hint.bdpcm_mode,
                ),
                cr: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                    cr_residuals,
                    chroma_width,
                    chroma_height,
                    chroma_ts_quant,
                    hint.bdpcm_mode,
                ),
            },
            score: VvcResidualBlockScore {
                distortion: 0,
                rate_cost: 0,
            },
        })
    } else {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
            predicted_cb,
            predicted_cr,
            prediction_scratch,
            hint.mode,
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
        intra_search_stats.add_chroma_prediction_nanos(
            vvc_chroma_prediction_stats_family(hint.mode),
            vvc_elapsed_nanos(prediction_start),
        );
        let chroma_x = usize::from(node.x) / chroma_subsample_x(source_frame.format.chroma_sampling);
        let chroma_y = usize::from(node.y) / chroma_subsample_y(source_frame.format.chroma_sampling);
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_tu_at_into(
            cb_residuals,
            &source_frame.cb,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cb,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_tu_at_into(
            cr_residuals,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cr,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        if !vvc_temporal_mode_hint_residual_is_cheap(
            cb_residuals,
            chroma_width * chroma_height,
            source_frame.format.bit_depth,
            policy,
        ) || !vvc_temporal_mode_hint_residual_is_cheap(
            cr_residuals,
            chroma_width * chroma_height,
            source_frame.format.bit_depth,
            policy,
        ) {
            return None;
        }
        None
    };
    Some(finalize_vvc_chroma_tu(
        policy.select_chroma_tu_coding_decision(node, hint.mode),
        source_frame,
        frame_recon,
        node,
        predicted_cb,
        predicted_cr,
        cb_residuals,
        cr_residuals,
        chroma_width,
        chroma_height,
        chroma_qp,
        chroma_ts_quant,
        vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, chroma_qp),
        preselected_residual,
        intra_search_stats,
        transform_scratch,
        reconstructed_residual,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch_with_mode_hints(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    luma_qp: i32,
    chroma_qp: i32,
    luma_mode_search_state: &mut VvcLumaModeSearchState,
    transform_skip_quant_tables: &VvcTransformSkipQuantTables,
    scratch: &mut VvcCtuQuantScratch,
    luma_inter_skip: Option<&[bool; MAX_VVC_LUMA_TUS]>,
    chroma_inter_skip: Option<&[bool; MAX_VVC_CHROMA_TUS]>,
    luma_inter_decisions: Option<&[Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS]>,
    inter_reference: Option<&VvcReconstructionFrame>,
    selected_luma_inter_decisions: Option<&mut [Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS]>,
    temporal_mode_hints: Option<&VvcQuantizedColor>,
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
    let mut applied_luma_inter_decisions = [None; MAX_VVC_LUMA_TUS];
    let mut luma_nodes = std::mem::take(&mut scratch.luma_nodes);
    let mut chroma_nodes = std::mem::take(&mut scratch.chroma_nodes);
    let mut prediction_scratch = std::mem::take(&mut scratch.prediction_scratch);
    let mut predicted_luma = std::mem::take(&mut scratch.predicted_luma);
    let mut predicted_cb = std::mem::take(&mut scratch.predicted_cb);
    let mut predicted_cr = std::mem::take(&mut scratch.predicted_cr);
    let mut transform_scratch = std::mem::take(&mut scratch.transform_scratch);
    let mut reconstructed_residual = std::mem::take(&mut scratch.reconstructed_residual);
    let mut luma_residuals = std::mem::take(&mut scratch.luma_residuals);
    let mut candidate_luma_prediction = std::mem::take(&mut scratch.candidate_luma_prediction);
    let mut candidate_luma_residuals = std::mem::take(&mut scratch.candidate_luma_residuals);
    let mut luma_rd_cache =
        std::mem::replace(&mut scratch.luma_rd_cache, VvcLumaModeRdCache::new());
    let mut cb_residuals = std::mem::take(&mut scratch.cb_residuals);
    let mut cr_residuals = std::mem::take(&mut scratch.cr_residuals);
    let mut candidate_cb_prediction = std::mem::take(&mut scratch.candidate_cb_prediction);
    let mut candidate_cr_prediction = std::mem::take(&mut scratch.candidate_cr_prediction);
    let mut candidate_cb_residuals = std::mem::take(&mut scratch.candidate_cb_residuals);
    let mut candidate_cr_residuals = std::mem::take(&mut scratch.candidate_cr_residuals);
    let mut chroma_rd_cache =
        std::mem::replace(&mut scratch.chroma_rd_cache, VvcChromaModeRdCache::new());
    predicted_luma.clear();
    predicted_cb.clear();
    predicted_cr.clear();
    reconstructed_residual.clear();
    luma_residuals.clear();
    candidate_luma_prediction.clear();
    candidate_luma_residuals.clear();
    cb_residuals.clear();
    cr_residuals.clear();
    candidate_cb_prediction.clear();
    candidate_cr_prediction.clear();
    candidate_cb_residuals.clear();
    candidate_cr_residuals.clear();
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
        dual_tree_intra: policy.dual_tree_intra(),
    };

    let mut luma_tu_count = 0usize;
    vvc_luma_transform_nodes_into_for_kind(
        &mut luma_nodes,
        ctu_shape,
        luma_max_leaf_size,
        policy.luma_split_kind(),
    );
    for local_node in luma_nodes.iter().copied() {
        if luma_tu_count >= MAX_VVC_LUMA_TUS {
            break;
        }
        let node = vvc_global_ctu_node(local_node, region);
        if luma_inter_skip
            .and_then(|mask| mask.get(luma_tu_count))
            .copied()
            .unwrap_or(false)
        {
            if let Some(hint) = vvc_luma_temporal_mode_hint(
                temporal_mode_hints,
                luma_tu_count,
                luma_nodes.len(),
                policy,
                node,
            ) {
                luma_tu_intra_modes[luma_tu_count] = hint.mode;
                luma_tu_bdpcm_modes[luma_tu_count] = hint.bdpcm_mode;
                luma_mode_search_state.mark_node(node, hint.mode);
            }
            copy_source_luma_node_into_reconstruction(frame_recon, source_frame, node);
            frame_recon.mark_luma_node_available(node);
            luma_tu_count += 1;
            continue;
        }
        luma_rd_cache.reset(policy, node);
        let left_luma_mode = luma_mode_search_state.left_of(node);
        let above_luma_mode = luma_mode_search_state.above_of(node);
        if let Some(hint) = vvc_luma_temporal_mode_hint(
            temporal_mode_hints,
            luma_tu_count,
            luma_nodes.len(),
            policy,
            node,
        ) {
            let luma_mode = hint.mode;
            #[cfg(feature = "vvc-stats")]
            let luma_finalize_start = StageStart::now();
            if let Some(luma_tu) = finalize_vvc_luma_tu_with_temporal_mode_hint(
                hint,
                policy,
                source_frame,
                frame_recon,
                node,
                luma_qp,
                luma_ts_quant,
                &mut prediction_scratch,
                &mut predicted_luma,
                &mut luma_residuals,
                &mut intra_search_stats,
                &mut transform_scratch,
                &mut reconstructed_residual,
            ) {
                #[cfg(feature = "vvc-stats")]
                intra_search_stats
                    .add_luma_finalize_nanos(luma_finalize_start.elapsed().as_nanos() as u64);
                luma_tu_intra_modes[luma_tu_count] = luma_mode;
                luma_mode_search_state.mark_node(node, luma_mode);
                #[cfg(feature = "vvc-stats")]
                residual_energy_stats.add_luma_residuals(
                    &luma_residuals,
                    usize::from(node.width),
                    usize::from(node.height),
                );
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
                continue;
            }
        }
        if let Some(decision) = luma_inter_decisions
            .and_then(|decisions| decisions.get(luma_tu_count))
            .copied()
            .flatten()
        {
            if let Some(reference) = inter_reference {
                #[cfg(feature = "vvc-stats")]
                let luma_finalize_start = StageStart::now();
                if let Some(luma_tu) = finalize_vvc_luma_exact_explicit_inter_candidate(
                    decision,
                    policy,
                    source_frame,
                    reference,
                    frame_recon,
                    node,
                    luma_qp,
                    luma_ts_quant,
                    &mut candidate_luma_prediction,
                    &mut candidate_luma_residuals,
                    &mut intra_search_stats,
                    &mut transform_scratch,
                    &mut reconstructed_residual,
                ) {
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats
                        .add_luma_finalize_nanos(luma_finalize_start.elapsed().as_nanos() as u64);
                    let luma_mode = VvcIntraPredictionMode::Dc;
                    luma_tu_intra_modes[luma_tu_count] = luma_mode;
                    #[cfg(feature = "vvc-stats")]
                    residual_energy_stats.add_luma_residuals(
                        &candidate_luma_residuals,
                        usize::from(node.width),
                        usize::from(node.height),
                    );
                    luma_tu_remainders[luma_tu_count] = luma_tu.abs_remainder;
                    luma_tu_negative[luma_tu_count] = luma_tu.negative;
                    luma_tu_dc_levels[luma_tu_count] = luma_tu.dc_level;
                    luma_tu_ac_levels[luma_tu_count] = luma_tu.ac_levels;
                    luma_tu_has_ac[luma_tu_count] = luma_tu.has_ac;
                    luma_tu_transform_skip[luma_tu_count] = luma_tu.transform_skip;
                    luma_tu_bdpcm_modes[luma_tu_count] = luma_tu.bdpcm_mode;
                    luma_tu_mrl_index[luma_tu_count] = luma_tu.mrl_index;
                    luma_tu_mts_index[luma_tu_count] = luma_tu.mts_index;
                    applied_luma_inter_decisions[luma_tu_count] = Some(decision);
                    #[cfg(feature = "vvc-stats")]
                    write_vvc_luma_tu_trace(
                        tu_trace_sink.as_mut(),
                        region,
                        luma_tu_count,
                        node,
                        luma_mode,
                        luma_tu,
                        &candidate_luma_prediction,
                        &candidate_luma_residuals,
                    );
                    luma_tu_count += 1;
                    continue;
                }
            }
        }
        #[cfg(feature = "vvc-stats")]
        let luma_mode_search_start = StageStart::now();
        let mut best_luma_mode = VvcIntraPredictionMode::Dc;
        let mut best_luma_score = u64::MAX;
        let mut luma_candidate_costs = VvcLumaIntraCandidateCosts::new(u64::MAX);
        if !vvc_luma_lossless_speed_skips_dc(policy) {
            #[cfg(feature = "vvc-stats")]
            let prediction_start = StageStart::now();
            predict_vvc_luma_intra_block_into_with_availability(
                &mut predicted_luma,
                &mut prediction_scratch,
                VvcIntraPredictionMode::Dc,
                &frame_recon.luma,
                frame_recon.coded_geometry(),
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
            let score_start = StageStart::now();
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
            best_luma_score = dc_score;
            luma_candidate_costs = VvcLumaIntraCandidateCosts::new(dc_score);
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_dc();
        }
        if policy.luma_planar_candidate_allowed(node)
            && vvc_luma_lossless_speed_evaluates_planar(policy, left_luma_mode, above_luma_mode)
        {
            let candidate_score = score_vvc_luma_planar_candidate(
                &mut luma_rd_cache,
                score_metric,
                source_frame,
                frame_recon,
                node,
                left_luma_mode,
                above_luma_mode,
                &mut prediction_scratch,
                &mut candidate_luma_prediction,
                &mut candidate_luma_residuals,
                &mut intra_search_stats,
            );
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
                let prediction_start = StageStart::now();
                predict_vvc_luma_intra_block_into_with_availability(
                    &mut candidate_luma_prediction,
                    &mut prediction_scratch,
                    mode,
                    &frame_recon.luma,
                    frame_recon.coded_geometry(),
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
                let score_start = StageStart::now();
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
                && !vvc_luma_lossless_speed_skips_directional_refinement(policy)
            {
                let refinement_start = luma_directional_candidates.count();
                let refinement_fast_search =
                    if policy.residual_mode() == VvcResidualCodingMode::Lossy
                        && policy.fast_search() == VvcFastSearch::LosslessSpeed
                    {
                        VvcFastSearch::Off
                    } else {
                        policy.fast_search()
                    };
                luma_directional_candidates
                    .add_refinement(best_luma_mode.luma_mode_index(), refinement_fast_search);
                for mode in luma_directional_candidates.iter_from(refinement_start) {
                    #[cfg(feature = "vvc-stats")]
                    let prediction_start = StageStart::now();
                    predict_vvc_luma_intra_block_into_with_availability(
                        &mut candidate_luma_prediction,
                        &mut prediction_scratch,
                        mode,
                        &frame_recon.luma,
                        frame_recon.coded_geometry(),
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
                    let score_start = StageStart::now();
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
            let residual_start = StageStart::now();
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
        let luma_rd_start = StageStart::now();
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
        let luma_mrl_start = StageStart::now();
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
        let luma_bdpcm_start = StageStart::now();
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
        let mut selected_luma_inter_decision = None;
        if let Some(decision) = luma_inter_decisions
            .and_then(|decisions| decisions.get(luma_tu_count))
            .copied()
            .flatten()
        {
            if let Some(reference) = inter_reference {
                if let Some(inter_residual) = select_vvc_luma_explicit_inter_candidate(
                    decision,
                    luma_mode,
                    luma_coding_decision,
                    selected_luma_residual,
                    left_luma_mode,
                    above_luma_mode,
                    policy,
                    source_frame,
                    reference,
                    node,
                    luma_qp,
                    luma_ts_quant,
                    &mut candidate_luma_prediction,
                    &mut candidate_luma_residuals,
                    &mut intra_search_stats,
                    &mut transform_scratch,
                    &mut reconstructed_residual,
                ) {
                    luma_mode = VvcIntraPredictionMode::Dc;
                    luma_coding_decision =
                        policy.select_luma_tu_coding_decision(node, luma_mode);
                    selected_luma_residual = Some(inter_residual);
                    selected_luma_inter_decision = Some(decision);
                    std::mem::swap(&mut predicted_luma, &mut candidate_luma_prediction);
                    std::mem::swap(&mut luma_residuals, &mut candidate_luma_residuals);
                }
            }
        }
        luma_tu_intra_modes[luma_tu_count] = luma_mode;
        if selected_luma_inter_decision.is_none() {
            luma_mode_search_state.mark_node(node, luma_mode);
        }
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats.add_luma_residuals(
            &luma_residuals,
            usize::from(node.width),
            usize::from(node.height),
        );
        #[cfg(feature = "vvc-stats")]
        let luma_finalize_start = StageStart::now();
        let luma_tu = finalize_vvc_luma_tu(
            luma_coding_decision,
            source_frame,
            frame_recon,
            node,
            &predicted_luma,
            &luma_residuals,
            luma_qp,
            luma_ts_quant,
            vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, luma_qp),
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
        applied_luma_inter_decisions[luma_tu_count] = selected_luma_inter_decision;
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
    if ctu_shape.dual_tree_intra {
        vvc_chroma_transform_nodes_into(&mut chroma_nodes, ctu_shape);
    } else {
        chroma_nodes.clear();
        chroma_nodes.extend(luma_nodes.iter().copied());
    }
    for local_node in chroma_nodes.iter().copied() {
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
        if let Some(decision) = applied_luma_inter_decisions
            .get(chroma_tu_count)
            .copied()
            .flatten()
        {
            if let Some(reference) = inter_reference {
                if VvcReconstructionFrame::predict_chroma_node_from_inter_motion_into(
                    reference,
                    &mut predicted_cb,
                    &mut predicted_cr,
                    node,
                    decision,
                ) {
                    #[cfg(feature = "vvc-stats")]
                    let residual_start = StageStart::now();
                    let cb_residuals_all_zero = residual_chroma_tu_at_into_and_detect_zero(
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
                    intra_search_stats
                        .add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
                    #[cfg(feature = "vvc-stats")]
                    let residual_start = StageStart::now();
                    let cr_residuals_all_zero = residual_chroma_tu_at_into_and_detect_zero(
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
                    intra_search_stats
                        .add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
                    let preselected_residual = if cb_residuals_all_zero && cr_residuals_all_zero {
                        Some(vvc_zero_chroma_preselected_residual())
                    } else {
                        None
                    };
                    let chroma_coding_decision = policy.select_chroma_tu_coding_decision(
                        node,
                        VvcChromaIntraPredictionMode::Derived,
                    );
                    #[cfg(feature = "vvc-stats")]
                    residual_energy_stats.add_chroma_residuals(
                        &cb_residuals,
                        chroma_width,
                        chroma_height,
                    );
                    #[cfg(feature = "vvc-stats")]
                    residual_energy_stats.add_chroma_residuals(
                        &cr_residuals,
                        chroma_width,
                        chroma_height,
                    );
                    #[cfg(feature = "vvc-stats")]
                    let chroma_finalize_start = StageStart::now();
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
                        vvc_transform_skip_qp_reconstructs_exact(
                            source_frame.format.bit_depth,
                            chroma_qp,
                        ),
                        preselected_residual,
                        &mut intra_search_stats,
                        &mut transform_scratch,
                        &mut reconstructed_residual,
                    );
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats.add_chroma_finalize_nanos(
                        chroma_finalize_start.elapsed().as_nanos() as u64,
                    );
                    cb_tu_dc_levels[chroma_tu_count] = chroma_tu.cb_dc_level;
                    cr_tu_dc_levels[chroma_tu_count] = chroma_tu.cr_dc_level;
                    cb_tu_ac_levels[chroma_tu_count] = chroma_tu.cb_ac_levels;
                    cr_tu_ac_levels[chroma_tu_count] = chroma_tu.cr_ac_levels;
                    cb_tu_has_ac[chroma_tu_count] = chroma_tu.cb_has_ac;
                    cr_tu_has_ac[chroma_tu_count] = chroma_tu.cr_has_ac;
                    cb_tu_transform_skip[chroma_tu_count] = chroma_tu.cb_transform_skip;
                    cr_tu_transform_skip[chroma_tu_count] = chroma_tu.cr_transform_skip;
                    chroma_tu_bdpcm_modes[chroma_tu_count] = chroma_tu.bdpcm_mode;
                    chroma_tu_count += 1;
                    continue;
                }
            }
        }
        if chroma_inter_skip
            .and_then(|mask| mask.get(chroma_tu_count))
            .copied()
            .unwrap_or(false)
        {
            if let Some(hint) = vvc_chroma_temporal_mode_hint(
                temporal_mode_hints,
                chroma_tu_count,
                chroma_nodes.len(),
                policy,
                source_frame.geometry,
                node,
                co_located_luma_mode,
                chroma_width,
                chroma_height,
            ) {
                chroma_tu_intra_modes[chroma_tu_count] = hint.mode;
                chroma_tu_bdpcm_modes[chroma_tu_count] = hint.bdpcm_mode;
            }
            let coded_geometry = frame_recon.coded_geometry();
            copy_source_chroma_node_into_reconstruction(
                &mut frame_recon.cb,
                &source_frame.cb,
                source_frame.geometry,
                coded_geometry,
                source_frame.format,
                node,
            );
            copy_source_chroma_node_into_reconstruction(
                &mut frame_recon.cr,
                &source_frame.cr,
                source_frame.geometry,
                coded_geometry,
                source_frame.format,
                node,
            );
            frame_recon.mark_chroma_node_available(node);
            chroma_tu_count += 1;
            continue;
        }
        if let Some(hint) = vvc_chroma_temporal_mode_hint(
            temporal_mode_hints,
            chroma_tu_count,
            chroma_nodes.len(),
            policy,
            source_frame.geometry,
            node,
            co_located_luma_mode,
            chroma_width,
            chroma_height,
        ) {
            #[cfg(feature = "vvc-stats")]
            let chroma_finalize_start = StageStart::now();
            if let Some(chroma_tu) = finalize_vvc_chroma_tu_with_temporal_mode_hint(
                hint,
                policy,
                source_frame,
                frame_recon,
                node,
                co_located_luma_mode,
                chroma_width,
                chroma_height,
                chroma_qp,
                chroma_ts_quant,
                &mut prediction_scratch,
                &mut predicted_cb,
                &mut predicted_cr,
                &mut cb_residuals,
                &mut cr_residuals,
                &mut intra_search_stats,
                &mut transform_scratch,
                &mut reconstructed_residual,
            ) {
                #[cfg(feature = "vvc-stats")]
                intra_search_stats
                    .add_chroma_finalize_nanos(chroma_finalize_start.elapsed().as_nanos() as u64);
                chroma_tu_intra_modes[chroma_tu_count] = hint.mode;
                #[cfg(feature = "vvc-stats")]
                {
                    residual_energy_stats.add_chroma_residuals(
                        &cb_residuals,
                        chroma_width,
                        chroma_height,
                    );
                    residual_energy_stats.add_chroma_residuals(
                        &cr_residuals,
                        chroma_width,
                        chroma_height,
                    );
                }
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
                    hint.mode,
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
                continue;
            }
        }
        let initial_chroma_mode = VvcChromaIntraPredictionMode::Derived;
        #[cfg(feature = "vvc-stats")]
        let chroma_mode_search_start = StageStart::now();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
            &mut predicted_cb,
            &mut predicted_cr,
            &mut prediction_scratch,
            initial_chroma_mode,
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
        intra_search_stats.add_chroma_prediction_nanos(
            VvcChromaPredictionStatsFamily::Derived,
            vvc_elapsed_nanos(prediction_start),
        );
        let (raw_chroma_mode, chroma_candidate_costs) =
            if vvc_chroma_fast_search_uses_derived_only(policy) {
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_derived();
                (
                    initial_chroma_mode,
                    VvcChromaIntraCandidateCosts::new(0),
                )
            } else {
                #[cfg(feature = "vvc-stats")]
                let score_start = StageStart::now();
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
                let mut chroma_candidate_costs =
                    VvcChromaIntraCandidateCosts::new(initial_score);
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_derived();
                if !vvc_chroma_lossless_speed_skips_near_exact_explicit_search(
                    policy,
                    best_chroma_score,
                    chroma_width,
                    chroma_height,
                ) && !vvc_chroma_lossy_exact_mode_search_done(
                    chroma_syntax_tie_breaker,
                    best_chroma_score,
                ) {
                    for explicit_mode in vvc_chroma_explicit_candidates(co_located_luma_mode) {
                        if !vvc_chroma_explicit_candidate_allowed_for_search(policy, explicit_mode)
                        {
                            continue;
                        }
                        let chroma_mode = VvcChromaIntraPredictionMode::Explicit(explicit_mode);
                        #[cfg(feature = "vvc-stats")]
                        let prediction_start = StageStart::now();
                        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                            &mut candidate_cb_prediction,
                            &mut candidate_cr_prediction,
                            &mut prediction_scratch,
                            chroma_mode,
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
                        intra_search_stats.add_chroma_prediction_nanos(
                            VvcChromaPredictionStatsFamily::Explicit,
                            vvc_elapsed_nanos(prediction_start),
                        );
                        #[cfg(feature = "vvc-stats")]
                        let score_start = StageStart::now();
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
                        intra_search_stats
                            .add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
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
                    && vvc_chroma_cclm_fast_search_allowed(
                        policy,
                        best_chroma_score,
                        chroma_width,
                        chroma_height,
                    )
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
                        let prediction_start = StageStart::now();
                        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                            &mut candidate_cb_prediction,
                            &mut candidate_cr_prediction,
                            &mut prediction_scratch,
                            chroma_mode,
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
                        intra_search_stats.add_chroma_prediction_nanos(
                            VvcChromaPredictionStatsFamily::Cclm,
                            vvc_elapsed_nanos(prediction_start),
                        );
                        #[cfg(feature = "vvc-stats")]
                        let score_start = StageStart::now();
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
                        intra_search_stats
                            .add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                        #[cfg(feature = "vvc-stats")]
                        intra_search_stats.add_chroma_cclm_mode(cclm_mode);
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
                (raw_chroma_mode, chroma_candidate_costs)
            };
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
            let residual_start = StageStart::now();
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
            let residual_start = StageStart::now();
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
        let chroma_rd_start = StageStart::now();
        let selected_chroma_mode = if vvc_chroma_lossy_speed_direct_bdpcm_candidates_allowed(
            policy,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            raw_chroma_mode,
        )
        {
            VvcSelectedChromaMode {
                mode: raw_chroma_mode,
                residual: None,
            }
        } else {
            select_vvc_chroma_mode_with_rd_refinement(
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
            )
        };
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
        let chroma_bdpcm_start = StageStart::now();
        if let Some(selected_bdpcm) = select_vvc_chroma_bdpcm_prediction(
            policy,
            node,
            chroma_mode,
            co_located_luma_mode,
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
        let chroma_finalize_start = StageStart::now();
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
            vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, chroma_qp),
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
    let quantized = VvcQuantizedColor {
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
        luma_tu_scc_decisions: [VvcLumaSccDecision::RegularIntra; MAX_VVC_LUMA_TUS],
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
    };
    scratch.prediction_scratch = prediction_scratch;
    scratch.predicted_luma = predicted_luma;
    scratch.predicted_cb = predicted_cb;
    scratch.predicted_cr = predicted_cr;
    scratch.transform_scratch = transform_scratch;
    scratch.reconstructed_residual = reconstructed_residual;
    scratch.luma_residuals = luma_residuals;
    scratch.candidate_luma_prediction = candidate_luma_prediction;
    scratch.candidate_luma_residuals = candidate_luma_residuals;
    scratch.luma_rd_cache = luma_rd_cache;
    scratch.cb_residuals = cb_residuals;
    scratch.cr_residuals = cr_residuals;
    scratch.candidate_cb_prediction = candidate_cb_prediction;
    scratch.candidate_cr_prediction = candidate_cr_prediction;
    scratch.candidate_cb_residuals = candidate_cb_residuals;
    scratch.candidate_cr_residuals = candidate_cr_residuals;
    scratch.chroma_rd_cache = chroma_rd_cache;
    scratch.luma_nodes = luma_nodes;
    scratch.chroma_nodes = chroma_nodes;
    if let Some(selected) = selected_luma_inter_decisions {
        *selected = applied_luma_inter_decisions;
    }
    quantized
}

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
use crate::vvc::cabac::vvc_luma_transform_nodes_into_for_kind;
