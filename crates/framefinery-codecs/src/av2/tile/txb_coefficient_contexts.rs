fn chroma_nz_map_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    scan_index: usize,
    is_eob_coefficient: bool,
    plane: Av2ChromaPlane,
) -> usize {
    if is_eob_coefficient {
        return get_lower_levels_ctx_eob(scan_index);
    }
    if chroma_lf_limits(pos) {
        return chroma_lower_levels_lf_context(levels, pos, plane);
    }
    chroma_lower_levels_context(levels, pos, plane)
}

fn chroma_tx8x8_nz_map_context(
    levels: &[u32; TX8X8_SAMPLES],
    pos: usize,
    scan_index: usize,
    is_eob_coefficient: bool,
    plane: Av2ChromaPlane,
) -> usize {
    if is_eob_coefficient {
        return get_lower_levels_ctx_eob_for_txb(scan_index, TX8X8_SAMPLES);
    }
    if chroma_lf_limits(pos) {
        return chroma_tx8x8_lower_levels_lf_context(levels, pos, plane);
    }
    chroma_tx8x8_lower_levels_context(levels, pos, plane)
}

fn chroma_tx4x8_nz_map_context(
    levels: &[u32; TX4X8_SAMPLES],
    pos: usize,
    scan_index: usize,
    is_eob_coefficient: bool,
    plane: Av2ChromaPlane,
) -> usize {
    if is_eob_coefficient {
        return get_lower_levels_ctx_eob_for_txb(scan_index, TX4X8_SAMPLES);
    }
    if chroma_lf_limits(pos) {
        return chroma_tx4x8_lower_levels_lf_context(levels, pos, plane);
    }
    chroma_tx4x8_lower_levels_context(levels, pos, plane)
}

fn luma_nz_map_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    scan_index: usize,
    is_eob_coefficient: bool,
) -> usize {
    if is_eob_coefficient {
        return get_lower_levels_ctx_eob(scan_index);
    }
    if luma_lf_limits(pos) {
        return luma_lower_levels_lf_context(levels, pos);
    }
    luma_lower_levels_context(levels, pos)
}

fn get_lower_levels_ctx_eob(scan_index: usize) -> usize {
    get_lower_levels_ctx_eob_for_txb(scan_index, TX4X4_SAMPLES)
}

fn get_lower_levels_ctx_eob_for_txb(scan_index: usize, samples: usize) -> usize {
    if scan_index == 0 {
        0
    } else if scan_index <= samples / 8 {
        1
    } else if scan_index <= samples / 4 {
        2
    } else {
        3
    }
}

fn luma_lower_levels_lf_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    let mag = tx4x4_level_at(levels, pos, 0, 1).min(5)
        + tx4x4_level_at(levels, pos, 1, 0).min(5)
        + tx4x4_level_at(levels, pos, 1, 1).min(5)
        + tx4x4_level_at(levels, pos, 0, 2).min(5)
        + tx4x4_level_at(levels, pos, 2, 0).min(5);
    let row = pos / TX4X4_SIZE;
    let col = pos % TX4X4_SIZE;
    let ctx = (mag + 1) >> 1;
    if pos == 0 {
        return ctx.min(8) as usize;
    }
    if row + col < 2 {
        return ctx.min(6) as usize + 9;
    }
    ctx.min(4) as usize + 16
}

fn luma_lower_levels_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mag = tx4x4_level_at(levels, pos, 0, 1).min(3)
        + tx4x4_level_at(levels, pos, 1, 0).min(3)
        + tx4x4_level_at(levels, pos, 1, 1).min(3)
        + tx4x4_level_at(levels, pos, 0, 2).min(3)
        + tx4x4_level_at(levels, pos, 2, 0).min(3);
    let row = pos / TX4X4_SIZE;
    let col = pos % TX4X4_SIZE;
    let ctx = ((mag + 1) >> 1).min(4) as usize;
    if row + col < 6 {
        ctx
    } else if row + col < 8 {
        ctx + 5
    } else {
        ctx + 10
    }
}

