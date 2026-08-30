#[cfg(any(test, feature = "bench-internals"))]
pub fn quantize_vvc_color(color: VvcSampledColor) -> VvcQuantizedColor {
    quantize_vvc_frame(&VvcSampledFrame::solid(color))
}

#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
pub(in crate::vvc) fn quantize_vvc_frame(frame: &VvcSampledFrame) -> VvcQuantizedColor {
    quantize_vvc_frame_with_reconstruction(frame).quantized
}

#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
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

#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
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

#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
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

#[cfg(any(feature = "bench-internals", feature = "vvc-stats"))]
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

#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
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
