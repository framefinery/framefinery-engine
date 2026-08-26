#[derive(Debug, Clone, Copy)]
struct VvcFinalizedResidualBlock<const AC_COEFFS: usize> {
    dc_level: i16,
    ac_levels: [i16; AC_COEFFS],
    has_ac: bool,
    transform_skip: bool,
    bdpcm_mode: VvcBdpcmMode,
}

impl<const AC_COEFFS: usize> VvcFinalizedResidualBlock<AC_COEFFS> {
    fn abs_remainder(self) -> u8 {
        self.dc_level.unsigned_abs().min(u8::MAX as u16) as u8
    }

    fn negative(self) -> bool {
        self.dc_level < 0 && self.abs_remainder() != 0
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedLumaResidual {
    block: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
}

#[derive(Debug, Clone, Copy)]
struct VvcScoredSelectedLumaResidual {
    residual: VvcSelectedLumaResidual,
    score: VvcResidualBlockScore,
}

impl VvcScoredSelectedLumaResidual {
    fn new(
        source_residuals: &[i16],
        node: VvcCodingTreeNode,
        bit_depth: SampleBitDepth,
        luma_qp: i32,
        luma_ts_quant: &VvcTransformSkipQuantTable,
        residual: VvcSelectedLumaResidual,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let score = vvc_luma_residual_block_score(
            source_residuals,
            node.width,
            node.height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            residual.block,
            residual.mts_index,
            transform_scratch,
            reconstructed_residual,
        );
        Self { residual, score }
    }

    fn from_scored_block(scored: VvcScoredLumaResidualBlock) -> Self {
        Self {
            residual: VvcSelectedLumaResidual {
                block: scored.block,
                mts_index: scored.mts_index,
            },
            score: scored.score,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcFinalizedLumaTu {
    abs_remainder: u8,
    negative: bool,
    dc_level: i16,
    ac_levels: [i16; VVC_LUMA_AC_COEFFS_PER_TU],
    has_ac: bool,
    transform_skip: bool,
    bdpcm_mode: VvcBdpcmMode,
    mrl_index: u8,
    mts_index: u8,
}

fn finalize_vvc_luma_tu(
    coding_decision: VvcLumaTuCodingDecision,
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    predicted_luma: &[VvcSample],
    residuals: &[i16],
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    exact_transform_skip_qp: bool,
    preselected_residual: Option<VvcScoredSelectedLumaResidual>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedLumaTu {
    #[cfg(feature = "vvc-stats")]
    let score_start = StageStart::now();
    let selected_residual = match preselected_residual {
        Some(residual) => refine_vvc_luma_final_mts_residual(
            residual.residual,
            coding_decision,
            residuals,
            node,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            stats,
            transform_scratch,
            reconstructed_residual,
        ),
        None => {
            let (block, mts_index) = select_vvc_luma_residual_block_with_mts(
                coding_decision.residual_coding,
                coding_decision.mts_index,
                residuals,
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
            VvcSelectedLumaResidual { block, mts_index }
        }
    };
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    let residual = selected_residual.block;
    let mts_index = selected_residual.mts_index;
    let coded_geometry = frame_recon.coded_geometry();
    if exact_transform_skip_qp
        && vvc_luma_transform_skip_score_is_exact(
            residual,
            node.width,
            node.height,
            source_frame.format.bit_depth,
            luma_qp,
        )
    {
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        copy_source_luma_node_into_reconstruction(frame_recon, source_frame, node);
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_fill_nanos(vvc_elapsed_nanos(fill_start));
    } else if residual.transform_skip {
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        fill_visible_luma_transform_skip_node(
            &mut frame_recon.luma,
            coded_geometry,
            node,
            predicted_luma,
            residual,
            source_frame.format.bit_depth,
            luma_ts_quant,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_fill_nanos(vvc_elapsed_nanos(fill_start));
    } else {
        #[cfg(feature = "vvc-stats")]
        let recon_start = StageStart::now();
        reconstruct_vvc_luma_residual_block_into(
            residual,
            mts_index,
            reconstructed_residual,
            transform_scratch,
            node.width,
            node.height,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_residual_recon_nanos(vvc_elapsed_nanos(recon_start));
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        fill_visible_luma_node(
            &mut frame_recon.luma,
            coded_geometry,
            node,
            predicted_luma,
            reconstructed_residual,
            source_frame.format.bit_depth,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_fill_nanos(vvc_elapsed_nanos(fill_start));
    }
    let finalized = VvcFinalizedLumaTu {
        abs_remainder: residual.abs_remainder(),
        negative: residual.negative(),
        dc_level: residual.dc_level,
        ac_levels: residual.ac_levels,
        has_ac: residual.has_ac,
        transform_skip: residual.transform_skip,
        bdpcm_mode: residual.bdpcm_mode,
        mrl_index: coding_decision.mrl_index,
        mts_index,
    };
    frame_recon.mark_luma_node_available(node);
    finalized
}

fn fill_visible_luma_transform_skip_node(
    luma: &mut [VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    bit_depth: SampleBitDepth,
    quant_table: &VvcTransformSkipQuantTable,
) {
    let node_width = usize::from(node.width);
    let node_height = usize::from(node.height);
    let start_x = usize::from(node.x);
    let start_y = usize::from(node.y);
    let visible_width = node_width.min(geometry.width.saturating_sub(start_x));
    let visible_height = node_height.min(geometry.height.saturating_sub(start_y));
    if visible_width == 0 || visible_height == 0 {
        return;
    }
    let (active_width, active_height) =
        vvc_luma_transform_skip_active_extent(node_width, node_height);
    if residual.bdpcm_mode.is_enabled() {
        fill_visible_luma_bdpcm_transform_skip_node(
            luma,
            geometry,
            start_x,
            start_y,
            visible_width,
            visible_height,
            node_width,
            active_width,
            active_height,
            predicted,
            residual,
            bit_depth,
            quant_table,
        );
        return;
    }

    let max_sample = i32::from(bit_depth.max_sample());
    for local_y in 0..visible_height {
        let row = (start_y + local_y) * geometry.width + start_x;
        let predicted_row = local_y * node_width;
        for local_x in 0..visible_width {
            let reconstructed_residual =
                if local_y < active_height && local_x < active_width {
                    let level = if local_x == 0 && local_y == 0 {
                        residual.dc_level
                    } else {
                        residual.ac_levels[local_y * active_width + local_x - 1]
                    };
                    quant_table.reconstructed(level)
                } else {
                    0
                };
            let idx = predicted_row + local_x;
            luma[row + local_x] =
                (i32::from(predicted[idx]) + i32::from(reconstructed_residual))
                    .clamp(0, max_sample) as VvcSample;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_visible_luma_bdpcm_transform_skip_node(
    luma: &mut [VvcSample],
    geometry: VvcVideoGeometry,
    start_x: usize,
    start_y: usize,
    visible_width: usize,
    visible_height: usize,
    node_width: usize,
    active_width: usize,
    active_height: usize,
    predicted: &[VvcSample],
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    bit_depth: SampleBitDepth,
    quant_table: &VvcTransformSkipQuantTable,
) {
    let max_sample = i32::from(bit_depth.max_sample());
    let active_rows = visible_height.min(active_height);
    let mut vertical_predictors = [0i16; 8];
    for local_y in 0..active_rows {
        let row = (start_y + local_y) * geometry.width + start_x;
        let predicted_row = local_y * node_width;
        let mut horizontal_predictor = 0i16;
        for local_x in 0..active_width {
            let delta = if local_x == 0 && local_y == 0 {
                residual.dc_level
            } else {
                residual.ac_levels[local_y * active_width + local_x - 1]
            };
            let level = match residual.bdpcm_mode {
                VvcBdpcmMode::None => unreachable!("BDPCM fill requires a direction"),
                VvcBdpcmMode::Horizontal if local_x > 0 => {
                    add_bdpcm_quantized_levels(delta, horizontal_predictor)
                }
                VvcBdpcmMode::Vertical if local_y > 0 => {
                    add_bdpcm_quantized_levels(delta, vertical_predictors[local_x])
                }
                VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => delta,
            };
            horizontal_predictor = level;
            vertical_predictors[local_x] = level;
            if local_x < visible_width {
                let idx = predicted_row + local_x;
                luma[row + local_x] =
                    (i32::from(predicted[idx]) + i32::from(quant_table.reconstructed(level)))
                        .clamp(0, max_sample) as VvcSample;
            }
        }
        for local_x in active_width..visible_width {
            let idx = predicted_row + local_x;
            luma[row + local_x] = predicted[idx];
        }
    }
    for local_y in active_rows..visible_height {
        let row = (start_y + local_y) * geometry.width + start_x;
        let predicted_row = local_y * node_width;
        luma[row..row + visible_width]
            .copy_from_slice(&predicted[predicted_row..predicted_row + visible_width]);
    }
}

fn copy_source_luma_node_into_reconstruction(
    frame_recon: &mut VvcReconstructionFrame,
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
) {
    let start_x = usize::from(node.x);
    let start_y = usize::from(node.y);
    if source_frame.geometry.width == 0 || source_frame.geometry.height == 0 {
        return;
    }
    let dst_width = frame_recon.luma_width();
    let dst_height = frame_recon.luma_height();
    if start_x >= dst_width || start_y >= dst_height {
        return;
    }
    let end_x = start_x
        .saturating_add(usize::from(node.width))
        .min(dst_width);
    let end_y = start_y
        .saturating_add(usize::from(node.height))
        .min(dst_height);
    for y in start_y..end_y {
        let dst_row = y * dst_width;
        let src_y = y.min(source_frame.geometry.height - 1);
        let src_row = src_y * source_frame.geometry.width;
        for x in start_x..end_x {
            let src_x = x.min(source_frame.geometry.width - 1);
            frame_recon.luma[dst_row + x] = source_frame.luma[src_row + src_x];
        }
    }
}

fn refine_vvc_luma_final_mts_residual(
    selected: VvcSelectedLumaResidual,
    coding_decision: VvcLumaTuCodingDecision,
    residuals: &[i16],
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedLumaResidual {
    if selected.block.transform_skip
        || !vvc_luma_mts_selection_allowed(
            coding_decision.residual_coding,
            coding_decision.mts_index,
            node.width,
            node.height,
            luma_qp,
            selected.block.has_ac,
        )
    {
        return selected;
    }
    let (block, mts_index) = select_vvc_luma_residual_block_with_mts(
        coding_decision.residual_coding,
        coding_decision.mts_index,
        residuals,
        node.width,
        node.height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        true,
        VvcLumaResidualQuantizationSearch::Full,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    VvcSelectedLumaResidual { block, mts_index }
}

fn select_vvc_luma_residual_block_with_mts(
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    allow_explicit_mts: bool,
    quantization_search: VvcLumaResidualQuantizationSearch,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> (VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>, u8) {
    let best = select_vvc_scored_luma_residual_block_with_mts(
        residual_coding,
        requested_mts_index,
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        allow_explicit_mts,
        quantization_search,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    (best.block, best.mts_index)
}

fn select_vvc_scored_luma_residual_block_with_mts(
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    allow_explicit_mts: bool,
    quantization_search: VvcLumaResidualQuantizationSearch,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcScoredLumaResidualBlock {
    if matches!(residual_coding, VvcTuResidualCodingMode::TransformSkip) {
        let block = finalize_vvc_luma_residual_block(
            residual_coding,
            0,
            residuals,
            width,
            height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            quantization_search,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        return VvcScoredLumaResidualBlock::new(
            residuals,
            width,
            height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            block,
            0,
            transform_scratch,
            reconstructed_residual,
        );
    }

    let transform_skip = select_vvc_scored_luma_transform_skip_candidate(
        residual_coding,
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    if let Some(transform_skip) = transform_skip {
        if matches!(
            quantization_search,
            VvcLumaResidualQuantizationSearch::TransformSkipFirstModeDecision
        ) {
            return transform_skip;
        }
        if vvc_transform_skip_short_circuits_transformed(transform_skip.score) {
            return transform_skip;
        }
    }

    let base = finalize_vvc_luma_residual_block(
        residual_coding,
        0,
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        quantization_search,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let mut best = VvcScoredLumaResidualBlock::new(
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        base,
        0,
        transform_scratch,
        reconstructed_residual,
    );

    if let Some(transform_skip) = transform_skip {
        if transform_skip.selects_over(best) {
            best = transform_skip;
        }
    }

    if allow_explicit_mts
        && vvc_luma_mts_selection_allowed(
            residual_coding,
            requested_mts_index,
            width,
            height,
            luma_qp,
            base.has_ac,
        )
    {
        if matches!(requested_mts_index, 2..=5) {
            let candidate = finalize_vvc_luma_residual_block(
                residual_coding,
                requested_mts_index,
                residuals,
                width,
                height,
                bit_depth,
                luma_qp,
                luma_ts_quant,
                quantization_search,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            if !vvc_luma_explicit_mts_candidate_is_signalable(candidate) {
                return best;
            }
            let candidate = VvcScoredLumaResidualBlock::new(
                residuals,
                width,
                height,
                bit_depth,
                luma_qp,
                luma_ts_quant,
                candidate,
                requested_mts_index,
                transform_scratch,
                reconstructed_residual,
            );
            if candidate.selects_over(best) {
                best = candidate;
            }
        } else {
            for candidate_mts_index in VVC_LUMA_EXPLICIT_MTS_CANDIDATES {
                let candidate = finalize_vvc_luma_residual_block(
                    residual_coding,
                    candidate_mts_index,
                    residuals,
                    width,
                    height,
                    bit_depth,
                    luma_qp,
                    luma_ts_quant,
                    quantization_search,
                    stats,
                    transform_scratch,
                    reconstructed_residual,
                );
                if !vvc_luma_explicit_mts_candidate_is_signalable(candidate) {
                    continue;
                }
                let candidate = VvcScoredLumaResidualBlock::new(
                    residuals,
                    width,
                    height,
                    bit_depth,
                    luma_qp,
                    luma_ts_quant,
                    candidate,
                    candidate_mts_index,
                    transform_scratch,
                    reconstructed_residual,
                );
                if candidate.selects_over(best) {
                    best = candidate;
                }
            }
        }
    }

    best
}

fn vvc_luma_explicit_mts_candidate_is_signalable(
    candidate: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
) -> bool {
    // VTM only parses/writes mts_idx when CUCtx::mtsLastScanPos is true, which
    // is derived from scanPosLast() >= 1. A DC-only TU therefore cannot carry
    // a non-default explicit MTS index; selecting one would make the encoder's
    // internal reconstruction use MTS while a reference decoder infers DCT-II.
    candidate.has_ac
}

#[derive(Debug, Clone, Copy)]
struct VvcScoredLumaResidualBlock {
    block: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
    score: VvcResidualBlockScore,
}

impl VvcScoredLumaResidualBlock {
    fn new(
        residuals: &[i16],
        width: u16,
        height: u16,
        bit_depth: SampleBitDepth,
        luma_qp: i32,
        luma_ts_quant: &VvcTransformSkipQuantTable,
        block: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
        mts_index: u8,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let score = vvc_luma_residual_block_score(
            residuals,
            width,
            height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            block,
            mts_index,
            transform_scratch,
            reconstructed_residual,
        );
        Self {
            block,
            mts_index,
            score,
        }
    }

    fn selects_over(self, best: Self) -> bool {
        self.score.selects_over(best.score)
    }
}

fn vvc_luma_mts_selection_allowed(
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    width: u16,
    height: u16,
    luma_qp: i32,
    base_has_ac: bool,
) -> bool {
    let valid_request = requested_mts_index == 0 || matches!(requested_mts_index, 2..=5);
    VVC_ENABLE_LUMA_MTS_SELECTION
        && valid_request
        && matches!(residual_coding, VvcTuResidualCodingMode::Transformed)
        && luma_qp > 0
        && base_has_ac
        && matches!(width, 4 | 8)
        && matches!(height, 4 | 8)
}

fn select_vvc_scored_luma_transform_skip_candidate(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcScoredLumaResidualBlock> {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if !vvc_luma_lossy_transform_skip_selection_allowed(residual_coding, width, height, luma_qp) {
        return None;
    }

    #[cfg(feature = "vvc-stats")]
    let quant_start = StageStart::now();
    let transform_skip =
        finalize_vvc_luma_transform_skip_residual_block(residuals, width, height, luma_ts_quant);
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
    if !transform_skip.has_ac && transform_skip.dc_level == 0 {
        return None;
    }

    let transform_skip = VvcScoredLumaResidualBlock::new(
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        transform_skip,
        0,
        transform_scratch,
        reconstructed_residual,
    );
    Some(transform_skip)
}

fn vvc_luma_lossy_transform_skip_selection_allowed(
    residual_coding: VvcTuResidualCodingMode,
    width: u16,
    height: u16,
    luma_qp: i32,
) -> bool {
    VVC_ENABLE_LOSSY_TRANSFORM_SKIP_SELECTION
        && matches!(residual_coding, VvcTuResidualCodingMode::Transformed)
        && luma_qp > 0
        && width <= VVC_TRANSFORM_SKIP_MAX_SIZE
        && height <= VVC_TRANSFORM_SKIP_MAX_SIZE
}

fn vvc_luma_residual_block_score(
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
) -> VvcResidualBlockScore {
    let distortion = if vvc_luma_transform_skip_score_is_exact(residual, width, height, bit_depth, qp)
    {
        0
    } else {
        luma_reconstructed_residual_sse(
            source_residuals,
            width,
            height,
            bit_depth,
            qp,
            ts_quant,
            residual,
            mts_index,
            transform_scratch,
            reconstructed_residual,
        )
    };
    let rate_cost = u64::from(residual.dc_level != 0)
        .saturating_mul(8)
        .saturating_add(luma_ac_syntax_cost_estimate(
            width,
            height,
            &residual.ac_levels,
        ))
        .saturating_add(luma_mts_syntax_cost_estimate(
            residual.has_ac && !residual.transform_skip,
            mts_index,
        ))
        .saturating_add(u64::from(residual.transform_skip));
    VvcResidualBlockScore {
        distortion,
        rate_cost,
    }
}

fn vvc_luma_transform_skip_score_is_exact(
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> bool {
    if !residual.transform_skip {
        return false;
    }
    let width = usize::from(width);
    let height = usize::from(height);
    let (active_width, active_height) = vvc_luma_transform_skip_active_extent(width, height);
    active_width == width && active_height == height && vvc_transform_skip_qp_reconstructs_exact(bit_depth, qp)
}

#[derive(Debug, Clone, Copy)]
struct VvcResidualBlockScore {
    distortion: u64,
    rate_cost: u64,
}

impl VvcResidualBlockScore {
    fn selects_over(self, best: Self) -> bool {
        vvc_rd_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn vvc_transform_skip_short_circuits_transformed(score: VvcResidualBlockScore) -> bool {
    score.distortion == 0
}

fn luma_mts_syntax_cost_estimate(has_ac: bool, mts_index: u8) -> u64 {
    if !has_ac {
        return 0;
    }
    match mts_index {
        0 => 1,
        2 => 2,
        3 => 3,
        4 | 5 => 4,
        _ => 8,
    }
}

fn finalize_vvc_luma_residual_block(
    residual_coding: VvcTuResidualCodingMode,
    mts_index: u8,
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    quantization_search: VvcLumaResidualQuantizationSearch,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU> {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            debug_assert_eq!(mts_index, 0);
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let block = finalize_vvc_luma_transform_skip_residual_block(
                residuals,
                width,
                height,
                luma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
            block
        }
        VvcTuResidualCodingMode::Transformed => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let quantized = match quantization_search {
                VvcLumaResidualQuantizationSearch::Full => {
                    quantize_vvc_luma_residual_greedy_with_qp_and_mts_into(
                        residuals,
                        width,
                        height,
                        bit_depth,
                        luma_qp,
                        mts_index,
                        transform_scratch,
                        reconstructed_residual,
                    )
                }
                VvcLumaResidualQuantizationSearch::FastModeDecision => {
                    quantize_vvc_luma_residual_fast_with_qp_and_mts_into(
                        residuals,
                        width,
                        height,
                        bit_depth,
                        luma_qp,
                        mts_index,
                        transform_scratch,
                        reconstructed_residual,
                    )
                }
                VvcLumaResidualQuantizationSearch::TransformSkipFirstModeDecision => {
                    quantize_vvc_luma_residual_fast_with_qp_and_mts_into(
                        residuals,
                        width,
                        height,
                        bit_depth,
                        luma_qp,
                        mts_index,
                        transform_scratch,
                        reconstructed_residual,
                    )
                }
            };
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_transformed_quant_nanos(vvc_elapsed_nanos(quant_start));
            VvcFinalizedResidualBlock {
                dc_level: quantized.reconstructed_dc_coeff,
                ac_levels: quantized.reconstructed_ac_coeffs,
                has_ac: quantized.has_ac,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            }
        }
    }
}

fn reconstruct_vvc_luma_residual_block_into(
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
    reconstructed_residual: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
) {
    if residual.transform_skip {
        if residual.bdpcm_mode.is_enabled() {
            reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                usize::from(width),
                usize::from(height),
                luma_ts_quant,
                residual.bdpcm_mode,
            );
        } else {
            reconstruct_vvc_luma_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                usize::from(width),
                usize::from(height),
                luma_ts_quant,
            );
        }
    } else {
        inverse_transform_vvc_luma_quantized_block_into_with_qp_and_mts(
            reconstructed_residual,
            transform_scratch,
            width,
            height,
            residual.dc_level,
            &residual.ac_levels,
            bit_depth,
            luma_qp,
            mts_index,
        );
    }
}

fn finalize_vvc_luma_transform_skip_residual_block(
    residuals: &[i16],
    width: u16,
    height: u16,
    quant_table: &VvcTransformSkipQuantTable,
) -> VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU> {
    debug_assert_eq!(residuals.len(), usize::from(width) * usize::from(height));
    if residuals.iter().all(|&residual| residual == 0) {
        return VvcFinalizedResidualBlock {
            dc_level: 0,
            ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
            has_ac: false,
            transform_skip: true,
            bdpcm_mode: VvcBdpcmMode::None,
        };
    }
    let dc_level = residuals
        .first()
        .copied()
        .map(|level| quant_table.level(level))
        .unwrap_or(0);
    let (ac_levels, has_ac) =
        transform_skip_luma_ac_levels_and_flag_with_table(residuals, usize::from(width), quant_table);
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::None,
    }
}

fn finalize_vvc_luma_bdpcm_transform_skip_residual_block(
    residuals: &[i16],
    width: u16,
    height: u16,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) -> VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU> {
    debug_assert!(bdpcm_mode.is_enabled());
    debug_assert_eq!(residuals.len(), usize::from(width) * usize::from(height));
    if residuals.iter().all(|&residual| residual == 0) {
        return VvcFinalizedResidualBlock {
            dc_level: 0,
            ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
            has_ac: false,
            transform_skip: true,
            bdpcm_mode,
        };
    }
    let (active_width, active_height) =
        vvc_luma_transform_skip_active_extent(usize::from(width), usize::from(height));
    let mut quantized_levels = [0i16; 64];
    let mut ac_levels = [0; VVC_LUMA_AC_COEFFS_PER_TU];
    let mut dc_level = 0i16;
    let mut has_ac = false;
    for y in 0..active_height {
        for x in 0..active_width {
            let level = quant_table.level(residuals[y * usize::from(width) + x]);
            quantized_levels[y * active_width + x] = level;
            let predictor = match bdpcm_mode {
                VvcBdpcmMode::None => unreachable!("BDPCM block requires a direction"),
                VvcBdpcmMode::Horizontal if x > 0 => quantized_levels[y * active_width + x - 1],
                VvcBdpcmMode::Vertical if y > 0 => quantized_levels[(y - 1) * active_width + x],
                VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => 0,
            };
            let coeff = (i32::from(level) - i32::from(predictor))
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            if x == 0 && y == 0 {
                dc_level = coeff;
            } else {
                ac_levels[y * active_width + x - 1] = coeff;
                has_ac |= coeff != 0;
            }
        }
    }
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode,
    }
}