fn chroma_lower_levels_lf_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    plane: Av2ChromaPlane,
) -> usize {
    let mag = tx4x4_level_at(levels, pos, 0, 1).min(5)
        + tx4x4_level_at(levels, pos, 1, 0).min(5)
        + tx4x4_level_at(levels, pos, 1, 1).min(5);
    let ctx = ((mag + 1) >> 1).min(3) as usize;
    chroma_context_with_plane_offset(ctx, plane)
}

fn chroma_lower_levels_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    plane: Av2ChromaPlane,
) -> usize {
    let mag = tx4x4_level_at(levels, pos, 0, 1).min(3)
        + tx4x4_level_at(levels, pos, 1, 0).min(3)
        + tx4x4_level_at(levels, pos, 1, 1).min(3);
    let ctx = ((mag + 1) >> 1).min(3) as usize;
    chroma_context_with_plane_offset(ctx, plane)
}

fn chroma_tx8x8_lower_levels_lf_context(
    levels: &[u32; TX8X8_SAMPLES],
    pos: usize,
    plane: Av2ChromaPlane,
) -> usize {
    let mag = tx8x8_level_at(levels, pos, 0, 1).min(5)
        + tx8x8_level_at(levels, pos, 1, 0).min(5)
        + tx8x8_level_at(levels, pos, 1, 1).min(5);
    let ctx = ((mag + 1) >> 1).min(3) as usize;
    chroma_context_with_plane_offset(ctx, plane)
}

fn chroma_tx8x8_lower_levels_context(
    levels: &[u32; TX8X8_SAMPLES],
    pos: usize,
    plane: Av2ChromaPlane,
) -> usize {
    let mag = tx8x8_level_at(levels, pos, 0, 1).min(3)
        + tx8x8_level_at(levels, pos, 1, 0).min(3)
        + tx8x8_level_at(levels, pos, 1, 1).min(3);
    let ctx = ((mag + 1) >> 1).min(3) as usize;
    chroma_context_with_plane_offset(ctx, plane)
}

fn chroma_tx4x8_lower_levels_lf_context(
    levels: &[u32; TX4X8_SAMPLES],
    pos: usize,
    plane: Av2ChromaPlane,
) -> usize {
    let mag = tx4x8_level_at(levels, pos, 0, 1).min(5)
        + tx4x8_level_at(levels, pos, 1, 0).min(5)
        + tx4x8_level_at(levels, pos, 1, 1).min(5);
    let ctx = ((mag + 1) >> 1).min(3) as usize;
    chroma_context_with_plane_offset(ctx, plane)
}

fn chroma_tx4x8_lower_levels_context(
    levels: &[u32; TX4X8_SAMPLES],
    pos: usize,
    plane: Av2ChromaPlane,
) -> usize {
    let mag = tx4x8_level_at(levels, pos, 0, 1).min(3)
        + tx4x8_level_at(levels, pos, 1, 0).min(3)
        + tx4x8_level_at(levels, pos, 1, 1).min(3);
    let ctx = ((mag + 1) >> 1).min(3) as usize;
    chroma_context_with_plane_offset(ctx, plane)
}

fn chroma_context_with_plane_offset(ctx: usize, plane: Av2ChromaPlane) -> usize {
    match plane {
        Av2ChromaPlane::U => ctx,
        Av2ChromaPlane::V => ctx + 4,
    }
}

fn chroma_br_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    let mag = tx4x4_level_at(levels, pos, 0, 1)
        + tx4x4_level_at(levels, pos, 1, 0)
        + tx4x4_level_at(levels, pos, 1, 1);
    ((mag + 1) >> 1).min(3) as usize
}

fn chroma_tx8x8_br_context(levels: &[u32; TX8X8_SAMPLES], pos: usize) -> usize {
    let mag = tx8x8_level_at(levels, pos, 0, 1)
        + tx8x8_level_at(levels, pos, 1, 0)
        + tx8x8_level_at(levels, pos, 1, 1);
    ((mag + 1) >> 1).min(3) as usize
}

