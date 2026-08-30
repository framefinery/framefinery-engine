fn quantize_direct_luma_ac_coeffs_into(
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    qp: i32,
    dc_level: i16,
    mts_index: u8,
    ac_search: VvcLumaAcCandidateSearch,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> ([i16; VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    if ac_search == VvcLumaAcCandidateSearch::FastModeDecision
        || !VVC_ENABLE_LUMA_DCT_COEFF_SELECTION
        || mts_index != 0
        || width != 8
        || height != 8
        || !residuals_have_ac_energy(residuals)
    {
        if mts_index == 0 {
            let _selector_inputs = (bit_depth, dc_level);
            return quantize_legacy_luma_ac_coeffs(residuals, width, height, qp);
        }
        return quantize_transform_luma_ac_coeffs(residuals, width, height, qp, mts_index);
    }

    let legacy = quantize_legacy_luma_ac_coeffs(residuals, width, height, qp);
    let dct = quantize_transform_luma_ac_coeffs(residuals, width, height, qp, 0);
    select_luma_ac_coeff_candidate_into(
        residuals,
        width,
        height,
        bit_depth,
        qp,
        dc_level,
        legacy,
        dct,
        0,
        transform_scratch,
        reconstructed_residual,
    )
}

fn select_luma_ac_coeff_candidate_into(
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    qp: i32,
    dc_level: i16,
    legacy: ([i16; VVC_LUMA_AC_COEFFS_PER_TU], bool),
    dct: ([i16; VVC_LUMA_AC_COEFFS_PER_TU], bool),
    mts_index: u8,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> ([i16; VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let legacy_sse = luma_reconstructed_residual_sse_with_mts_into(
        residuals,
        width,
        height,
        bit_depth,
        qp,
        dc_level,
        &legacy.0,
        mts_index,
        transform_scratch,
        reconstructed_residual,
    );
    let dct_sse = luma_reconstructed_residual_sse_with_mts_into(
        residuals,
        width,
        height,
        bit_depth,
        qp,
        dc_level,
        &dct.0,
        mts_index,
        transform_scratch,
        reconstructed_residual,
    );
    if dct_sse >= legacy_sse {
        return legacy;
    }

    let lambda = luma_coeff_rd_lambda(qp, bit_depth);
    let legacy_score = legacy_sse.saturating_add(
        lambda.saturating_mul(luma_ac_syntax_cost_estimate(width, height, &legacy.0)),
    );
    let dct_score = dct_sse
        .saturating_add(lambda.saturating_mul(luma_ac_syntax_cost_estimate(width, height, &dct.0)));
    if dct_score < legacy_score {
        dct
    } else {
        legacy
    }
}
