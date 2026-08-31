#[derive(Debug, Clone, Copy)]
struct VvcSelectedChromaResidual {
    cb: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    cr: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
}

type VvcScoredSelectedChromaResidual = VvcScoredResidual<VvcSelectedChromaResidual>;

impl VvcScoredSelectedChromaResidual {
    pub(super) fn from_scored_blocks(
        residual: VvcSelectedChromaResidual,
        cb_score: VvcResidualBlockScore,
        cr_score: VvcResidualBlockScore,
        width: usize,
        height: usize,
    ) -> Self {
        Self {
            score: VvcResidualBlockScore {
                distortion: cb_score.distortion.saturating_add(cr_score.distortion),
                rate_cost: chroma_coeff_syntax_cost_estimate(width, height, residual.cb)
                    .saturating_add(chroma_coeff_syntax_cost_estimate(
                        width,
                        height,
                        residual.cr,
                    )),
            },
            residual,
        }
    }

    fn new(
        cb_residuals: &[i16],
        cr_residuals: &[i16],
        width: usize,
        height: usize,
        bit_depth: SampleBitDepth,
        chroma_qp: i32,
        chroma_ts_quant: &VvcTransformSkipQuantTable,
        residual: VvcSelectedChromaResidual,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let cb_score = vvc_chroma_residual_block_score(
            cb_residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual.cb,
            transform_scratch,
            reconstructed_residual,
        );
        let cr_score = vvc_chroma_residual_block_score(
            cr_residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual.cr,
            transform_scratch,
            reconstructed_residual,
        );
        Self::from_scored_blocks(residual, cb_score, cr_score, width, height)
    }
}

fn finalize_vvc_chroma_residual_block(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    // Keep this defensive gate beside finalization as well as mode selection:
    // callers that provide a preselected decision must still take a legal,
    // exactly reconstructable path.
    let residual_coding = if matches!(residual_coding, VvcTuResidualCodingMode::TransformSkip)
        && (width > 8 || height > 8)
    {
        VvcTuResidualCodingMode::Transformed
    } else {
        residual_coding
    };
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let block = finalize_vvc_chroma_transform_skip_residual_block(
                residuals,
                width,
                height,
                chroma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
            block
        }
        VvcTuResidualCodingMode::Transformed => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let quantized = quantize_vvc_chroma_residual_greedy_with_qp(
                residuals,
                width as u16,
                height as u16,
                bit_depth,
                chroma_qp,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transformed_quant_nanos(vvc_elapsed_nanos(quant_start));
            let transformed = VvcFinalizedResidualBlock {
                dc_level: quantized.reconstructed_dc_coeff,
                ac_levels: quantized.reconstructed_ac_coeffs,
                has_ac: quantized.has_ac,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            };
            select_vvc_chroma_residual_block_with_transform_skip(
                residual_coding,
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                transformed,
                stats,
                transform_scratch,
                reconstructed_residual,
            )
        }
    }
}

type VvcScoredChromaResidualBlock =
    VvcScoredResidual<VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>>;

impl VvcScoredChromaResidualBlock {
    fn new(
        residuals: &[i16],
        width: usize,
        height: usize,
        bit_depth: SampleBitDepth,
        chroma_qp: i32,
        chroma_ts_quant: &VvcTransformSkipQuantTable,
        block: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let score = vvc_chroma_residual_block_score(
            residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            block,
            transform_scratch,
            reconstructed_residual,
        );
        Self {
            residual: block,
            score,
        }
    }
}

