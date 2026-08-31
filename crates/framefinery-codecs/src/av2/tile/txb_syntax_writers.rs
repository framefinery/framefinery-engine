struct Av2TxbSkipSyntax {
    all_zero_name: &'static str,
    nonzero_name: &'static str,
    static_cdf_key: usize,
    cdf: [u16; 6],
}

impl Av2TxbSkipSyntax {
    fn write(self, writer: &mut Av2EntropyWriter, all_zero: bool) {
        let name = if all_zero {
            self.all_zero_name
        } else {
            self.nonzero_name
        };
        let mut cdf = self.cdf;
        writer.write_symbol_with_static_cdf_key(
            name,
            self.static_cdf_key,
            usize::from(all_zero),
            &mut cdf,
            2,
            false,
        );
    }
}

fn write_y_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_y_txb_skip(writer, skip_ctx, true);
}

fn write_y_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_y_txb_skip(writer, skip_ctx, false);
}

fn write_y_txb_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, all_zero: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 1, 5, 5, "AV2 luma TXB skip");
    let (all_zero_name, nonzero_name, cdf) = match skip_ctx {
        1 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx1",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx1",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx2",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx2",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx3",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx3",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx4",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx4",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx5",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX5_CDF,
        ),
        _ => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx5",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX5_CDF,
        ),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: y_txb_skip_static_cdf_key(skip_ctx),
        cdf,
    }
    .write(writer, all_zero);
}

fn write_y_inter_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_y_inter_txb_skip(writer, skip_ctx, true);
}

fn write_y_inter_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_y_inter_txb_skip(writer, skip_ctx, false);
}

fn write_y_inter_txb_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, all_zero: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 1, 5, 5, "AV2 inter luma TXB skip");
    let (all_zero_name, nonzero_name, cdf) = match skip_ctx {
        1 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx1",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx1",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx2",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx2",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx3",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx3",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx4",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx4",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx5",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX5_CDF,
        ),
        _ => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx5",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX5_CDF,
        ),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: y_inter_txb_skip_static_cdf_key(skip_ctx),
        cdf,
    }
    .write(writer, all_zero);
}

fn write_y_fsc_txb_all_zero(writer: &mut Av2EntropyWriter) {
    write_y_fsc_txb_skip(writer, true);
}

fn write_y_fsc_txb_nonzero(writer: &mut Av2EntropyWriter) {
    write_y_fsc_txb_skip(writer, false);
}

fn write_y_fsc_txb_skip(writer: &mut Av2EntropyWriter, all_zero: bool) {
    Av2TxbSkipSyntax {
        all_zero_name: "tile.coeff.y.txb_all_zero_fsc_tx4x4_ctx9",
        nonzero_name: "tile.coeff.y.txb_nonzero_fsc_tx4x4_ctx9",
        static_cdf_key: y_fsc_txb_skip_static_cdf_key(9),
        cdf: DEFAULT_TXB_SKIP_Y_FSC_TX4X4_CTX9_CDF,
    }
    .write(writer, all_zero);
}

fn write_u_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool) {
    write_u_txb_skip(writer, skip_ctx, use_fsc, true);
}

fn write_u_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool) {
    write_u_txb_skip(writer, skip_ctx, use_fsc, false);
}

fn write_u_txb_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool, all_zero: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 6, 8, 8, "AV2 U TXB skip");
    let (all_zero_name, nonzero_name, cdf) = match skip_ctx {
        6 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx6",
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx6",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX6_CDF,
        ),
        6 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx6",
            "tile.coeff.u.txb_nonzero_tx4x4_ctx6",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX6_CDF,
        ),
        7 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx7",
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx7",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX7_CDF,
        ),
        7 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx7",
            "tile.coeff.u.txb_nonzero_tx4x4_ctx7",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX7_CDF,
        ),
        8 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx8",
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX8_CDF,
        ),
        8 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx8",
            "tile.coeff.u.txb_nonzero_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX8_CDF,
        ),
        _ if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx8",
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX8_CDF,
        ),
        _ => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx8",
            "tile.coeff.u.txb_nonzero_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX8_CDF,
        ),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: u_txb_skip_static_cdf_key(skip_ctx, use_fsc),
        cdf,
    }
    .write(writer, all_zero);
}

