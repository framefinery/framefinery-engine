include!("txb_skip_syntax.rs");

fn write_eob_one_y(writer: &mut Av2EntropyWriter) {
    write_eob_y(writer, 1);
}

fn write_eob_one_uv(writer: &mut Av2EntropyWriter) {
    write_eob_uv(writer, 1);
}

fn write_y_dc_level(writer: &mut Av2EntropyWriter, level: u16) {
    let mut base_cdf = DEFAULT_COEFF_BASE_LF_EOB_Y_TX4X4_CTX0_CDF;
    let base_symbol = usize::from(level.min(5) - 1);
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.y.dc_base_lf_eob_ctx0",
        AV2_STATIC_CDF_COEFF_Y_DC_BASE_LF_EOB_CTX0,
        base_symbol,
        &mut base_cdf,
        5,
        false,
    );

    if level > 4 {
        let mut low_cdf = DEFAULT_COEFF_LPS_LF_CTX0_CDF;
        let low_symbol = usize::from((level - 1 - 4).min(3));
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.dc_low_range_lf_ctx0",
            AV2_STATIC_CDF_COEFF_Y_DC_LOW_RANGE_LF_CTX0,
            low_symbol,
            &mut low_cdf,
            4,
            false,
        );
    }
}

fn write_uv_dc_level(writer: &mut Av2EntropyWriter, level: u16) {
    let mut base_cdf = DEFAULT_COEFF_BASE_LF_EOB_UV_CTX0_CDF;
    let base_symbol = usize::from(level.min(5) - 1);
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.uv.dc_base_lf_eob_ctx0",
        AV2_STATIC_CDF_COEFF_UV_DC_BASE_LF_EOB_CTX0,
        base_symbol,
        &mut base_cdf,
        5,
        false,
    );
}

fn write_y_negative_dc_sign(writer: &mut Av2EntropyWriter, dc_sign_ctx: u8) {
    write_y_dc_sign(writer, true, dc_sign_ctx);
}