fn select_vvc_scored_chroma_residual_block_with_transform_skip(
    policy: VvcResidualCodingPolicy,
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcScoredChromaResidualBlock {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let block = finalize_vvc_chroma_transform_skip_residual_block(
                residuals,
                width,
                height,
                chroma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
            VvcScoredChromaResidualBlock::new(
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                block,
                transform_scratch,
                reconstructed_residual,
            )
        }
        VvcTuResidualCodingMode::Transformed => {
            let mut transform_skip_candidate = None;
            if vvc_chroma_lossy_transform_skip_selection_allowed(
                residual_coding,
                width,
                height,
                chroma_qp,
            ) {
                #[cfg(feature = "vvc-stats")]
                let quant_start = StageStart::now();
                let transform_skip = finalize_vvc_chroma_transform_skip_residual_block(
                    residuals,
                    width,
                    height,
                    chroma_ts_quant,
                );
                #[cfg(feature = "vvc-stats")]
                stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
                if transform_skip.has_ac || transform_skip.dc_level != 0 {
                    let transform_skip = VvcScoredChromaResidualBlock::new(
                        residuals,
                        width,
                        height,
                        bit_depth,
                        chroma_qp,
                        chroma_ts_quant,
                        transform_skip,
                        transform_scratch,
                        reconstructed_residual,
                    );
                    if vvc_chroma_fast_search_uses_transform_skip_candidate(policy) {
                        return transform_skip;
                    }
                    transform_skip_candidate = Some(transform_skip);
                }
            }
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let quantized = quantize_vvc_chroma_residual_greedy_with_qp(
                residuals,
                width as u16,
                height as u16,
                bit_depth,
                chroma_qp,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transformed_quant_nanos(vvc_elapsed_nanos(quant_start));
            let transformed = VvcFinalizedResidualBlock {
                dc_level: quantized.reconstructed_dc_coeff,
                ac_levels: quantized.reconstructed_ac_coeffs,
                has_ac: quantized.has_ac,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            };
            let mut best = VvcScoredChromaResidualBlock::new(
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                transformed,
                transform_scratch,
                reconstructed_residual,
            );
            if let Some(transform_skip) = transform_skip_candidate {
                if transform_skip.selects_over(best) {
                    best = transform_skip;
                }
            }
            best
        }
    }
}

fn vvc_chroma_lossy_transform_skip_selection_allowed(
    residual_coding: VvcTuResidualCodingMode,
    width: usize,
    height: usize,
    chroma_qp: i32,
) -> bool {
    VVC_ENABLE_LOSSY_TRANSFORM_SKIP_SELECTION
        && matches!(residual_coding, VvcTuResidualCodingMode::Transformed)
        && chroma_qp > 0
        && width <= 8
        && height <= 8
}

fn vvc_chroma_residual_block_score(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcResidualBlockScore {
    let distortion =
        if vvc_chroma_transform_skip_score_is_exact(residual, width, height, bit_depth, qp) {
            0
        } else {
            chroma_reconstructed_residual_sse(
                source_residuals,
                width,
                height,
                bit_depth,
                qp,
                chroma_ts_quant,
                residual,
                transform_scratch,
                reconstructed_residual,
            )
        };
    let rate_cost = u64::from(residual.dc_level != 0)
        .saturating_mul(8)
        .saturating_add(chroma_coeff_syntax_cost_estimate(width, height, residual))
        .saturating_add(u64::from(residual.transform_skip));
    VvcResidualBlockScore {
        distortion,
        rate_cost,
    }
}

fn vvc_chroma_transform_skip_score_is_exact(
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> bool {
    residual.transform_skip
        && width <= 8
        && height <= 8
        && vvc_transform_skip_qp_reconstructs_exact(bit_depth, qp)
}

fn finalize_vvc_chroma_transform_skip_residual_block(
    residuals: &[i16],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    finalize_vvc_transform_skip_residual_block(
        residuals,
        vvc_chroma_transform_skip_residual_layout(width, height),
        quant_table,
    )
}

fn finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
    residuals: &[i16],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    finalize_vvc_bdpcm_transform_skip_residual_block(
        residuals,
        vvc_chroma_bdpcm_transform_skip_residual_layout(width, height),
        quant_table,
        bdpcm_mode,
    )
}

fn reconstruct_vvc_chroma_residual_block_into(
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    reconstructed_residual: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
) {
    if residual.transform_skip {
        if residual.bdpcm_mode.is_enabled() {
            reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                width,
                height,
                chroma_ts_quant,
                residual.bdpcm_mode,
            );
        } else {
            reconstruct_vvc_chroma_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                width,
                height,
                chroma_ts_quant,
            );
        }
    } else {
        inverse_transform_vvc_chroma_quantized_block_into_with_qp(
            reconstructed_residual,
            transform_scratch,
            width as u16,
            height as u16,
            residual.dc_level,
            &residual.ac_levels,
            bit_depth,
            chroma_qp,
        );
    }
}