fn write_v_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_v_txb_skip(writer, skip_ctx, true);
}

fn write_v_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_v_txb_skip(writer, skip_ctx, false);
}

fn write_v_txb_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, all_zero: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 0, 11, 11, "AV2 V TXB skip");
    let (all_zero_name, nonzero_name, cdf) = match skip_ctx {
        0 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx0",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx0",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX0_CDF,
        ),
        1 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx1",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx1",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx2",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx2",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx3",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx3",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx4",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx4",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx5",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx5",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX5_CDF,
        ),
        6 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx6",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx6",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX6_CDF,
        ),
        7 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx7",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx7",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX7_CDF,
        ),
        8 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx8",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx8",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX8_CDF,
        ),
        9 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx9",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx9",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX9_CDF,
        ),
        10 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx10",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx10",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX10_CDF,
        ),
        11 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx11",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx11",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX11_CDF,
        ),
        _ => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx11",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx11",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX11_CDF,
        ),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: v_txb_skip_static_cdf_key(skip_ctx),
        cdf,
    }
    .write(writer, all_zero);
}

fn write_u_txb_all_zero_tx8x8(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    use_inter_contexts: bool,
) {
    write_u_txb_skip_tx8x8(writer, skip_ctx, use_inter_contexts, true);
}

fn write_u_txb_nonzero_tx8x8(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    use_inter_contexts: bool,
) {
    write_u_txb_skip_tx8x8(writer, skip_ctx, use_inter_contexts, false);
}

fn write_u_txb_skip_tx8x8(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    use_inter_contexts: bool,
    all_zero: bool,
) {
    let skip_ctx = normalize_av2_context(skip_ctx, 6, 8, 8, "AV2 U TXB skip 8x8");
    let (all_zero_name, nonzero_name, cdf) = match (use_inter_contexts, skip_ctx) {
        (false, 6) => (
            "tile.coeff.u.txb_all_zero_tx8x8_ctx6",
            "tile.coeff.u.txb_nonzero_tx8x8_ctx6",
            DEFAULT_TXB_SKIP_U_TX8X8_CTX6_CDF,
        ),
        (false, 7) => (
            "tile.coeff.u.txb_all_zero_tx8x8_ctx7",
            "tile.coeff.u.txb_nonzero_tx8x8_ctx7",
            DEFAULT_TXB_SKIP_U_TX8X8_CTX7_CDF,
        ),
        (false, 8) => (
            "tile.coeff.u.txb_all_zero_tx8x8_ctx8",
            "tile.coeff.u.txb_nonzero_tx8x8_ctx8",
            DEFAULT_TXB_SKIP_U_TX8X8_CTX8_CDF,
        ),
        (true, 6) => (
            "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx6",
            "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx6",
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX6_CDF,
        ),
        (true, 7) => (
            "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx7",
            "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx7",
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX7_CDF,
        ),
        (true, 8) => (
            "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx8",
            "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx8",
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX8_CDF,
        ),
        _ => unreachable!("normalized AV2 U TXB skip 8x8 context is in range"),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: u_txb_skip_tx8x8_static_cdf_key(skip_ctx, use_inter_contexts),
        cdf,
    }
    .write(writer, all_zero);
}

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
    if level > 7 {
        write_adaptive_high_range(writer, "tile.coeff.y.dc_high_range", u32::from(level - 8));
    }
}

fn write_uv_dc_high_range(writer: &mut Av2EntropyWriter, level: u16) {
    if level > 4 {
        write_adaptive_high_range(writer, "tile.coeff.uv.dc_high_range", u32::from(level - 5));
    }
}

