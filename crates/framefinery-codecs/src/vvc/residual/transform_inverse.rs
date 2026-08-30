fn inverse_transform_vvc_quantized_block_into(
    residuals: &mut Vec<i16>,
    scratch: &mut VvcInverseTransformScratch,
    width: u16,
    height: u16,
    dc_level: i16,
    ac_levels: &[i16],
    coeff_stride: usize,
    qp: i32,
    bit_depth: SampleBitDepth,
    mts_index: u8,
) {
    let width_usize = usize::from(width);
    let height_usize = usize::from(height);
    let coefficient_count = width_usize * height_usize;
    debug_assert!(coefficient_count <= VVC_MAX_TRANSFORM_COEFFS);
    debug_assert!([4, 8, 16, 32].contains(&width));
    debug_assert!([4, 8, 16, 32].contains(&height));

    debug_assert!(coeff_stride > 0);
    let active_coefficients = ac_levels.len().saturating_add(1);
    let active_width = width_usize.min(coeff_stride);
    let active_height = height_usize.min(active_coefficients.div_ceil(coeff_stride));
    if mts_index == 0 && ac_levels.iter().all(|level| *level == 0) {
        let dequant_params = vvc_transform_dequant_params(width, height, qp);
        let reconstructed_dc = dc_only_residual_from_level_with_params(
            dc_level,
            width,
            height,
            bit_depth,
            dequant_params,
        ) as i16;
        residuals.clear();
        residuals.resize(coefficient_count, reconstructed_dc);
        return;
    }
    let dequantized = &mut scratch.dequantized[..coefficient_count];
    for level in dequantized.iter_mut() {
        *level = 0;
    }
    let dequant_params = vvc_transform_dequant_params(width, height, qp);
    dequantized[0] = dequantize_vvc_transform_level_with_params(dc_level, dequant_params);
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let compact_idx = y * coeff_stride + x;
            if compact_idx >= active_coefficients {
                continue;
            }
            let level = ac_levels[compact_idx - 1];
            if level != 0 {
                let raster_idx = y * width_usize + x;
                dequantized[raster_idx] =
                    dequantize_vvc_transform_level_with_params(level, dequant_params);
            }
        }
    }
    inverse_transform_vvc_dequantized_levels_into(
        residuals,
        scratch,
        width,
        height,
        active_width,
        active_height,
        bit_depth,
        mts_index,
    );
}

#[cfg(test)]
pub(in crate::vvc) fn inverse_transform_vvc_luma_residual_levels(
    width: u16,
    height: u16,
    coeff_levels: &[i16],
    bit_depth: SampleBitDepth,
) -> Vec<i16> {
    let mut scratch = VvcInverseTransformScratch::default();
    let mut residuals = Vec::new();
    inverse_transform_vvc_residual_levels_into(
        &mut residuals,
        &mut scratch,
        width,
        height,
        coeff_levels,
        VVC_DEFAULT_LOSSY_LUMA_QP,
        bit_depth,
    );
    residuals
}

#[cfg(test)]
fn inverse_transform_vvc_residual_levels_into(
    residuals: &mut Vec<i16>,
    scratch: &mut VvcInverseTransformScratch,
    width: u16,
    height: u16,
    coeff_levels: &[i16],
    qp: i32,
    bit_depth: SampleBitDepth,
) {
    let width_usize = usize::from(width);
    let height_usize = usize::from(height);
    assert_eq!(coeff_levels.len(), width_usize * height_usize);
    debug_assert!([4, 8, 16, 32].contains(&width));
    debug_assert!([4, 8, 16, 32].contains(&height));

    let dequantized = &mut scratch.dequantized[..coeff_levels.len()];
    let dequant_params = vvc_transform_dequant_params(width, height, qp);
    for (dst, level) in dequantized.iter_mut().zip(coeff_levels.iter().copied()) {
        *dst = dequantize_vvc_transform_level_with_params(level, dequant_params);
    }
    inverse_transform_vvc_dequantized_levels_into(
        residuals,
        scratch,
        width,
        height,
        width_usize,
        height_usize,
        bit_depth,
        0,
    );
}

