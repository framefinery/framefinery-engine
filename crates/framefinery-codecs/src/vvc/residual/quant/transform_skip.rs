#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_luma_transform_skip_residuals_into(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = dc_level;
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            residuals[y * width + x] = ac_levels[y * active_width + x - 1];
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
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    residuals[0] = reconstruct_vvc_transform_skip_level_with_params(dc_level, scale, right_shift);
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            residuals[y * width + x] = reconstruct_vvc_transform_skip_level_with_params(
                ac_levels[y * active_width + x - 1],
                scale,
                right_shift,
            );
        }
    }
}

fn reconstruct_vvc_luma_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = quant_table.reconstructed(dc_level);
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            residuals[y * width + x] =
                quant_table.reconstructed(ac_levels[y * active_width + x - 1]);
        }
    }
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
    debug_assert!(bdpcm_mode.is_enabled());
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let (active_width, active_height) = vvc_luma_transform_skip_active_extent(width, height);
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    let mut levels = [0i16; 64];
    levels[0] = dc_level;
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            levels[y * active_width + x] = ac_levels[y * active_width + x - 1];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(&mut levels, active_width, active_height, bdpcm_mode);
    for y in 0..active_height {
        for x in 0..active_width {
            residuals[y * width + x] = reconstruct_vvc_transform_skip_level_with_params(
                levels[y * active_width + x],
                scale,
                right_shift,
            );
        }
    }
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
    debug_assert!(bdpcm_mode.is_enabled());
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let (active_width, active_height) = vvc_luma_transform_skip_active_extent(width, height);
    let mut levels = [0i16; 64];
    levels[0] = dc_level;
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            levels[y * active_width + x] = ac_levels[y * active_width + x - 1];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(&mut levels, active_width, active_height, bdpcm_mode);
    for y in 0..active_height {
        for x in 0..active_width {
            residuals[y * width + x] = quant_table.reconstructed(levels[y * active_width + x]);
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_chroma_transform_skip_residuals_into(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = dc_level;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < width && y < height {
            residuals[y * width + x] = ac_levels[slot];
        }
    }
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
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    residuals[0] = reconstruct_vvc_transform_skip_level_with_params(dc_level, scale, right_shift);
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < width && y < height {
            residuals[y * width + x] = reconstruct_vvc_transform_skip_level_with_params(
                ac_levels[slot],
                scale,
                right_shift,
            );
        }
    }
}

fn reconstruct_vvc_chroma_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = quant_table.reconstructed(dc_level);
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < width && y < height {
            residuals[y * width + x] = quant_table.reconstructed(ac_levels[slot]);
        }
    }
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
    debug_assert!(bdpcm_mode.is_enabled());
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let active_width = width.min(4);
    let active_height = height.min(4);
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    let mut levels = [0i16; 16];
    levels[0] = dc_level;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < active_width && y < active_height {
            levels[y * 4 + x] = ac_levels[slot];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(&mut levels, 4, active_height, bdpcm_mode);
    for y in 0..active_height {
        for x in 0..active_width {
            residuals[y * width + x] = reconstruct_vvc_transform_skip_level_with_params(
                levels[y * 4 + x],
                scale,
                right_shift,
            );
        }
    }
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
    debug_assert!(bdpcm_mode.is_enabled());
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let active_width = width.min(4);
    let active_height = height.min(4);
    let mut levels = [0i16; 16];
    levels[0] = dc_level;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < active_width && y < active_height {
            levels[y * 4 + x] = ac_levels[slot];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(&mut levels, 4, active_height, bdpcm_mode);
    for y in 0..active_height {
        for x in 0..active_width {
            residuals[y * width + x] = quant_table.reconstructed(levels[y * 4 + x]);
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn transform_skip_luma_ac_levels_and_flag(
    residuals: &[i16],
    width: usize,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; super::VVC_LUMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let height = residuals.len() / width;
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let raster_idx = y * width + x;
            if raster_idx < residuals.len() {
                let level = residuals[raster_idx];
                levels[y * active_width + x - 1] = level;
                has_ac |= level != 0;
            }
        }
    }
    (levels, has_ac)
}

#[cfg(test)]
pub(in crate::vvc) fn transform_skip_luma_ac_levels_and_flag_with_qp(
    residuals: &[i16],
    width: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    transform_skip_luma_ac_levels_and_flag_with_params(residuals, width, scale, right_shift)
}

#[cfg(test)]
fn transform_skip_luma_ac_levels_and_flag_with_params(
    residuals: &[i16],
    width: usize,
    scale: i32,
    right_shift: i32,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; super::VVC_LUMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let height = residuals.len() / width;
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let raster_idx = y * width + x;
            if raster_idx < residuals.len() {
                let level = quantize_vvc_transform_skip_level_with_params(
                    residuals[raster_idx],
                    scale,
                    right_shift,
                    VVC_TRANSFORM_SKIP_LEVEL_SEARCH_RADIUS,
                );
                levels[y * active_width + x - 1] = level;
                has_ac |= level != 0;
            }
        }
    }
    (levels, has_ac)
}

fn transform_skip_luma_ac_levels_and_flag_with_table(
    residuals: &[i16],
    width: usize,
    quant_table: &VvcTransformSkipQuantTable,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; super::VVC_LUMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let height = residuals.len() / width;
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let raster_idx = y * width + x;
            let level = quant_table.level(residuals[raster_idx]);
            levels[y * active_width + x - 1] = level;
            has_ac |= level != 0;
        }
    }
    (levels, has_ac)
}

