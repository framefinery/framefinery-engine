#[derive(Debug, Clone, Copy)]
struct VvcSelectedLumaResidual {
    block: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
}

type VvcScoredSelectedLumaResidual = VvcScoredResidual<VvcSelectedLumaResidual>;

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

    fn from_block(
        source_residuals: &[i16],
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
            source_residuals,
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
            residual: VvcSelectedLumaResidual { block, mts_index },
            score,
        }
    }
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
    (best.residual.block, best.residual.mts_index)
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
) -> VvcScoredSelectedLumaResidual {
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
        return VvcScoredSelectedLumaResidual::from_block(
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
    let mut best = VvcScoredSelectedLumaResidual::from_block(
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
            let candidate = VvcScoredSelectedLumaResidual::from_block(
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
                let candidate = VvcScoredSelectedLumaResidual::from_block(
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
) -> Option<VvcScoredSelectedLumaResidual> {
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

    let transform_skip = VvcScoredSelectedLumaResidual::from_block(
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
    let distortion =
        if vvc_luma_transform_skip_score_is_exact(residual, width, height, bit_depth, qp) {
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
    active_width == width
        && active_height == height
        && vvc_transform_skip_qp_reconstructs_exact(bit_depth, qp)
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
    let width = usize::from(width);
    let height = usize::from(height);
    finalize_vvc_transform_skip_residual_block(
        residuals,
        vvc_luma_transform_skip_residual_layout(width, height),
        quant_table,
    )
}

fn finalize_vvc_luma_bdpcm_transform_skip_residual_block(
    residuals: &[i16],
    width: u16,
    height: u16,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) -> VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU> {
    let width = usize::from(width);
    let height = usize::from(height);
    finalize_vvc_bdpcm_transform_skip_residual_block(
        residuals,
        vvc_luma_transform_skip_residual_layout(width, height),
        quant_table,
        bdpcm_mode,
    )
}