fn chroma_tx4x8_br_context(levels: &[u32; TX4X8_SAMPLES], pos: usize) -> usize {
    let mag = tx4x8_level_at(levels, pos, 0, 1)
        + tx4x8_level_at(levels, pos, 1, 0)
        + tx4x8_level_at(levels, pos, 1, 1);
    ((mag + 1) >> 1).min(3) as usize
}

fn luma_br_lf_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    let mag = tx4x4_level_at(levels, pos, 0, 1).min(5)
        + tx4x4_level_at(levels, pos, 1, 0).min(5)
        + tx4x4_level_at(levels, pos, 1, 1).min(5);
    let mag = ((mag + 1) >> 1).min(6) as usize;
    if pos == 0 {
        mag
    } else {
        mag + 7
    }
}

fn luma_br_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    let mag = tx4x4_level_at(levels, pos, 0, 1).min(5)
        + tx4x4_level_at(levels, pos, 1, 0).min(5)
        + tx4x4_level_at(levels, pos, 1, 1).min(5);
    ((mag + 1) >> 1).min(6) as usize
}

include!("txb_idtx_contexts.rs");

#[inline(always)]
fn tx_level_at<const SAMPLES: usize, const WIDTH: usize, const HEIGHT: usize>(
    levels: &[u32; SAMPLES],
    pos: usize,
    row_delta: usize,
    col_delta: usize,
) -> u32 {
    debug_assert_eq!(SAMPLES, WIDTH * HEIGHT);
    let row = pos / WIDTH + row_delta;
    let col = pos % WIDTH + col_delta;
    if row < HEIGHT && col < WIDTH {
        levels[row * WIDTH + col].min(127)
    } else {
        0
    }
}

fn tx4x4_level_at(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    row_delta: usize,
    col_delta: usize,
) -> u32 {
    tx_level_at::<TX4X4_SAMPLES, TX4X4_SIZE, TX4X4_SIZE>(levels, pos, row_delta, col_delta)
}

fn tx8x8_level_at(
    levels: &[u32; TX8X8_SAMPLES],
    pos: usize,
    row_delta: usize,
    col_delta: usize,
) -> u32 {
    tx_level_at::<TX8X8_SAMPLES, TX8X8_SIZE, TX8X8_SIZE>(levels, pos, row_delta, col_delta)
}

fn tx4x8_level_at(
    levels: &[u32; TX4X8_SAMPLES],
    pos: usize,
    row_delta: usize,
    col_delta: usize,
) -> u32 {
    tx_level_at::<TX4X8_SAMPLES, TX4X8_WIDTH, TX4X8_HEIGHT>(levels, pos, row_delta, col_delta)
}

fn chroma_lf_limits(pos: usize) -> bool {
    // AV2's chroma LF coefficient set is the DC position for each transform
    // geometry currently emitted by this encoder.
    pos == 0
}

fn luma_lf_limits(pos: usize) -> bool {
    let row = pos / TX4X4_SIZE;
    let col = pos % TX4X4_SIZE;
    row + col < 4
}

fn lossless_entropy_context(cul_level: u32, dc_val: i32) -> u8 {
    let mut context = cul_level.min(7) as u8;
    if dc_val < 0 {
        context |= 1 << 3;
    } else if dc_val > 0 {
        context += 2 << 3;
    }
    context
}

fn lossless_dc_level_for_sample(sample: u8) -> (u16, bool) {
    let delta = i16::from(sample) - i16::from(LOSSLESS_DC_PREDICTOR);
    let level = delta.unsigned_abs() * 4;
    debug_assert!(level > 0);
    (level, delta < 0)
}

fn nonzero_dc_entropy_context(negative: bool) -> u8 {
    if negative {
        NONZERO_NEGATIVE_DC_ENTROPY_CONTEXT
    } else {
        NONZERO_POSITIVE_DC_ENTROPY_CONTEXT
    }
}

#[cfg(test)]
mod coefficient_context_geometry_tests {
    use super::*;

    include!("txb_coefficient_context_geometry_tests.rs");
}
