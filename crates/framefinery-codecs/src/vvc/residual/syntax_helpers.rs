fn vvc_residual_scan(width: usize, height: usize) -> &'static [VvcScanPosition] {
    if width == 8 && height == 8 {
        &VVC_GROUPED_8X8_SCAN
    } else {
        &VVC_FIRST_4X4_DIAG_SCAN
    }
}

fn template_abs_sum_level(abs_level: u16) -> u8 {
    abs_level.min(4 + (abs_level & 1)) as u8
}

fn last_sig_coeff_group_index(position: u8) -> u8 {
    match position {
        0..=3 => position,
        4..=5 => 4,
        6..=7 => 5,
        8..=11 => 6,
        12..=15 => 7,
        16..=23 => 8,
        24..=31 => 9,
        32..=47 => 10,
        48..=63 => 11,
        _ => {
            debug_assert!(
                false,
                "VVC last coefficient groups above 64 samples are not wired yet"
            );
            11
        }
    }
}

fn last_sig_coeff_group_min(group_idx: u8) -> u8 {
    match group_idx {
        0..=4 => group_idx,
        5 => 6,
        6 => 8,
        7 => 12,
        8 => 16,
        9 => 24,
        10 => 32,
        11 => 48,
        _ => {
            debug_assert!(
                false,
                "VVC last coefficient group minima above 64 samples are not wired yet"
            );
            48
        }
    }
}

fn luma_stored_coeff_stride(log2_tb_width: u8, log2_tb_height: u8) -> usize {
    if log2_tb_width == 3 && log2_tb_height == 3 {
        8
    } else {
        (1usize << log2_tb_width).min(4)
    }
}

fn chroma_stored_coeff_stride(log2_tb_width: u8, _log2_tb_height: u8) -> usize {
    (1usize << log2_tb_width).min(8)
}

fn regular_bin_limit(width: usize, height: usize) -> i32 {
    // H.266 7.3.11.11 residual_coding() initializes remBinsPass1 as
    // ((1 << (Log2ZoTbWidth + Log2ZoTbHeight)) * 7) >> 2. VTM expresses the
    // same value as (TbAreaAfterCoefZeroOut * MAX_TU_LEVEL_CTX_CODED_BIN_*) >>
    // 4; VTM 24.0 sets both luma and chroma constraints to 28.
    ((width * height * 28) >> 4) as i32
}

fn regular_level_bin_count(abs_level: u16) -> i32 {
    if abs_level > 1 {
        3
    } else {
        1
    }
}

fn append_sign_bit(sign_bits: &mut u32, sign_count: &mut u8, negative: bool) {
    if *sign_count != 0 {
        *sign_bits <<= 1;
    }
    *sign_bits |= u32::from(negative);
    *sign_count += 1;
}

fn disabled_dep_quant_state_transition(_state: u8, _abs_level: u16) -> u8 {
    // VTM passes stateTransTable = 0 when dependent quantization is disabled,
    // so the residual state remains zero for both regular and bypass passes.
    0
}

fn derive_rice_param_from_state(
    scan_pos: usize,
    state: &VvcResidualPass1State,
    scan: &[VvcScanPosition],
    base_level: i32,
) -> u8 {
    rice_param_from_template_abs_sum(
        rice_template_abs_sum_from_state(scan_pos, state, scan),
        base_level,
    )
}

fn rice_param_from_template_abs_sum(sum_abs: i32, base_level: i32) -> u8 {
    const GO_RICE_PARS_COEFF: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
        2, 3, 3, 3, 3,
    ];
    let clipped = (sum_abs - 5 * base_level).clamp(0, 31) as usize;
    GO_RICE_PARS_COEFF[clipped]
}

fn rice_template_abs_sum_from_state(
    scan_pos: usize,
    state: &VvcResidualPass1State,
    scan: &[VvcScanPosition],
) -> i32 {
    let pos = scan[scan_pos];
    let width = state.config.tb_width();
    let height = state.config.tb_height();
    let x = pos.x;
    let y = pos.y;
    let mut sum = 0i32;
    if x + 1 < width {
        sum += state_rice_abs_at(state, x + 1, y);
        if x + 2 < width {
            sum += state_rice_abs_at(state, x + 2, y);
        }
        if y + 1 < height {
            sum += state_rice_abs_at(state, x + 1, y + 1);
        }
    }
    if y + 1 < height {
        sum += state_rice_abs_at(state, x, y + 1);
        if y + 2 < height {
            sum += state_rice_abs_at(state, x, y + 2);
        }
    }
    sum
}

fn state_rice_abs_at(state: &VvcResidualPass1State, x: usize, y: usize) -> i32 {
    state.rice_abs_level_at(x as u8, y as u8) as i32
}

fn go_rice_zero_position(state: u8, rice_param: u8) -> u32 {
    u32::from(if state < 2 { 1u8 } else { 2u8 }) << rice_param
}
