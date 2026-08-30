fn quantize_legacy_luma_ac_coeffs(
    residuals: &[i16],
    width: u16,
    height: u16,
    qp: i32,
) -> ([i16; VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let cell_sums = luma_hadamard_cell_sums(residuals, width);
    let (stored_width, _) = luma_coded_coefficient_extent(width, height);
    let mut ac_coeffs = [0; VVC_LUMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let quant_shift = luma_legacy_ac_quant_shift(qp);
    let level_limit = luma_ac_level_limit(qp);
    for ky in 0..usize::from(height).min(4) {
        for kx in 0..usize::from(width).min(4) {
            if kx == 0 && ky == 0 {
                continue;
            }
            let mut acc = 0i64;
            for cell_y in 0..4 {
                for cell_x in 0..4 {
                    acc += cell_sums[cell_y * 4 + cell_x]
                        * i64::from(luma_lossy_hadamard4_basis(kx, cell_x))
                        * i64::from(luma_lossy_hadamard4_basis(ky, cell_y));
                }
            }
            let level = div_round_nearest_i64(acc, 1i64 << quant_shift);
            let stored_idx = ky * stored_width + kx - 1;
            ac_coeffs[stored_idx] =
                level.clamp(i64::from(-level_limit), i64::from(level_limit)) as i16;
            has_ac |= ac_coeffs[stored_idx] != 0;
        }
    }
    (ac_coeffs, has_ac)
}

fn quantize_transform_luma_ac_coeffs(
    residuals: &[i16],
    width: u16,
    height: u16,
    qp: i32,
    mts_index: u8,
) -> ([i16; VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let width_usize = usize::from(width);
    let height_usize = usize::from(height);
    debug_assert_eq!(residuals.len(), width_usize * height_usize);
    let (active_width, active_height) = luma_coded_coefficient_extent(width, height);
    let transform_pair = vvc_luma_mts_transform_pair(mts_index);
    let mut ac_coeffs = [0; VVC_LUMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let mut vertical = [0i64; 8 * VVC_MAX_TRANSFORM_EDGE];
    let quant_shift = luma_ac_quant_shift(width, height, qp);
    let level_limit = luma_ac_level_limit(qp);
    for ky in 0..active_height {
        if transform_pair.vertical == VvcLumaTransformType::Dct2 {
            let row = dct2_row(height, ky);
            for x in 0..width_usize {
                let mut sum = 0i64;
                for y in 0..height_usize {
                    sum += i64::from(residuals[y * width_usize + x]) * i64::from(row[y]);
                }
                vertical[ky * VVC_MAX_TRANSFORM_EDGE + x] = sum;
            }
        } else {
            for x in 0..width_usize {
                let mut sum = 0i64;
                for y in 0..height_usize {
                    sum += i64::from(residuals[y * width_usize + x])
                        * i64::from(vvc_luma_transform_value(
                            transform_pair.vertical,
                            height,
                            ky,
                            y,
                        ));
                }
                vertical[ky * VVC_MAX_TRANSFORM_EDGE + x] = sum;
            }
        }
    }

    for ky in 0..active_height {
        for kx in 0..active_width {
            if kx == 0 && ky == 0 {
                continue;
            }
            let mut acc = 0i64;
            if transform_pair.horizontal == VvcLumaTransformType::Dct2 {
                let row = dct2_row(width, kx);
                for x in 0..width_usize {
                    acc += vertical[ky * VVC_MAX_TRANSFORM_EDGE + x] * i64::from(row[x]);
                }
            } else {
                for x in 0..width_usize {
                    acc += vertical[ky * VVC_MAX_TRANSFORM_EDGE + x]
                        * i64::from(vvc_luma_transform_value(
                            transform_pair.horizontal,
                            width,
                            kx,
                            x,
                        ));
                }
            }
            let level = div_round_nearest_i64(acc, 1i64 << quant_shift);
            ac_coeffs[ky * active_width + kx - 1] =
                level.clamp(i64::from(-level_limit), i64::from(level_limit)) as i16;
            has_ac |= ac_coeffs[ky * active_width + kx - 1] != 0;
        }
    }
    (ac_coeffs, has_ac)
}