fn write_adaptive_high_range(writer: &mut Av2EntropyWriter, name: &'static str, value: u32) {
    // AVM write_adaptive_hr() starts every TXB with hr_level_avg=0; the
    // resulting Rice parameter is m=1, k=2, cmax=5 for this DC-only path.
    write_adaptive_high_range_with_context(writer, name, value, 0);
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

#[cfg(test)]
mod txb_skip_writer_tests {
    use super::*;

    fn assert_skip_pair(
        write_all_zero: impl FnOnce(&mut Av2EntropyWriter),
        write_nonzero: impl FnOnce(&mut Av2EntropyWriter),
        expected_all_zero_name: &str,
        expected_nonzero_name: &str,
    ) {
        let mut all_zero_writer = Av2EntropyWriter::with_cdf_updates(true);
        write_all_zero(&mut all_zero_writer);
        let all_zero = all_zero_writer.finish();
        assert_eq!(all_zero.fields.len(), 1);
        assert_eq!(all_zero.fields[0].name, expected_all_zero_name);
        assert_eq!(all_zero.fields[0].symbol, Some(1));

        let mut nonzero_writer = Av2EntropyWriter::with_cdf_updates(true);
        write_nonzero(&mut nonzero_writer);
        let nonzero = nonzero_writer.finish();
        assert_eq!(nonzero.fields.len(), 1);
        assert_eq!(nonzero.fields[0].name, expected_nonzero_name);
        assert_eq!(nonzero.fields[0].symbol, Some(0));
    }

    #[test]
    fn txb_skip_writer_pairs_preserve_all_context_names_and_symbols() {
        for skip_ctx in 1..=5 {
            assert_skip_pair(
                |writer| write_y_txb_all_zero(writer, skip_ctx),
                |writer| write_y_txb_nonzero(writer, skip_ctx),
                &format!("tile.coeff.y.txb_all_zero_tx4x4_ctx{skip_ctx}"),
                &format!("tile.coeff.y.txb_nonzero_tx4x4_ctx{skip_ctx}"),
            );
            assert_skip_pair(
                |writer| write_y_inter_txb_all_zero(writer, skip_ctx),
                |writer| write_y_inter_txb_nonzero(writer, skip_ctx),
                &format!("tile.coeff.y.inter_txb_all_zero_tx4x4_ctx{skip_ctx}"),
                &format!("tile.coeff.y.inter_txb_nonzero_tx4x4_ctx{skip_ctx}"),
            );
        }

        assert_skip_pair(
            write_y_fsc_txb_all_zero,
            write_y_fsc_txb_nonzero,
            "tile.coeff.y.txb_all_zero_fsc_tx4x4_ctx9",
            "tile.coeff.y.txb_nonzero_fsc_tx4x4_ctx9",
        );

        for skip_ctx in 6..=8 {
            for use_fsc in [false, true] {
                let fsc = if use_fsc { "_fsc" } else { "" };
                assert_skip_pair(
                    |writer| write_u_txb_all_zero(writer, skip_ctx, use_fsc),
                    |writer| write_u_txb_nonzero(writer, skip_ctx, use_fsc),
                    &format!("tile.coeff.u.txb_all_zero{fsc}_tx4x4_ctx{skip_ctx}"),
                    &format!("tile.coeff.u.txb_nonzero{fsc}_tx4x4_ctx{skip_ctx}"),
                );
            }
            for use_inter_contexts in [false, true] {
                let inter = if use_inter_contexts { "_inter" } else { "" };
                assert_skip_pair(
                    |writer| write_u_txb_all_zero_tx8x8(writer, skip_ctx, use_inter_contexts),
                    |writer| write_u_txb_nonzero_tx8x8(writer, skip_ctx, use_inter_contexts),
                    &format!("tile.coeff.u.txb_all_zero{inter}_tx8x8_ctx{skip_ctx}"),
                    &format!("tile.coeff.u.txb_nonzero{inter}_tx8x8_ctx{skip_ctx}"),
                );
            }
        }

        for skip_ctx in 0..=11 {
            assert_skip_pair(
                |writer| write_v_txb_all_zero(writer, skip_ctx),
                |writer| write_v_txb_nonzero(writer, skip_ctx),
                &format!("tile.coeff.v.txb_all_zero_tx4x4_ctx{skip_ctx}"),
                &format!("tile.coeff.v.txb_nonzero_tx4x4_ctx{skip_ctx}"),
            );
        }
    }
}