fn inverse_transform_vvc_dequantized_levels_into(
    residuals: &mut Vec<i16>,
    scratch: &mut VvcInverseTransformScratch,
    width: u16,
    height: u16,
    active_width: usize,
    active_height: usize,
    bit_depth: SampleBitDepth,
    mts_index: u8,
) {
    if mts_index == 0 {
        inverse_transform_vvc_dct2_dequantized_levels_into(
            residuals,
            scratch,
            width,
            height,
            active_width,
            active_height,
            bit_depth,
        );
        return;
    }

    let width_usize = usize::from(width);
    let height_usize = usize::from(height);
    let coefficient_count = width_usize * height_usize;
    let dequantized = &scratch.dequantized[..coefficient_count];
    let vertical = &mut scratch.vertical[..coefficient_count];
    let transform_pair = vvc_luma_mts_transform_pair(mts_index);
    debug_assert!(active_width <= width_usize);
    debug_assert!(active_height <= height_usize);
    for x in 0..active_width {
        for y in 0..height_usize {
            let mut sum = 0;
            for k in 0..active_height {
                let coeff = dequantized[k * width_usize + x];
                if coeff != 0 {
                    sum += vvc_luma_transform_value(transform_pair.vertical, height, k, y) * coeff;
                }
            }
            vertical[y * width_usize + x] = if height > 1 { (sum + 64) >> 7 } else { sum };
        }
    }

    let residual_bd_shift = if width > 1 && height > 1 {
        5 + 15 - i32::from(bit_depth.bits())
    } else {
        6 + 15 - i32::from(bit_depth.bits())
    };
    let residual_offset = 1 << (residual_bd_shift - 1);
    residuals.clear();
    residuals.resize(coefficient_count, 0);
    for y in 0..height_usize {
        for x in 0..width_usize {
            let mut sum = 0;
            for k in 0..active_width {
                let coeff = vertical[y * width_usize + k];
                if coeff != 0 {
                    sum += vvc_luma_transform_value(transform_pair.horizontal, width, k, x) * coeff;
                }
            }
            residuals[y * width_usize + x] = ((sum + residual_offset) >> residual_bd_shift) as i16;
        }
    }
}

fn inverse_transform_vvc_dct2_dequantized_levels_into(
    residuals: &mut Vec<i16>,
    scratch: &mut VvcInverseTransformScratch,
    width: u16,
    height: u16,
    active_width: usize,
    active_height: usize,
    bit_depth: SampleBitDepth,
) {
    let width_usize = usize::from(width);
    let height_usize = usize::from(height);
    let coefficient_count = width_usize * height_usize;
    let dequantized = &scratch.dequantized[..coefficient_count];
    let vertical = &mut scratch.vertical[..coefficient_count];
    debug_assert!(active_width <= width_usize);
    debug_assert!(active_height <= height_usize);
    for x in 0..active_width {
        let mut sums = [0i32; VVC_MAX_TRANSFORM_EDGE];
        for k in 0..active_height {
            let coeff = dequantized[k * width_usize + x];
            if coeff == 0 {
                continue;
            }
            let row = dct2_row(height, k);
            for y in 0..height_usize {
                sums[y] += row[y] * coeff;
            }
        }
        for y in 0..height_usize {
            vertical[y * width_usize + x] = if height > 1 {
                (sums[y] + 64) >> 7
            } else {
                sums[y]
            };
        }
    }

    let residual_bd_shift = if width > 1 && height > 1 {
        5 + 15 - i32::from(bit_depth.bits())
    } else {
        6 + 15 - i32::from(bit_depth.bits())
    };
    let residual_offset = 1 << (residual_bd_shift - 1);
    residuals.clear();
    residuals.resize(coefficient_count, 0);
    for y in 0..height_usize {
        let mut sums = [0i32; VVC_MAX_TRANSFORM_EDGE];
        for k in 0..active_width {
            let coeff = vertical[y * width_usize + k];
            if coeff == 0 {
                continue;
            }
            let row = dct2_row(width, k);
            for x in 0..width_usize {
                sums[x] += row[x] * coeff;
            }
        }
        for x in 0..width_usize {
            residuals[y * width_usize + x] =
                ((sums[x] + residual_offset) >> residual_bd_shift) as i16;
        }
    }
}
