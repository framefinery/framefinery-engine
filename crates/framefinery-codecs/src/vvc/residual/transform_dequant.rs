#[derive(Debug, Clone, Copy)]
struct VvcTransformDequantParams {
    level_scale: i32,
    bd_shift: u32,
    bd_offset: i32,
}

fn vvc_transform_dequant_params(
    tb_width: u16,
    tb_height: u16,
    qp: i32,
) -> VvcTransformDequantParams {
    debug_assert!((0..=63).contains(&qp));
    let log2_width = tb_width.ilog2() as i32;
    let log2_height = tb_height.ilog2() as i32;
    let log2_sum = log2_width + log2_height;
    let rect_non_ts = (log2_sum & 1) as usize;
    let level_scale = [[40, 45, 51, 57, 64, 72], [57, 64, 72, 80, 90, 102]];
    let level_scale = 16 * level_scale[rect_non_ts][(qp % 6) as usize] * (1 << (qp / 6));
    let bd_shift = (8 + rect_non_ts as i32 + (log2_sum / 2) + 10 - 15) as u32;
    let bd_offset = 1 << (bd_shift - 1);
    VvcTransformDequantParams {
        level_scale,
        bd_shift,
        bd_offset,
    }
}

fn dequantize_vvc_transform_level_with_params(
    level: i16,
    params: VvcTransformDequantParams,
) -> i32 {
    if level == 0 {
        return 0;
    }
    (i32::from(level) * params.level_scale + params.bd_offset) >> params.bd_shift
}

fn dc_only_residual_from_level_with_params(
    level: i16,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    dequant_params: VvcTransformDequantParams,
) -> i64 {
    if level == 0 {
        return 0;
    }
    let dequantized = dequantize_vvc_transform_level_with_params(level, dequant_params);
    let vertical = if height > 1 {
        (64 * dequantized + 64) >> 7
    } else {
        64 * dequantized
    };
    let residual_bd_shift = if width > 1 && height > 1 {
        5 + 15 - i32::from(bit_depth.bits())
    } else {
        6 + 15 - i32::from(bit_depth.bits())
    };
    let residual_offset = 1 << (residual_bd_shift - 1);
    i64::from((64 * vertical + residual_offset) >> residual_bd_shift)
}