fn write_y_dc_sign(writer: &mut Av2EntropyWriter, negative: bool, dc_sign_ctx: u8) {
    let dc_sign_ctx = normalize_av2_context(dc_sign_ctx, 0, 2, 0, "AV2 luma DC sign");
    let (name, mut cdf) = match dc_sign_ctx {
        0 => (
            "tile.coeff.y.dc_sign_negative_ctx0",
            DEFAULT_DC_SIGN_Y_CTX0_CDF,
        ),
        1 => (
            "tile.coeff.y.dc_sign_negative_ctx1",
            DEFAULT_DC_SIGN_Y_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.dc_sign_negative_ctx2",
            DEFAULT_DC_SIGN_Y_CTX2_CDF,
        ),
        _ => (
            "tile.coeff.y.dc_sign_negative_ctx0",
            DEFAULT_DC_SIGN_Y_CTX0_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        AV2_STATIC_CDF_COEFF_Y_DC_SIGN_BASE + usize::from(dc_sign_ctx),
        usize::from(negative),
        &mut cdf,
        2,
        false,
    );
}

fn write_y_dc_high_range(writer: &mut Av2EntropyWriter, level: u16) {
    write_dc_only_high_range(
        writer,
        "tile.coeff.y.dc_high_range",
        Av2CoefficientCodingKind::LumaTransform,
        u32::from(level),
    );
}

fn write_uv_dc_high_range(writer: &mut Av2EntropyWriter, level: u16) {
    write_dc_only_high_range(
        writer,
        "tile.coeff.uv.dc_high_range",
        Av2CoefficientCodingKind::ChromaTransform,
        u32::from(level),
    );
}

fn write_adaptive_high_range_with_context(
    writer: &mut Av2EntropyWriter,
    name: &'static str,
    value: u32,
    context: u32,
) {
    // AV2 v1.0.0 high-range coefficient coding mirrors AVM
    // write_adaptive_hr(): derive Rice parameter m from hr_level_avg, then use
    // truncated Rice with Exp-Golomb order k=m+1 and cmax=min(m+4,6).
    let m = adaptive_high_range_rice_parameter(context);
    write_truncated_rice(writer, name, value, m, m + 1, (m + 4).min(6));
}

fn adaptive_high_range_rice_parameter(context: u32) -> u8 {
    if context < 4 {
        1
    } else if context < 8 {
        2
    } else if context < 16 {
        3
    } else if context < 32 {
        4
    } else if context < 64 {
        5
    } else {
        6
    }
}

fn write_truncated_rice(
    writer: &mut Av2EntropyWriter,
    name: &'static str,
    value: u32,
    m: u8,
    k: u8,
    cmax: u8,
) {
    let q = value >> m;
    if q >= u32::from(cmax) {
        writer.write_literal(name, 0, cmax);
        write_exp_golomb(writer, name, value - (u32::from(cmax) << m), k);
    } else {
        if q > 0 {
            writer.write_literal(name, 0, q as u8);
        }
        writer.write_literal_bit(name, true);
        if m > 0 {
            writer.write_literal(name, value & ((1u32 << m) - 1), m);
        }
    }
}

fn write_exp_golomb(writer: &mut Av2EntropyWriter, name: &'static str, value: u32, k: u8) {
    let x = value + (1u32 << k);
    let length = (u32::BITS - x.leading_zeros()) as u8;
    assert!(length > k, "AV2 Exp-Golomb length must exceed order");
    writer.write_literal(name, 0, length - 1 - k);
    writer.write_literal(name, x, length);
}

fn ceil_log2(value: u32) -> u32 {
    assert!(value > 0, "ceil_log2 expects a positive value");
    if value == 1 {
        0
    } else {
        u32::BITS - (value - 1).leading_zeros()
    }
}

fn luma_txb_skip_context(above: u8, left: u8) -> u8 {
    let top = (above & 7).min(4);
    let left = (left & 7).min(4);
    match (top, left) {
        (0, 0) => 1,
        (0, 1..=2) | (1..=2, 0) | (1, 1) => 2,
        (0, _) | (_, 0) | (1, 2..=3) | (2..=3, 1) | (2, 2) => 3,
        (1..=2, 4) | (4, 1..=2) | (2..=3, 3) | (3, 2..=3) => 4,
        _ => 5,
    }
}

fn chroma_txb_skip_base_context(above: u8, left: u8) -> u8 {
    u8::from(above != 0) + u8::from(left != 0)
}

fn v_txb_skip_context(above: u8, left: u8, last_u_txb_nonzero: bool) -> u8 {
    // AV2 v1.0.0 Section 5.20.7.23 read_tx_block(): AVM get_txb_ctx()
    // offsets V-plane TX_4X4 contexts by three when the 8x8 coding block is
    // larger than the transform block, then av2_read_sig_txtype() adds
    // V_TXB_SKIP_CONTEXT_OFFSET (6) if the retained U-plane EOB flag is set.
    chroma_txb_skip_base_context(above, left) + 3 + if last_u_txb_nonzero { 6 } else { 0 }
}

fn v_txb_skip_context_for_chroma_format(
    above: u8,
    left: u8,
    last_u_txb_nonzero: bool,
    chroma_format: Av2ChromaFormat,
    block_size: Av2MvpBlockSize,
) -> u8 {
    // AV2 v1.0.0 get_txb_ctx() adds half of V_TXB_SKIP_CONTEXT_OFFSET only
    // when the chroma coding block is larger than the TXB. 4:2:0 8x8 luma
    // leaves map to exactly one 4x4 chroma TXB, while larger lossless leaves
    // inherit the same +3 offset as 4:2:2/4:4:4.
    let chroma_block_width = block_size.width / chroma_subsample_x(chroma_format);
    let chroma_block_height = block_size.height / chroma_subsample_y(chroma_format);
    let block_larger_than_txb_offset =
        if chroma_block_width > TX4X4_SIZE || chroma_block_height > TX4X4_SIZE {
            3
        } else {
            0
        };
    chroma_txb_skip_base_context(above, left)
        + block_larger_than_txb_offset
        + if last_u_txb_nonzero { 6 } else { 0 }
}

fn dc_sign_context(above: u8, left: u8) -> u8 {
    let mut sign_sum = entropy_context_dc_sign(above) + entropy_context_dc_sign(left);
    sign_sum = sign_sum.clamp(-32, 32);
    match sign_sum {
        0 => 0,
        -32..=-1 => 1,
        1..=32 => 2,
        _ => unreachable!("AV2 DC sign sum was clamped before context lookup"),
    }
}

fn entropy_context_dc_sign(context: u8) -> i8 {
    match context >> 3 {
        0 => 0,
        1 => -1,
        2 => 1,
        _ => {
            debug_assert!(false, "unsupported AV2 DC sign entropy context {context}");
            0
        }
    }
}