fn vvc_luma_transform_skip_active_extent(width: usize, height: usize) -> (usize, usize) {
    if width == 8 && height == 8 {
        (8, 8)
    } else {
        (width.min(4), height.min(4))
    }
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

#[cfg(test)]
pub(in crate::vvc) fn transform_skip_chroma_ac_levels_and_flag(
    residuals: &[i16],
    width: usize,
) -> ([i16; VVC_CHROMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        let raster_idx = y * width + x;
        if raster_idx < residuals.len() {
            let level = residuals[raster_idx];
            levels[slot] = level;
            has_ac |= level != 0;
        }
    }
    (levels, has_ac)
}

fn transform_skip_chroma_ac_levels_and_flag_with_table(
    residuals: &[i16],
    width: usize,
    quant_table: &VvcTransformSkipQuantTable,
) -> ([i16; VVC_CHROMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let height = residuals.len() / width;
    if width >= 4 && height >= 4 {
        for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
            let level = quant_table.level(residuals[y * width + x]);
            levels[slot] = level;
            has_ac |= level != 0;
        }
        return (levels, has_ac);
    }
    let active_width = width.min(4);
    let active_height = height.min(4);
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < active_width && y < active_height {
            let level = quant_table.level(residuals[y * width + x]);
            levels[slot] = level;
            has_ac |= level != 0;
        }
    }
    (levels, has_ac)
}

#[cfg(test)]
pub(in crate::vvc) fn quantize_vvc_transform_skip_level(
    residual: i16,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> i16 {
    quantize_vvc_transform_skip_level_with_radius(
        residual,
        bit_depth,
        qp,
        VVC_TRANSFORM_SKIP_LEVEL_SEARCH_RADIUS,
    )
}

#[cfg(test)]
fn quantize_vvc_transform_skip_level_with_radius(
    residual: i16,
    bit_depth: SampleBitDepth,
    qp: i32,
    search_radius: i64,
) -> i16 {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    quantize_vvc_transform_skip_level_with_params(residual, scale, right_shift, search_radius)
}

#[inline]
fn quantize_vvc_transform_skip_level_with_params(
    residual: i16,
    scale: i32,
    right_shift: i32,
    search_radius: i64,
) -> i16 {
    if residual == 0 {
        return 0;
    }
    let estimate = if right_shift > 0 {
        div_round_nearest_i64(i64::from(residual) << right_shift, i64::from(scale))
    } else {
        div_round_nearest_i64(
            i64::from(residual),
            i64::from(scale) << (-right_shift as u32),
        )
    };
    if search_radius == 1 {
        return quantize_vvc_transform_skip_level_radius_one_with_params(
            estimate,
            residual,
            scale,
            right_shift,
        );
    }
    let mut best_level = estimate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
    let mut best_error =
        vvc_transform_skip_level_error_with_params(best_level, residual, scale, right_shift);
    for candidate in (estimate - search_radius)..=(estimate + search_radius) {
        let level = candidate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
        if level == best_level {
            continue;
        }
        let error = vvc_transform_skip_level_error_with_params(level, residual, scale, right_shift);
        if error < best_error
            || (error == best_error && level.unsigned_abs() < best_level.unsigned_abs())
        {
            best_error = error;
            best_level = level;
        }
    }
    best_level
}

#[inline]
fn quantize_vvc_transform_skip_level_radius_one_with_params(
    estimate: i64,
    residual: i16,
    scale: i32,
    right_shift: i32,
) -> i16 {
    let mut best_level = estimate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
    let mut best_error =
        vvc_transform_skip_level_error_with_params(best_level, residual, scale, right_shift);
    for candidate in [estimate - 1, estimate + 1] {
        let level = candidate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
        if level == best_level {
            continue;
        }
        let error = vvc_transform_skip_level_error_with_params(level, residual, scale, right_shift);
        if error < best_error
            || (error == best_error && level.unsigned_abs() < best_level.unsigned_abs())
        {
            best_error = error;
            best_level = level;
        }
    }
    best_level
}

fn div_round_nearest_i64(value: i64, divisor: i64) -> i64 {
    debug_assert!(divisor > 0);
    if value < 0 {
        -(((-value) + (divisor / 2)) / divisor)
    } else {
        (value + (divisor / 2)) / divisor
    }
}

#[inline]
fn vvc_transform_skip_level_error_with_params(
    level: i16,
    residual: i16,
    scale: i32,
    right_shift: i32,
) -> u64 {
    let reconstructed = reconstruct_vvc_transform_skip_level_with_params(level, scale, right_shift);
    let diff = i64::from(residual) - i64::from(reconstructed);
    (diff * diff) as u64
}

#[inline]
fn reconstruct_vvc_transform_skip_level_with_params(
    level: i16,
    scale: i32,
    right_shift: i32,
) -> i16 {
    if level == 0 {
        return 0;
    }
    let value = if right_shift > 0 {
        let add = 1i64 << ((right_shift - 1) as u32);
        (i64::from(level) * i64::from(scale) + add) >> (right_shift as u32)
    } else {
        i64::from(level) * i64::from(scale) * (1i64 << ((-right_shift) as u32))
    };
    value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}

fn vvc_transform_skip_dequant_params(bit_depth: SampleBitDepth, qp: i32) -> (i32, i32) {
    let qp_bd_offset = (i32::from(bit_depth.bits()) - 8) * 6;
    let transform_skip_qp = (qp + qp_bd_offset).max(4);
    let qp_rem = transform_skip_qp.rem_euclid(6) as usize;
    let qp_per = transform_skip_qp.div_euclid(6);
    let scale = VVC_TRANSFORM_SKIP_INV_QUANT_SCALES[qp_rem];
    let right_shift = 6 - qp_per;
    (scale, right_shift)
}

fn vvc_transform_skip_qp_reconstructs_exact(bit_depth: SampleBitDepth, qp: i32) -> bool {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    right_shift >= 0 && i64::from(scale) == (1i64 << (right_shift as u32))
}
