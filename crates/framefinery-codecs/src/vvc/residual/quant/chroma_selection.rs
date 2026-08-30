fn vvc_chroma_fast_search_uses_transform_skip_candidate(policy: VvcResidualCodingPolicy) -> bool {
    // Chroma transform skip remains available as a candidate, but lossy fast
    // search must compare it against transformed residual coding for 8-bit
    // 4:4:4 screen content. For flat 4:4:4/RGB blocks, forcing transform skip
    // emits many AC coefficients where transformed coding can collapse the
    // block to DC-only. Other formats keep the prior shortcut for throughput.
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
        && (policy.chroma_sampling() != ChromaSampling::Cs444 || policy.bit_depth().bits() != 8)
}

fn select_vvc_chroma_residual_block_with_transform_skip(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    transformed: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if !vvc_chroma_lossy_transform_skip_selection_allowed(residual_coding, width, height, chroma_qp)
    {
        return transformed;
    }

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
    if !transform_skip.has_ac && transform_skip.dc_level == 0 {
        return transformed;
    }

    select_best_vvc_chroma_residual_block(
        residuals,
        width,
        height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        transformed,
        transform_skip,
        transform_scratch,
        reconstructed_residual,
    )
}

fn select_best_vvc_chroma_residual_block(
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    transformed: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    transform_skip: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    let transformed_score = vvc_chroma_residual_block_score(
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
    let transform_skip_score = vvc_chroma_residual_block_score(
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
    if transform_skip_score.selects_over(transformed_score) {
        transform_skip
    } else {
        transformed
    }
}
