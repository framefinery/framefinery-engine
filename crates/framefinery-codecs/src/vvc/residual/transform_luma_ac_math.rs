fn luma_legacy_ac_quant_shift(qp: i32) -> u32 {
    qp_adjusted_quant_shift(8, qp, VVC_DEFAULT_LOSSY_LUMA_QP)
}

fn luma_coded_coefficient_extent(width: u16, height: u16) -> (usize, usize) {
    if width == 8 && height == 8 {
        (8, 8)
    } else {
        (usize::from(width).min(4), usize::from(height).min(4))
    }
}

fn luma_hadamard_cell_sums(residuals: &[i16], width: u16) -> [i64; 16] {
    let width_usize = usize::from(width);
    let height_usize = residuals.len() / width_usize;
    let cell_width = (width_usize / 4).max(1);
    let cell_height = (height_usize / 4).max(1);
    let mut cell_sums = [0i64; 16];

    for cell_y in 0..4 {
        for cell_x in 0..4 {
            let mut cell_sum = 0i64;
            let y_start = cell_y * cell_height;
            let y_end = ((cell_y + 1) * cell_height).min(height_usize);
            let x_start = cell_x * cell_width;
            let x_end = ((cell_x + 1) * cell_width).min(width_usize);
            for y in y_start..y_end {
                for x in x_start..x_end {
                    cell_sum += i64::from(residuals[y * width_usize + x]);
                }
            }
            cell_sums[cell_y * 4 + cell_x] = cell_sum;
        }
    }
    cell_sums
}

fn luma_lossy_hadamard4_basis(k: usize, n: usize) -> i32 {
    match k {
        0 => 1,
        1 => {
            if n < 2 {
                1
            } else {
                -1
            }
        }
        2 => {
            if n == 0 || n == 3 {
                1
            } else {
                -1
            }
        }
        3 => {
            if n == 0 || n == 2 {
                1
            } else {
                -1
            }
        }
        _ => 0,
    }
}

fn luma_ac_quant_shift(width: u16, height: u16, qp: i32) -> u32 {
    let log2_sum = width.ilog2() as i32 + height.ilog2() as i32;
    let base_shift = (VVC_LUMA_AC_QUANT_SHIFT_FOR_8X8 + log2_sum - 6).max(0) as u32;
    qp_adjusted_quant_shift(base_shift, qp, VVC_DEFAULT_LOSSY_LUMA_QP)
}

fn luma_ac_level_limit(qp: i32) -> i16 {
    qp_adjusted_level_limit(VVC_LUMA_AC_LEVEL_LIMIT, qp, VVC_DEFAULT_LOSSY_LUMA_QP)
}
