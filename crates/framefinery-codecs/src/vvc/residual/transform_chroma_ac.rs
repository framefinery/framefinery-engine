fn quantize_direct_chroma_ac_coeffs(
    residuals: &[i16],
    width: u16,
    height: u16,
    chroma_qp: i32,
) -> ([i16; VVC_CHROMA_AC_COEFFS_PER_TU], bool) {
    let width_usize = usize::from(width);
    let height_usize = usize::from(height);
    debug_assert_eq!(residuals.len(), width_usize * height_usize);
    let active_width = width_usize.min(8);
    let active_height = height_usize.min(8);
    let mut ac_coeffs = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let mut vertical = [0i64; 8 * VVC_MAX_TRANSFORM_EDGE];
    let quant_shift = chroma_ac_quant_shift(width, height, chroma_qp);
    let level_limit = chroma_ac_level_limit(chroma_qp);
    for ky in 0..active_height {
        let transform_row = dct2_row(height, ky);
        for x in 0..width_usize {
            let mut sum = 0i64;
            for y in 0..height_usize {
                sum += i64::from(residuals[y * width_usize + x]) * i64::from(transform_row[y]);
            }
            vertical[ky * VVC_MAX_TRANSFORM_EDGE + x] = sum;
        }
    }

    for ky in 0..active_height {
        for kx in 0..active_width {
            if kx == 0 && ky == 0 {
                continue;
            }
            let transform_row = dct2_row(width, kx);
            let mut acc = 0i64;
            for x in 0..width_usize {
                acc += vertical[ky * VVC_MAX_TRANSFORM_EDGE + x] * i64::from(transform_row[x]);
            }
            let level = div_round_nearest_i64(acc, 1i64 << quant_shift);
            let compact_idx = ky * active_width + kx - 1;
            ac_coeffs[compact_idx] =
                level.clamp(i64::from(-level_limit), i64::from(level_limit)) as i16;
            has_ac |= ac_coeffs[compact_idx] != 0;
        }
    }
    (ac_coeffs, has_ac)
}

fn chroma_ac_quant_shift(width: u16, height: u16, chroma_qp: i32) -> u32 {
    let log2_sum = width.ilog2() as i32 + height.ilog2() as i32;
    let base_shift = (VVC_CHROMA_AC_QUANT_SHIFT_FOR_8X8 + log2_sum - 6).max(0) as u32;
    qp_adjusted_quant_shift(base_shift, chroma_qp, VVC_DEFAULT_LOSSY_CHROMA_QP)
}

fn chroma_ac_level_limit(chroma_qp: i32) -> i16 {
    qp_adjusted_level_limit(
        VVC_CHROMA_AC_LEVEL_LIMIT,
        chroma_qp,
        VVC_DEFAULT_LOSSY_CHROMA_QP,
    )
}
