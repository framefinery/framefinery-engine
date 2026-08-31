const VVC_TRANSFORM_SKIP_MAX_SAMPLES: usize = 64;

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_luma_transform_skip_residuals_into(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
) {
    reconstruct_vvc_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_luma_transform_skip_residual_layout(width, height),
        |level| level,
    );
}

fn reconstruct_vvc_transform_skip_residuals_into<const AC_COEFFS: usize>(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; AC_COEFFS],
    layout: VvcTransformSkipResidualLayout,
    reconstruct_level: impl Fn(i16) -> i16,
) {
    layout.debug_assert_coefficients_fit::<AC_COEFFS>();
    debug_assert_eq!(layout.coefficient_stride, layout.active_width);
    residuals.clear();
    residuals.resize(layout.source_width * layout.source_height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = reconstruct_level(dc_level);
    for y in 0..layout.active_height {
        for x in 0..layout.active_width {
            if x == 0 && y == 0 {
                continue;
            }
            residuals[y * layout.source_width + x] =
                reconstruct_level(ac_levels[y * layout.coefficient_stride + x - 1]);
        }
    }
}

fn reconstruct_vvc_bdpcm_transform_skip_residuals_into<const AC_COEFFS: usize>(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; AC_COEFFS],
    layout: VvcTransformSkipResidualLayout,
    bdpcm_mode: VvcBdpcmMode,
    reconstruct_level: impl Fn(i16) -> i16,
) {
    debug_assert!(bdpcm_mode.is_enabled());
    layout.debug_assert_coefficients_fit::<AC_COEFFS>();
    residuals.clear();
    residuals.resize(layout.source_width * layout.source_height, 0);
    if residuals.is_empty() {
        return;
    }
    let mut levels = [0i16; VVC_TRANSFORM_SKIP_MAX_SAMPLES];
    levels[0] = dc_level;
    for y in 0..layout.active_height {
        for x in 0..layout.active_width {
            if x == 0 && y == 0 {
                continue;
            }
            levels[y * layout.coefficient_stride + x] =
                ac_levels[y * layout.coefficient_stride + x - 1];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(
        &mut levels,
        layout.coefficient_stride,
        layout.active_height,
        bdpcm_mode,
    );
    for y in 0..layout.active_height {
        for x in 0..layout.active_width {
            residuals[y * layout.source_width + x] =
                reconstruct_level(levels[y * layout.coefficient_stride + x]);
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_luma_transform_skip_residuals_into_with_qp(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    reconstruct_vvc_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_luma_transform_skip_residual_layout(width, height),
        |level| reconstruct_vvc_transform_skip_level_with_params(level, scale, right_shift),
    );
}

fn reconstruct_vvc_luma_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) {
    reconstruct_vvc_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_luma_transform_skip_residual_layout(width, height),
        |level| quant_table.reconstructed(level),
    );
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_qp(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
    bdpcm_mode: VvcBdpcmMode,
) {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    reconstruct_vvc_bdpcm_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_luma_transform_skip_residual_layout(width, height),
        bdpcm_mode,
        |level| reconstruct_vvc_transform_skip_level_with_params(level, scale, right_shift),
    );
}

fn reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) {
    reconstruct_vvc_bdpcm_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_luma_transform_skip_residual_layout(width, height),
        bdpcm_mode,
        |level| quant_table.reconstructed(level),
    );
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_chroma_transform_skip_residuals_into(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
) {
    reconstruct_vvc_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_chroma_transform_skip_residual_layout(width, height),
        |level| level,
    );
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_chroma_transform_skip_residuals_into_with_qp(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    reconstruct_vvc_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_chroma_transform_skip_residual_layout(width, height),
        |level| reconstruct_vvc_transform_skip_level_with_params(level, scale, right_shift),
    );
}

fn reconstruct_vvc_chroma_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) {
    reconstruct_vvc_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_chroma_transform_skip_residual_layout(width, height),
        |level| quant_table.reconstructed(level),
    );
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_qp(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
    bdpcm_mode: VvcBdpcmMode,
) {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    reconstruct_vvc_bdpcm_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_chroma_bdpcm_transform_skip_residual_layout(width, height),
        bdpcm_mode,
        |level| reconstruct_vvc_transform_skip_level_with_params(level, scale, right_shift),
    );
}

fn reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) {
    reconstruct_vvc_bdpcm_transform_skip_residuals_into(
        residuals,
        dc_level,
        ac_levels,
        vvc_chroma_bdpcm_transform_skip_residual_layout(width, height),
        bdpcm_mode,
        |level| quant_table.reconstructed(level),
    );
}

fn inverse_bdpcm_quantized_levels_in_place(
    levels: &mut [i16],
    stride: usize,
    height: usize,
    bdpcm_mode: VvcBdpcmMode,
) {
    match bdpcm_mode {
        VvcBdpcmMode::None => unreachable!("BDPCM inverse requires a direction"),
        VvcBdpcmMode::Horizontal => {
            for y in 0..height {
                let row = y * stride;
                for x in 1..stride {
                    let idx = row + x;
                    levels[idx] = (i32::from(levels[idx]) + i32::from(levels[idx - 1]))
                        .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                        as i16;
                }
            }
        }
        VvcBdpcmMode::Vertical => {
            for y in 1..height {
                let row = y * stride;
                let above = row - stride;
                for x in 0..stride {
                    let idx = row + x;
                    levels[idx] = (i32::from(levels[idx]) + i32::from(levels[above + x]))
                        .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                        as i16;
                }
            }
        }
    }
}
