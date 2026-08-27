const AV2_STATIC_CDF_TXB_SKIP_Y_BASE: usize = 0;
const AV2_STATIC_CDF_TXB_SKIP_Y_FSC_BASE: usize = 16;
const AV2_STATIC_CDF_TXB_SKIP_U_BASE: usize = 32;
const AV2_STATIC_CDF_TXB_SKIP_U_FSC_BASE: usize = 48;
const AV2_STATIC_CDF_TXB_SKIP_V_BASE: usize = 64;
const AV2_STATIC_CDF_EOB_Y: usize = 100;
const AV2_STATIC_CDF_EOB_UV: usize = 101;
const AV2_STATIC_CDF_EOB_EXTRA: usize = 102;
const AV2_STATIC_CDF_COEFF_Y_BASE_LF_EOB_BASE: usize = 110;
const AV2_STATIC_CDF_COEFF_Y_BASE_EOB_BASE: usize = 130;
const AV2_STATIC_CDF_COEFF_Y_BASE_LF_BASE: usize = 160;
const AV2_STATIC_CDF_COEFF_Y_BASE_BASE: usize = 190;
const AV2_STATIC_CDF_COEFF_Y_BR_LF_BASE: usize = 220;
const AV2_STATIC_CDF_COEFF_Y_BR_BASE: usize = 240;
const AV2_STATIC_CDF_COEFF_UV_BASE_LF_EOB_BASE: usize = 260;
const AV2_STATIC_CDF_COEFF_UV_BASE_EOB_BASE: usize = 280;
const AV2_STATIC_CDF_COEFF_UV_BASE_LF_BASE: usize = 300;
const AV2_STATIC_CDF_COEFF_UV_BASE_BASE: usize = 320;
const AV2_STATIC_CDF_COEFF_UV_BR_BASE: usize = 340;
const AV2_STATIC_CDF_COEFF_Y_DC_BASE_LF_EOB_CTX0: usize = 360;
const AV2_STATIC_CDF_COEFF_Y_DC_LOW_RANGE_LF_CTX0: usize = 361;
const AV2_STATIC_CDF_COEFF_UV_DC_BASE_LF_EOB_CTX0: usize = 362;
const AV2_STATIC_CDF_COEFF_Y_DC_SIGN_BASE: usize = 370;
const AV2_STATIC_CDF_TXB_SKIP_Y_INTER_BASE: usize = 584;
const AV2_STATIC_CDF_EOB_Y_INTER: usize = 600;
const AV2_STATIC_CDF_INTER_EXT_TX_DCT_IDTX_4X4_BASE: usize = 601;
const AV2_STATIC_CDF_TXB_SKIP_U_TX8X8_BASE: usize = 620;
const AV2_STATIC_CDF_TXB_SKIP_U_INTER_TX8X8_BASE: usize = 630;
const AV2_STATIC_CDF_EOB_UV_TX8X8: usize = 640;
const AV2_STATIC_CDF_EOB_UV_TX4X8: usize = 641;

fn y_txb_skip_static_cdf_key(skip_ctx: u8) -> usize {
    AV2_STATIC_CDF_TXB_SKIP_Y_BASE + usize::from(skip_ctx)
}

fn y_inter_txb_skip_static_cdf_key(skip_ctx: u8) -> usize {
    AV2_STATIC_CDF_TXB_SKIP_Y_INTER_BASE + usize::from(skip_ctx)
}

fn y_fsc_txb_skip_static_cdf_key(skip_ctx: u8) -> usize {
    AV2_STATIC_CDF_TXB_SKIP_Y_FSC_BASE + usize::from(skip_ctx)
}

fn u_txb_skip_static_cdf_key(skip_ctx: u8, use_fsc: bool) -> usize {
    let base = if use_fsc {
        AV2_STATIC_CDF_TXB_SKIP_U_FSC_BASE
    } else {
        AV2_STATIC_CDF_TXB_SKIP_U_BASE
    };
    base + usize::from(skip_ctx)
}

fn u_txb_skip_tx8x8_static_cdf_key(skip_ctx: u8, use_inter_contexts: bool) -> usize {
    let base = if use_inter_contexts {
        AV2_STATIC_CDF_TXB_SKIP_U_INTER_TX8X8_BASE
    } else {
        AV2_STATIC_CDF_TXB_SKIP_U_TX8X8_BASE
    };
    base + usize::from(skip_ctx)
}

fn v_txb_skip_static_cdf_key(skip_ctx: u8) -> usize {
    AV2_STATIC_CDF_TXB_SKIP_V_BASE + usize::from(skip_ctx)
}

fn normalize_av2_context(context: u8, min: u8, max: u8, fallback: u8, label: &str) -> u8 {
    if (min..=max).contains(&context) {
        context
    } else {
        debug_assert!(false, "unsupported {label} context {context}");
        fallback
    }
}

fn tx4x4_coefficients_from_residual(
    residual: &[i32; TX4X4_SAMPLES],
    use_fsc: bool,
) -> [i32; TX4X4_SAMPLES] {
    if use_fsc {
        idtx4x4_coefficients(residual)
    } else {
        av2_fwht4x4(residual)
    }
}

fn tx4x4_residual_is_zero(residual: &[i32; TX4X4_SAMPLES]) -> bool {
    residual.iter().all(|&sample| sample == 0)
}

fn av2_fwht4x4(input: &[i32; TX4X4_SAMPLES]) -> [i32; TX4X4_SAMPLES] {
    // AV2 v1.0.0 lossless TX_4X4 uses AVM av2_fwht4x4_c() before coefficient
    // coding. The final UNIT_QUANT_FACTOR multiply is preserved so coefficient
    // levels below divide by eight, matching qindex 0 dequantization.
    let mut output = [0i32; TX4X4_SAMPLES];
    for i in 0..TX4X4_SIZE {
        let mut a1 = input[i];
        let mut b1 = input[TX4X4_SIZE + i];
        let mut c1 = input[2 * TX4X4_SIZE + i];
        let mut d1 = input[3 * TX4X4_SIZE + i];

        a1 += b1;
        d1 -= c1;
        let e1 = (a1 - d1) >> 1;
        b1 = e1 - b1;
        c1 = e1 - c1;
        a1 -= c1;
        d1 += b1;

        output[i] = a1;
        output[TX4X4_SIZE + i] = c1;
        output[2 * TX4X4_SIZE + i] = d1;
        output[3 * TX4X4_SIZE + i] = b1;
    }

    let pass0 = output;
    for i in 0..TX4X4_SIZE {
        let mut a1 = pass0[i * TX4X4_SIZE];
        let mut b1 = pass0[i * TX4X4_SIZE + 1];
        let mut c1 = pass0[i * TX4X4_SIZE + 2];
        let mut d1 = pass0[i * TX4X4_SIZE + 3];

        a1 += b1;
        d1 -= c1;
        let e1 = (a1 - d1) >> 1;
        b1 = e1 - b1;
        c1 = e1 - c1;
        a1 -= c1;
        d1 += b1;

        output[i * TX4X4_SIZE] = a1 * 8;
        output[i * TX4X4_SIZE + 1] = c1 * 8;
        output[i * TX4X4_SIZE + 2] = d1 * 8;
        output[i * TX4X4_SIZE + 3] = b1 * 8;
    }
    output
}

fn av2_iwht4x4(coefficients: &[i32; TX4X4_SAMPLES]) -> [i32; TX4X4_SAMPLES] {
    // Mirrors AVM av2_highbd_iwht4x4_16_add_c(), excluding the final
    // predictor add and clipping step.
    let mut output = [0i32; TX4X4_SAMPLES];
    for i in 0..TX4X4_SIZE {
        let mut a1 = coefficients[i * TX4X4_SIZE] >> 3;
        let mut c1 = coefficients[i * TX4X4_SIZE + 1] >> 3;
        let mut d1 = coefficients[i * TX4X4_SIZE + 2] >> 3;
        let mut b1 = coefficients[i * TX4X4_SIZE + 3] >> 3;

        a1 += c1;
        d1 -= b1;
        let e1 = (a1 - d1) >> 1;
        b1 = e1 - b1;
        c1 = e1 - c1;
        a1 -= b1;
        d1 += c1;

        output[i * TX4X4_SIZE] = a1;
        output[i * TX4X4_SIZE + 1] = b1;
        output[i * TX4X4_SIZE + 2] = c1;
        output[i * TX4X4_SIZE + 3] = d1;
    }

    let pass0 = output;
    for i in 0..TX4X4_SIZE {
        let mut a1 = pass0[i];
        let mut c1 = pass0[TX4X4_SIZE + i];
        let mut d1 = pass0[2 * TX4X4_SIZE + i];
        let mut b1 = pass0[3 * TX4X4_SIZE + i];

        a1 += c1;
        d1 -= b1;
        let e1 = (a1 - d1) >> 1;
        b1 = e1 - b1;
        c1 = e1 - c1;
        a1 -= b1;
        d1 += c1;

        output[i] = a1;
        output[TX4X4_SIZE + i] = b1;
        output[2 * TX4X4_SIZE + i] = c1;
        output[3 * TX4X4_SIZE + i] = d1;
    }
    output
}

const AV2_QUANT_TABLE_BITS: u8 = 3;
const AV2_QUANT_FP_BITS: u8 = 4;
const AV2_DCT_CONST_BITS: u8 = 14;
const AV2_COSPI_4_64: i32 = 16069;
const AV2_COSPI_8_64: i32 = 15137;
const AV2_COSPI_12_64: i32 = 13623;
const AV2_COSPI_16_64: i32 = 11585;
const AV2_COSPI_20_64: i32 = 9102;
const AV2_COSPI_24_64: i32 = 6270;
const AV2_COSPI_28_64: i32 = 3196;
const AV2_NEW_SQRT2: i32 = 5793;
const AV2_NEW_INV_SQRT2: i32 = 2896;
const AV2_NEW_SQRT2_BITS: u8 = 12;
const AV2_DCT4_KERNEL: [[i32; TX4X4_SIZE]; TX4X4_SIZE] = [
    [64, 64, 64, 64],
    [83, 35, -35, -83],
    [64, -64, -64, 64],
    [35, -83, 83, -35],
];
const AV2_DCT8_KERNEL: [[i32; TX8X8_SIZE]; TX8X8_SIZE] = [
    [64, 64, 64, 64, 64, 64, 64, 64],
    [89, 75, 50, 18, -18, -50, -75, -89],
    [83, 35, -35, -83, -83, -35, 35, 83],
    [75, -18, -89, -50, 50, 89, 18, -75],
    [64, -64, -64, 64, 64, -64, -64, 64],
    [50, -89, 18, 75, -75, -18, 89, -50],
    [35, -83, 83, -35, -35, 83, -83, 35],
    [18, -50, 75, -89, 89, -75, 50, -18],
];
const AV2_QLOOKUP_QTX: [i32; 25] = [
    64, 40, 41, 43, 44, 45, 47, 48, 49, 51, 52, 54, 55, 57, 59, 60, 62, 64, 66,
    68, 70, 72, 74, 76, 78,
];

fn av2_qlookup_qtx(qindex: u16, bit_depth: SampleBitDepth) -> i32 {
    let max_qindex = match bit_depth.bits() {
        8 => 255,
        10 => 303,
        12 => 351,
        bits => unreachable!("unsupported AV2 bit depth {bits}"),
    };
    let qindex = i32::from(qindex).clamp(1, max_qindex);
    if qindex < 25 {
        AV2_QLOOKUP_QTX[qindex as usize]
    } else {
        AV2_QLOOKUP_QTX[((qindex - 1) % 24 + 1) as usize] << ((qindex - 1) / 24)
    }
}

fn av2_regular_dequant_qtx(qindex: u16, bit_depth: SampleBitDepth) -> [i32; 2] {
    let q = av2_qlookup_qtx(qindex, bit_depth);
    [q, q]
}

fn av2_regular_quantize_dct4x4(
    coefficients: &[i32; TX4X4_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> ([i32; TX4X4_SAMPLES], [i32; TX4X4_SAMPLES]) {
    let dequant = av2_regular_dequant_qtx(qindex, bit_depth);
    let mut qcoeff = [0i32; TX4X4_SAMPLES];
    for pos in 0..TX4X4_SAMPLES {
        qcoeff[pos] = av2_regular_quantize_coefficient(coefficients[pos], dequant, pos != 0);
    }
    let dqcoeff = av2_regular_dequantize_dct4x4(&qcoeff, qindex, bit_depth);
    (qcoeff, dqcoeff)
}

fn av2_regular_quantize_dct8x8(
    coefficients: &[i32; TX8X8_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> ([i32; TX8X8_SAMPLES], [i32; TX8X8_SAMPLES]) {
    let dequant = av2_regular_dequant_qtx(qindex, bit_depth);
    let mut qcoeff = [0i32; TX8X8_SAMPLES];
    for pos in 0..TX8X8_SAMPLES {
        qcoeff[pos] = av2_regular_quantize_coefficient(coefficients[pos], dequant, pos != 0);
    }
    let dqcoeff = av2_regular_dequantize_dct8x8(&qcoeff, qindex, bit_depth);
    (qcoeff, dqcoeff)
}

fn av2_regular_quantize_dct4x8(
    coefficients: &[i32; TX4X8_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> ([i32; TX4X8_SAMPLES], [i32; TX4X8_SAMPLES]) {
    let dequant = av2_regular_dequant_qtx(qindex, bit_depth);
    let mut qcoeff = [0i32; TX4X8_SAMPLES];
    for pos in 0..TX4X8_SAMPLES {
        qcoeff[pos] = av2_regular_quantize_coefficient(coefficients[pos], dequant, pos != 0);
    }
    let dqcoeff = av2_regular_dequantize_dct4x8(&qcoeff, qindex, bit_depth);
    (qcoeff, dqcoeff)
}

fn av2_regular_quantize_coefficient(
    coefficient: i32,
    dequant: [i32; 2],
    ac_coefficient: bool,
) -> i32 {
    let rc01 = usize::from(ac_coefficient);
    let quant_fp =
        (1i64 << (16 + AV2_QUANT_FP_BITS + AV2_QUANT_TABLE_BITS)) / i64::from(dequant[rc01]);
    let round_fp = (64 * dequant[rc01]) >> (7 + AV2_QUANT_TABLE_BITS);
    let shift = 16 + AV2_QUANT_FP_BITS;
    let sign = coefficient.signum();
    let abs_coeff = i64::from(coefficient.abs());
    if (abs_coeff << (1 + AV2_QUANT_TABLE_BITS)) < i64::from(dequant[rc01]) {
        return 0;
    }
    let abs_qcoeff = ((abs_coeff + i64::from(round_fp)) * quant_fp) >> shift;
    (abs_qcoeff as i32) * sign
}

fn av2_regular_dequantize_dct4x4(
    qcoeff: &[i32; TX4X4_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> [i32; TX4X4_SAMPLES] {
    let dequant = av2_regular_dequant_qtx(qindex, bit_depth);
    let mut dqcoeff = [0i32; TX4X4_SAMPLES];
    for (pos, (&level, dst)) in qcoeff.iter().zip(dqcoeff.iter_mut()).enumerate() {
        let rc01 = usize::from(pos != 0);
        let abs_dqcoeff = round_power_of_two_i64(
            i64::from(level.abs()) * i64::from(dequant[rc01]),
            AV2_QUANT_TABLE_BITS,
        ) as i32;
        *dst = if level < 0 { -abs_dqcoeff } else { abs_dqcoeff };
    }
    dqcoeff
}

fn av2_regular_dequantize_dct8x8(
    qcoeff: &[i32; TX8X8_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> [i32; TX8X8_SAMPLES] {
    let dequant = av2_regular_dequant_qtx(qindex, bit_depth);
    let mut dqcoeff = [0i32; TX8X8_SAMPLES];
    for (pos, (&level, dst)) in qcoeff.iter().zip(dqcoeff.iter_mut()).enumerate() {
        let rc01 = usize::from(pos != 0);
        let abs_dqcoeff = round_power_of_two_i64(
            i64::from(level.abs()) * i64::from(dequant[rc01]),
            AV2_QUANT_TABLE_BITS,
        ) as i32;
        *dst = if level < 0 { -abs_dqcoeff } else { abs_dqcoeff };
    }
    dqcoeff
}

fn av2_regular_dequantize_dct4x8(
    qcoeff: &[i32; TX4X8_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> [i32; TX4X8_SAMPLES] {
    let dequant = av2_regular_dequant_qtx(qindex, bit_depth);
    let mut dqcoeff = [0i32; TX4X8_SAMPLES];
    for (pos, (&level, dst)) in qcoeff.iter().zip(dqcoeff.iter_mut()).enumerate() {
        let rc01 = usize::from(pos != 0);
        let abs_dqcoeff = round_power_of_two_i64(
            i64::from(level.abs()) * i64::from(dequant[rc01]),
            AV2_QUANT_TABLE_BITS,
        ) as i32;
        *dst = if level < 0 { -abs_dqcoeff } else { abs_dqcoeff };
    }
    dqcoeff
}

fn av2_regular_quantized_level_coefficients(
    qcoeff: &[i32; TX4X4_SAMPLES],
) -> [i32; TX4X4_SAMPLES] {
    let mut coefficients = [0i32; TX4X4_SAMPLES];
    for (dst, &level) in coefficients.iter_mut().zip(qcoeff.iter()) {
        *dst = level * 8;
    }
    coefficients
}

fn av2_regular_quantized_level_coefficients_tx8x8(
    qcoeff: &[i32; TX8X8_SAMPLES],
) -> [i32; TX8X8_SAMPLES] {
    let mut coefficients = [0i32; TX8X8_SAMPLES];
    for (dst, &level) in coefficients.iter_mut().zip(qcoeff.iter()) {
        *dst = level * 8;
    }
    coefficients
}

fn av2_regular_quantized_level_coefficients_tx4x8(
    qcoeff: &[i32; TX4X8_SAMPLES],
) -> [i32; TX4X8_SAMPLES] {
    let mut coefficients = [0i32; TX4X8_SAMPLES];
    for (dst, &level) in coefficients.iter_mut().zip(qcoeff.iter()) {
        *dst = level * 8;
    }
    coefficients
}

fn av2_fdct4x4(input: &[i32; TX4X4_SAMPLES]) -> [i32; TX4X4_SAMPLES] {
    let mut intermediate = [0i32; TX4X4_SAMPLES];
    for col in 0..TX4X4_SIZE {
        let mut in_high = [
            input[col] * 16,
            input[TX4X4_SIZE + col] * 16,
            input[2 * TX4X4_SIZE + col] * 16,
            input[3 * TX4X4_SIZE + col] * 16,
        ];
        if col == 0 && in_high[0] != 0 {
            in_high[0] += 1;
        }
        fdct4x4_pass(&in_high, &mut intermediate[col * TX4X4_SIZE..][..TX4X4_SIZE]);
    }

    let mut output = [0i32; TX4X4_SAMPLES];
    for col in 0..TX4X4_SIZE {
        let in_high = [
            intermediate[col],
            intermediate[TX4X4_SIZE + col],
            intermediate[2 * TX4X4_SIZE + col],
            intermediate[3 * TX4X4_SIZE + col],
        ];
        fdct4x4_pass(&in_high, &mut output[col * TX4X4_SIZE..][..TX4X4_SIZE]);
    }

    for coefficient in &mut output {
        *coefficient = (*coefficient + 1) >> 2;
    }
    output
}

fn av2_fdct8x8(input: &[i32; TX8X8_SAMPLES]) -> [i32; TX8X8_SAMPLES] {
    // Mirrors AVM avm_highbd_fdct8x8_c() for the DCT_DCT/TX_8X8 path.
    let mut intermediate = [0i32; TX8X8_SAMPLES];
    for col in 0..TX8X8_SIZE {
        let src = [
            input[col] * 4,
            input[TX8X8_SIZE + col] * 4,
            input[2 * TX8X8_SIZE + col] * 4,
            input[3 * TX8X8_SIZE + col] * 4,
            input[4 * TX8X8_SIZE + col] * 4,
            input[5 * TX8X8_SIZE + col] * 4,
            input[6 * TX8X8_SIZE + col] * 4,
            input[7 * TX8X8_SIZE + col] * 4,
        ];
        fdct8x8_pass(&src, &mut intermediate[col * TX8X8_SIZE..][..TX8X8_SIZE]);
    }

    let mut output = [0i32; TX8X8_SAMPLES];
    for col in 0..TX8X8_SIZE {
        let src = [
            intermediate[col],
            intermediate[TX8X8_SIZE + col],
            intermediate[2 * TX8X8_SIZE + col],
            intermediate[3 * TX8X8_SIZE + col],
            intermediate[4 * TX8X8_SIZE + col],
            intermediate[5 * TX8X8_SIZE + col],
            intermediate[6 * TX8X8_SIZE + col],
            intermediate[7 * TX8X8_SIZE + col],
        ];
        fdct8x8_pass(&src, &mut output[col * TX8X8_SIZE..][..TX8X8_SIZE]);
    }

    for coefficient in &mut output {
        *coefficient /= 2;
    }
    output
}

fn av2_fdct4x8(input: &[i32; TX4X8_SAMPLES]) -> [i32; TX4X8_SAMPLES] {
    // Mirrors AVM's generic DCT_DCT/TX_4X8 transform:
    // vertical size-8 pass with fwd shift 1, horizontal size-4 pass with
    // fwd shift 11, then the rectangular sqrt2 normalization.
    let mut intermediate = [0i32; TX4X8_SAMPLES];
    for col in 0..TX4X8_WIDTH {
        let src = [
            input[col],
            input[TX4X8_WIDTH + col],
            input[2 * TX4X8_WIDTH + col],
            input[3 * TX4X8_WIDTH + col],
            input[4 * TX4X8_WIDTH + col],
            input[5 * TX4X8_WIDTH + col],
            input[6 * TX4X8_WIDTH + col],
            input[7 * TX4X8_WIDTH + col],
        ];
        let dst = fwd_dct8_shifted(&src, 1);
        for row in 0..TX4X8_HEIGHT {
            intermediate[col * TX4X8_HEIGHT + row] = dst[row];
        }
    }

    let mut output = [0i32; TX4X8_SAMPLES];
    for row in 0..TX4X8_HEIGHT {
        let src = [
            intermediate[row],
            intermediate[TX4X8_HEIGHT + row],
            intermediate[2 * TX4X8_HEIGHT + row],
            intermediate[3 * TX4X8_HEIGHT + row],
        ];
        let dst = fwd_dct4_shifted(&src, 11);
        for col in 0..TX4X8_WIDTH {
            output[row * TX4X8_WIDTH + col] = dst[col];
        }
    }

    for coefficient in &mut output {
        *coefficient = round_power_of_two_i64(
            i64::from(*coefficient) * i64::from(AV2_NEW_SQRT2),
            AV2_NEW_SQRT2_BITS,
        ) as i32;
    }
    output
}

fn fwd_dct4_shifted(input: &[i32; TX4X4_SIZE], shift: u8) -> [i32; TX4X4_SIZE] {
    let step0 = input[0] + input[3];
    let step1 = input[1] + input[2];
    let step2 = input[1] - input[2];
    let step3 = input[0] - input[3];
    let add = if shift > 0 { 1i64 << (shift - 1) } else { 0 };
    [
        ((i64::from(AV2_DCT4_KERNEL[0][0]) * i64::from(step0)
            + i64::from(AV2_DCT4_KERNEL[0][1]) * i64::from(step1)
            + add)
            >> shift) as i32,
        ((i64::from(AV2_DCT4_KERNEL[1][0]) * i64::from(step3)
            + i64::from(AV2_DCT4_KERNEL[1][1]) * i64::from(step2)
            + add)
            >> shift) as i32,
        ((i64::from(AV2_DCT4_KERNEL[2][0]) * i64::from(step0)
            + i64::from(AV2_DCT4_KERNEL[2][1]) * i64::from(step1)
            + add)
            >> shift) as i32,
        ((i64::from(AV2_DCT4_KERNEL[3][0]) * i64::from(step3)
            + i64::from(AV2_DCT4_KERNEL[3][1]) * i64::from(step2)
            + add)
            >> shift) as i32,
    ]
}

fn fwd_dct8_shifted(input: &[i32; TX8X8_SIZE], shift: u8) -> [i32; TX8X8_SIZE] {
    let mut a = [0i32; 4];
    let mut b = [0i32; 4];
    for k in 0..4 {
        a[k] = input[k] + input[TX8X8_SIZE - 1 - k];
        b[k] = input[k] - input[TX8X8_SIZE - 1 - k];
    }
    let c0 = a[0] + a[3];
    let d0 = a[0] - a[3];
    let c1 = a[1] + a[2];
    let d1 = a[1] - a[2];
    let add = if shift > 0 { 1i64 << (shift - 1) } else { 0 };
    let mut output = [0i32; TX8X8_SIZE];
    output[0] = shifted_kernel_sum(&AV2_DCT8_KERNEL[0][..2], &[c0, c1], add, shift);
    output[4] = shifted_kernel_sum(&AV2_DCT8_KERNEL[4][..2], &[c0, c1], add, shift);
    output[2] = shifted_kernel_sum(&AV2_DCT8_KERNEL[2][..2], &[d0, d1], add, shift);
    output[6] = shifted_kernel_sum(&AV2_DCT8_KERNEL[6][..2], &[d0, d1], add, shift);
    for &index in &[1usize, 3, 5, 7] {
        output[index] = shifted_kernel_sum(&AV2_DCT8_KERNEL[index][..4], &b, add, shift);
    }
    output
}

fn shifted_kernel_sum(kernel: &[i32], values: &[i32], add: i64, shift: u8) -> i32 {
    let mut sum = add;
    for (&kernel, &value) in kernel.iter().zip(values.iter()) {
        sum += i64::from(kernel) * i64::from(value);
    }
    (sum >> shift) as i32
}

fn fdct8x8_pass(input: &[i32; TX8X8_SIZE], output: &mut [i32]) {
    let s0 = input[0] + input[7];
    let s1 = input[1] + input[6];
    let s2 = input[2] + input[5];
    let s3 = input[3] + input[4];
    let s4 = input[3] - input[4];
    let s5 = input[2] - input[5];
    let s6 = input[1] - input[6];
    let s7 = input[0] - input[7];

    let x0 = s0 + s3;
    let x1 = s1 + s2;
    let x2 = s1 - s2;
    let x3 = s0 - s3;
    output[0] = fdct_round_shift(i64::from(x0 + x1) * i64::from(AV2_COSPI_16_64));
    output[2] = fdct_round_shift(
        i64::from(x2) * i64::from(AV2_COSPI_24_64)
            + i64::from(x3) * i64::from(AV2_COSPI_8_64),
    );
    output[4] = fdct_round_shift(i64::from(x0 - x1) * i64::from(AV2_COSPI_16_64));
    output[6] = fdct_round_shift(
        -i64::from(x2) * i64::from(AV2_COSPI_8_64)
            + i64::from(x3) * i64::from(AV2_COSPI_24_64),
    );

    let t0 = fdct_round_shift(i64::from(s6 - s5) * i64::from(AV2_COSPI_16_64));
    let t1 = fdct_round_shift(i64::from(s6 + s5) * i64::from(AV2_COSPI_16_64));
    let x0 = s4 + t0;
    let x1 = s4 - t0;
    let x2 = s7 - t1;
    let x3 = s7 + t1;
    output[1] = fdct_round_shift(
        i64::from(x0) * i64::from(AV2_COSPI_28_64)
            + i64::from(x3) * i64::from(AV2_COSPI_4_64),
    );
    output[3] = fdct_round_shift(
        i64::from(x2) * i64::from(AV2_COSPI_12_64)
            - i64::from(x1) * i64::from(AV2_COSPI_20_64),
    );
    output[5] = fdct_round_shift(
        i64::from(x1) * i64::from(AV2_COSPI_12_64)
            + i64::from(x2) * i64::from(AV2_COSPI_20_64),
    );
    output[7] = fdct_round_shift(
        i64::from(x3) * i64::from(AV2_COSPI_28_64)
            - i64::from(x0) * i64::from(AV2_COSPI_4_64),
    );
}

fn fdct4x4_pass(input: &[i32; TX4X4_SIZE], output: &mut [i32]) {
    let step0 = input[0] + input[3];
    let step1 = input[1] + input[2];
    let step2 = input[1] - input[2];
    let step3 = input[0] - input[3];

    output[0] = fdct_round_shift(i64::from(step0 + step1) * i64::from(AV2_COSPI_16_64));
    output[2] = fdct_round_shift(i64::from(step0 - step1) * i64::from(AV2_COSPI_16_64));
    output[1] = fdct_round_shift(
        i64::from(step2) * i64::from(AV2_COSPI_24_64)
            + i64::from(step3) * i64::from(AV2_COSPI_8_64),
    );
    output[3] = fdct_round_shift(
        -i64::from(step2) * i64::from(AV2_COSPI_8_64)
            + i64::from(step3) * i64::from(AV2_COSPI_24_64),
    );
}

fn fdct_round_shift(value: i64) -> i32 {
    round_power_of_two_i64(value, AV2_DCT_CONST_BITS) as i32
}

fn av2_idct4x4(input: &[i32; TX4X4_SAMPLES], bit_depth: SampleBitDepth) -> [i32; TX4X4_SAMPLES] {
    let intermediate_bitdepth = i32::from(bit_depth.bits()) + 8;
    let rng_min = -(1 << (intermediate_bitdepth - 1));
    let rng_max = (1 << (intermediate_bitdepth - 1)) - 1;
    let col_rng_min = -(1 << bit_depth.bits());
    let col_rng_max = (1 << bit_depth.bits()) - 1;

    let mut block = *input;
    for coeff in &mut block {
        *coeff = (*coeff).clamp(rng_min, rng_max);
    }

    let tmp = inv_dct4_pass(&block, 7, rng_min, rng_max);
    let block = inv_dct4_pass(&tmp, 10, col_rng_min, col_rng_max);
    block
}

fn av2_idct8x8(input: &[i32; TX8X8_SAMPLES], bit_depth: SampleBitDepth) -> [i32; TX8X8_SAMPLES] {
    let intermediate_bitdepth = i32::from(bit_depth.bits()) + 8;
    let rng_min = -(1 << (intermediate_bitdepth - 1));
    let rng_max = (1 << (intermediate_bitdepth - 1)) - 1;
    let col_rng_min = -(1 << bit_depth.bits());
    let col_rng_max = (1 << bit_depth.bits()) - 1;

    let mut block = *input;
    for coeff in &mut block {
        *coeff = (*coeff).clamp(rng_min, rng_max);
    }

    let tmp = inv_dct8_pass(&block, 7, TX8X8_SIZE, rng_min, rng_max);
    inv_dct8_pass(&tmp, 11, TX8X8_SIZE, col_rng_min, col_rng_max)
}

fn av2_idct4x8(input: &[i32; TX4X8_SAMPLES], bit_depth: SampleBitDepth) -> [i32; TX4X8_SAMPLES] {
    let intermediate_bitdepth = i32::from(bit_depth.bits()) + 8;
    let rng_min = -(1 << (intermediate_bitdepth - 1));
    let rng_max = (1 << (intermediate_bitdepth - 1)) - 1;
    let col_rng_min = -(1 << bit_depth.bits());
    let col_rng_max = (1 << bit_depth.bits()) - 1;

    let mut block = *input;
    for coeff in &mut block {
        *coeff = round_power_of_two_i64(
            i64::from(*coeff) * i64::from(AV2_NEW_INV_SQRT2),
            AV2_NEW_SQRT2_BITS,
        ) as i32;
        *coeff = (*coeff).clamp(rng_min, rng_max);
    }

    let mut intermediate = [0i32; TX4X8_SAMPLES];
    for row in 0..TX4X8_HEIGHT {
        let src = [
            block[row * TX4X8_WIDTH],
            block[row * TX4X8_WIDTH + 1],
            block[row * TX4X8_WIDTH + 2],
            block[row * TX4X8_WIDTH + 3],
        ];
        let dst = inv_dct4_shifted(&src, 7, rng_min, rng_max);
        for col in 0..TX4X8_WIDTH {
            intermediate[col * TX4X8_HEIGHT + row] = dst[col];
        }
    }

    let mut output = [0i32; TX4X8_SAMPLES];
    for col in 0..TX4X8_WIDTH {
        let src = [
            intermediate[col * TX4X8_HEIGHT],
            intermediate[col * TX4X8_HEIGHT + 1],
            intermediate[col * TX4X8_HEIGHT + 2],
            intermediate[col * TX4X8_HEIGHT + 3],
            intermediate[col * TX4X8_HEIGHT + 4],
            intermediate[col * TX4X8_HEIGHT + 5],
            intermediate[col * TX4X8_HEIGHT + 6],
            intermediate[col * TX4X8_HEIGHT + 7],
        ];
        let dst = inv_dct8_shifted(&src, 10, col_rng_min, col_rng_max);
        for row in 0..TX4X8_HEIGHT {
            output[row * TX4X8_WIDTH + col] = dst[row];
        }
    }
    output
}

fn inv_dct4_shifted(
    input: &[i32; TX4X4_SIZE],
    shift: u8,
    min: i32,
    max: i32,
) -> [i32; TX4X4_SIZE] {
    let add = 1 << (shift - 1);
    let b0 = AV2_DCT4_KERNEL[1][0] * input[1] + AV2_DCT4_KERNEL[3][0] * input[3];
    let b1 = AV2_DCT4_KERNEL[1][1] * input[1] + AV2_DCT4_KERNEL[3][1] * input[3];
    let a0 = AV2_DCT4_KERNEL[0][0] * input[0] + AV2_DCT4_KERNEL[2][0] * input[2];
    let a1 = AV2_DCT4_KERNEL[0][1] * input[0] + AV2_DCT4_KERNEL[2][1] * input[2];
    [
        ((a0 + b0 + add) >> shift).clamp(min, max),
        ((a1 + b1 + add) >> shift).clamp(min, max),
        ((a1 - b1 + add) >> shift).clamp(min, max),
        ((a0 - b0 + add) >> shift).clamp(min, max),
    ]
}

fn inv_dct8_shifted(
    input: &[i32; TX8X8_SIZE],
    shift: u8,
    min: i32,
    max: i32,
) -> [i32; TX8X8_SIZE] {
    let add = 1 << (shift - 1);
    let mut b = [0i32; 4];
    for k in 0..4 {
        b[k] = AV2_DCT8_KERNEL[1][k] * input[1]
            + AV2_DCT8_KERNEL[3][k] * input[3]
            + AV2_DCT8_KERNEL[5][k] * input[5]
            + AV2_DCT8_KERNEL[7][k] * input[7];
    }

    let d0 = AV2_DCT8_KERNEL[2][0] * input[2] + AV2_DCT8_KERNEL[6][0] * input[6];
    let d1 = AV2_DCT8_KERNEL[2][1] * input[2] + AV2_DCT8_KERNEL[6][1] * input[6];
    let c0 = AV2_DCT8_KERNEL[0][0] * input[0] + AV2_DCT8_KERNEL[4][0] * input[4];
    let c1 = AV2_DCT8_KERNEL[0][1] * input[0] + AV2_DCT8_KERNEL[4][1] * input[4];

    let a = [c0 + d0, c1 + d1, c1 - d1, c0 - d0];
    let mut output = [0i32; TX8X8_SIZE];
    for k in 0..4 {
        output[k] = ((a[k] + b[k] + add) >> shift).clamp(min, max);
        output[k + 4] = ((a[3 - k] - b[3 - k] + add) >> shift).clamp(min, max);
    }
    output
}

fn inv_dct8_pass(
    input: &[i32; TX8X8_SAMPLES],
    shift: u8,
    line: usize,
    min: i32,
    max: i32,
) -> [i32; TX8X8_SAMPLES] {
    let mut output = [0i32; TX8X8_SAMPLES];
    let add = 1 << (shift - 1);
    for j in 0..TX8X8_SIZE {
        let src = j * TX8X8_SIZE;
        let mut b = [0i32; 4];
        for k in 0..4 {
            b[k] = AV2_DCT8_KERNEL[1][k] * input[src + 1]
                + AV2_DCT8_KERNEL[3][k] * input[src + 3]
                + AV2_DCT8_KERNEL[5][k] * input[src + 5]
                + AV2_DCT8_KERNEL[7][k] * input[src + 7];
        }

        let d0 = AV2_DCT8_KERNEL[2][0] * input[src + 2] + AV2_DCT8_KERNEL[6][0] * input[src + 6];
        let d1 = AV2_DCT8_KERNEL[2][1] * input[src + 2] + AV2_DCT8_KERNEL[6][1] * input[src + 6];
        let c0 = AV2_DCT8_KERNEL[0][0] * input[src] + AV2_DCT8_KERNEL[4][0] * input[src + 4];
        let c1 = AV2_DCT8_KERNEL[0][1] * input[src] + AV2_DCT8_KERNEL[4][1] * input[src + 4];

        let a = [c0 + d0, c1 + d1, c1 - d1, c0 - d0];
        for k in 0..4 {
            output[k * line + j] = ((a[k] + b[k] + add) >> shift).clamp(min, max);
            output[(k + 4) * line + j] =
                ((a[3 - k] - b[3 - k] + add) >> shift).clamp(min, max);
        }
    }
    output
}

fn inv_dct4_pass(input: &[i32; TX4X4_SAMPLES], shift: u8, min: i32, max: i32) -> [i32; TX4X4_SAMPLES] {
    let mut output = [0i32; TX4X4_SAMPLES];
    let add = 1 << (shift - 1);
    for j in 0..TX4X4_SIZE {
        let src = j * TX4X4_SIZE;
        let b0 = AV2_DCT4_KERNEL[1][0] * input[src + 1]
            + AV2_DCT4_KERNEL[3][0] * input[src + 3];
        let b1 = AV2_DCT4_KERNEL[1][1] * input[src + 1]
            + AV2_DCT4_KERNEL[3][1] * input[src + 3];
        let a0 = AV2_DCT4_KERNEL[0][0] * input[src]
            + AV2_DCT4_KERNEL[2][0] * input[src + 2];
        let a1 = AV2_DCT4_KERNEL[0][1] * input[src]
            + AV2_DCT4_KERNEL[2][1] * input[src + 2];
        output[j] = ((a0 + b0 + add) >> shift).clamp(min, max);
        output[TX4X4_SIZE + j] = ((a1 + b1 + add) >> shift).clamp(min, max);
        output[2 * TX4X4_SIZE + j] = ((a1 - b1 + add) >> shift).clamp(min, max);
        output[3 * TX4X4_SIZE + j] = ((a0 - b0 + add) >> shift).clamp(min, max);
    }
    output
}

fn round_power_of_two_i64(value: i64, bits: u8) -> i64 {
    debug_assert!(bits > 0);
    (value + (1i64 << (bits - 1))) >> bits
}

fn write_luma_palette_residual_txb(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    dc_sign_ctx: u8,
    coefficients: &[i32; TX4X4_SAMPLES],
) -> (u8, bool) {
    let (levels, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    let Some((_, eob)) = bounds else {
        write_y_txb_all_zero(writer, skip_ctx);
        return (0, false);
    };

    write_y_txb_nonzero(writer, skip_ctx);
    write_eob_y(writer, eob);

    for scan_index in (1..eob).rev() {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        let coeff_ctx = luma_nz_map_context(&levels, pos, scan_index, scan_index + 1 == eob);
        write_luma_coefficient_level(
            writer,
            &levels,
            pos,
            scan_index + 1 == eob,
            coeff_ctx,
            level,
        );
    }

    let dc_level = levels[0];
    let dc_ctx = luma_nz_map_context(&levels, 0, 0, eob == 1);
    write_luma_coefficient_level(writer, &levels, 0, eob == 1, dc_ctx, dc_level);

    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in (0..eob).rev() {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        if scan_index == 0 {
            write_y_dc_sign(writer, negative, dc_sign_ctx);
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        } else {
            writer.write_literal_bit("tile.coeff.y.ac_sign_negative", negative);
        }
        write_luma_high_range(writer, pos, level, &mut hr_level_avg);
        cul_level += level;
    }

    (lossless_entropy_context(cul_level, dc_val), true)
}

fn write_luma_inter_residual_txb(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    dc_sign_ctx: u8,
    coefficients: &[i32; TX4X4_SAMPLES],
) -> (u8, bool) {
    let (levels, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    let Some((_, eob)) = bounds else {
        write_y_inter_txb_all_zero(writer, skip_ctx);
        return (0, false);
    };

    write_y_inter_txb_nonzero(writer, skip_ctx);
    write_eob_y_inter(writer, eob);
    write_regular_inter_dct_dct_tx_type(writer, eob);

    for scan_index in (1..eob).rev() {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        let coeff_ctx = luma_nz_map_context(&levels, pos, scan_index, scan_index + 1 == eob);
        write_luma_coefficient_level(
            writer,
            &levels,
            pos,
            scan_index + 1 == eob,
            coeff_ctx,
            level,
        );
    }

    let dc_level = levels[0];
    let dc_ctx = luma_nz_map_context(&levels, 0, 0, eob == 1);
    write_luma_coefficient_level(writer, &levels, 0, eob == 1, dc_ctx, dc_level);

    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in (0..eob).rev() {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        if scan_index == 0 {
            write_y_dc_sign(writer, negative, dc_sign_ctx);
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        } else {
            writer.write_literal_bit("tile.coeff.y.ac_sign_negative", negative);
        }
        write_luma_high_range(writer, pos, level, &mut hr_level_avg);
        cul_level += level;
    }

    (lossless_entropy_context(cul_level, dc_val), true)
}

fn write_luma_palette_fsc_txb(
    writer: &mut Av2EntropyWriter,
    coefficients: &[i32; TX4X4_SAMPLES],
) -> (u8, bool) {
    let (levels, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    let Some((bob, _)) = bounds else {
        write_y_fsc_txb_all_zero(writer);
        return (0, false);
    };

    write_y_fsc_txb_nonzero(writer);
    write_eob_y(writer, TX4X4_SAMPLES - bob);

    for scan_index in bob..TX4X4_SAMPLES {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        if scan_index == bob {
            let coeff_ctx = idtx_bob_context(scan_index);
            let mut cdf = DEFAULT_COEFF_BASE_BOB_IDTX_CDFS[coeff_ctx];
            writer.write_symbol(
                "tile.coeff.y.idtx_base_bob",
                level.min(3) as usize - 1,
                &mut cdf,
                3,
                false,
            );
        } else {
            let coeff_ctx = idtx_upper_levels_context(&levels, pos);
            let mut cdf = DEFAULT_COEFF_BASE_IDTX_CDFS[coeff_ctx];
            writer.write_symbol(
                "tile.coeff.y.idtx_base",
                level.min(3) as usize,
                &mut cdf,
                4,
                false,
            );
        }
        if level > 2 {
            write_idtx_low_range(writer, &levels, pos, level);
        }
    }

    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in 0..TX4X4_SAMPLES {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        let sign_ctx = idtx_sign_context(&levels, coefficients, pos);
        let mut cdf = DEFAULT_IDTX_SIGN_CDFS[sign_ctx];
        writer.write_symbol(
            "tile.coeff.y.idtx_sign_negative",
            usize::from(negative),
            &mut cdf,
            2,
            false,
        );
        write_idtx_high_range(writer, level, &mut hr_level_avg);
        if scan_index == 0 {
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        }
        cul_level += level;
    }

    (lossless_entropy_context(cul_level, dc_val), true)
}

fn write_chroma_bdpcm_txb(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    skip_ctx: u8,
    coefficients: &[i32; TX4X4_SAMPLES],
    use_fsc: bool,
) -> (u8, bool) {
    let (levels, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    let Some((_, eob)) = bounds else {
        match plane {
            Av2ChromaPlane::U => write_u_txb_all_zero(writer, skip_ctx, use_fsc),
            Av2ChromaPlane::V => write_v_txb_all_zero(writer, skip_ctx),
        }
        return (0, false);
    };

    match plane {
        Av2ChromaPlane::U => write_u_txb_nonzero(writer, skip_ctx, use_fsc),
        Av2ChromaPlane::V => write_v_txb_nonzero(writer, skip_ctx),
    }
    write_eob_uv(writer, eob);

    for scan_index in (1..eob).rev() {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        let coeff_ctx =
            chroma_nz_map_context(&levels, pos, scan_index, scan_index + 1 == eob, plane);
        write_chroma_coefficient_level(
            writer,
            &levels,
            pos,
            scan_index + 1 == eob,
            coeff_ctx,
            level,
        );
    }

    let dc_level = levels[0];
    let dc_ctx = chroma_nz_map_context(&levels, 0, 0, eob == 1, plane);
    write_chroma_coefficient_level(writer, &levels, 0, eob == 1, dc_ctx, dc_level);

    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in (0..eob).rev() {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        let sign_name = match plane {
            Av2ChromaPlane::U if scan_index == 0 => "tile.coeff.u.dc_sign_negative",
            Av2ChromaPlane::V if scan_index == 0 => "tile.coeff.v.dc_sign_negative",
            Av2ChromaPlane::U => "tile.coeff.u.ac_sign_negative",
            Av2ChromaPlane::V => "tile.coeff.v.ac_sign_negative",
        };
        writer.write_literal_bit(sign_name, negative);
        write_chroma_high_range(writer, plane, pos, level, &mut hr_level_avg);
        if scan_index == 0 {
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        }
        cul_level += level;
    }

    (lossless_entropy_context(cul_level, dc_val), true)
}

fn write_chroma_tx8x8_txb(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    skip_ctx: u8,
    coefficients: &[i32; TX8X8_SAMPLES],
    use_inter_contexts: bool,
) -> (u8, bool) {
    let (levels, bounds) = tx8x8_coefficient_levels_and_bounds(coefficients);
    let Some((_, eob)) = bounds else {
        match plane {
            Av2ChromaPlane::U => write_u_txb_all_zero_tx8x8(writer, skip_ctx, use_inter_contexts),
            Av2ChromaPlane::V => write_v_txb_all_zero(writer, skip_ctx),
        }
        return (0, false);
    };

    match plane {
        Av2ChromaPlane::U => write_u_txb_nonzero_tx8x8(writer, skip_ctx, use_inter_contexts),
        Av2ChromaPlane::V => write_v_txb_nonzero(writer, skip_ctx),
    }
    write_eob_uv_tx8x8(writer, eob);

    for scan_index in (1..eob).rev() {
        let pos = TX8X8_SCAN[scan_index];
        let level = levels[pos];
        let coeff_ctx =
            chroma_tx8x8_nz_map_context(&levels, pos, scan_index, scan_index + 1 == eob, plane);
        write_chroma_tx8x8_coefficient_level(
            writer,
            &levels,
            pos,
            scan_index + 1 == eob,
            coeff_ctx,
            level,
        );
    }

    let dc_level = levels[0];
    let dc_ctx = chroma_tx8x8_nz_map_context(&levels, 0, 0, eob == 1, plane);
    write_chroma_tx8x8_coefficient_level(writer, &levels, 0, eob == 1, dc_ctx, dc_level);

    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in (0..eob).rev() {
        let pos = TX8X8_SCAN[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        let sign_name = match plane {
            Av2ChromaPlane::U if scan_index == 0 => "tile.coeff.u.dc_sign_negative_tx8x8",
            Av2ChromaPlane::V if scan_index == 0 => "tile.coeff.v.dc_sign_negative_tx8x8",
            Av2ChromaPlane::U => "tile.coeff.u.ac_sign_negative_tx8x8",
            Av2ChromaPlane::V => "tile.coeff.v.ac_sign_negative_tx8x8",
        };
        writer.write_literal_bit(sign_name, negative);
        write_chroma_high_range(writer, plane, pos, level, &mut hr_level_avg);
        if scan_index == 0 {
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        }
        cul_level += level;
    }

    (lossless_entropy_context(cul_level, dc_val), true)
}

fn write_chroma_tx4x8_txb(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    skip_ctx: u8,
    coefficients: &[i32; TX4X8_SAMPLES],
    use_inter_contexts: bool,
) -> (u8, bool) {
    let (levels, bounds) = tx4x8_coefficient_levels_and_bounds(coefficients);
    let Some((_, eob)) = bounds else {
        match plane {
            Av2ChromaPlane::U => write_u_txb_all_zero_tx8x8(writer, skip_ctx, use_inter_contexts),
            Av2ChromaPlane::V => write_v_txb_all_zero(writer, skip_ctx),
        }
        return (0, false);
    };

    match plane {
        Av2ChromaPlane::U => write_u_txb_nonzero_tx8x8(writer, skip_ctx, use_inter_contexts),
        Av2ChromaPlane::V => write_v_txb_nonzero(writer, skip_ctx),
    }
    write_eob_uv_tx4x8(writer, eob);

    for scan_index in (1..eob).rev() {
        let pos = TX4X8_SCAN[scan_index];
        let level = levels[pos];
        let coeff_ctx =
            chroma_tx4x8_nz_map_context(&levels, pos, scan_index, scan_index + 1 == eob, plane);
        write_chroma_tx4x8_coefficient_level(
            writer,
            &levels,
            pos,
            scan_index + 1 == eob,
            coeff_ctx,
            level,
        );
    }

    let dc_level = levels[0];
    let dc_ctx = chroma_tx4x8_nz_map_context(&levels, 0, 0, eob == 1, plane);
    write_chroma_tx4x8_coefficient_level(writer, &levels, 0, eob == 1, dc_ctx, dc_level);

    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in (0..eob).rev() {
        let pos = TX4X8_SCAN[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        let sign_name = match plane {
            Av2ChromaPlane::U if scan_index == 0 => "tile.coeff.u.dc_sign_negative_tx4x8",
            Av2ChromaPlane::V if scan_index == 0 => "tile.coeff.v.dc_sign_negative_tx4x8",
            Av2ChromaPlane::U => "tile.coeff.u.ac_sign_negative_tx4x8",
            Av2ChromaPlane::V => "tile.coeff.v.ac_sign_negative_tx4x8",
        };
        writer.write_literal_bit(sign_name, negative);
        write_chroma_high_range(writer, plane, pos, level, &mut hr_level_avg);
        if scan_index == 0 {
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        }
        cul_level += level;
    }

    (lossless_entropy_context(cul_level, dc_val), true)
}

fn lossless_coefficient_levels_and_bounds(
    coefficients: &[i32; TX4X4_SAMPLES],
) -> ([u32; TX4X4_SAMPLES], Option<(usize, usize)>) {
    let mut levels = [0u32; TX4X4_SAMPLES];
    let mut first = None;
    let mut eob = 0usize;
    for (scan_index, &index) in TX4X4_SCAN.iter().enumerate() {
        let coefficient = coefficients[index];
        debug_assert_eq!(
            coefficient % 8,
            0,
            "AV2 lossless WHT coefficient must be divisible by UNIT_QUANT_FACTOR"
        );
        let level = coefficient.unsigned_abs() / 8;
        levels[index] = level;
        if level != 0 {
            first.get_or_insert(scan_index);
            eob = scan_index + 1;
        }
    }
    (levels, first.map(|first| (first, eob)))
}

fn tx8x8_coefficient_levels_and_bounds(
    coefficients: &[i32; TX8X8_SAMPLES],
) -> ([u32; TX8X8_SAMPLES], Option<(usize, usize)>) {
    let mut levels = [0u32; TX8X8_SAMPLES];
    let mut first = None;
    let mut eob = 0usize;
    for (scan_index, &index) in TX8X8_SCAN.iter().enumerate() {
        let coefficient = coefficients[index];
        debug_assert_eq!(
            coefficient % 8,
            0,
            "AV2 quantized DCT coefficient must be scaled by UNIT_QUANT_FACTOR"
        );
        let level = coefficient.unsigned_abs() / 8;
        levels[index] = level;
        if level != 0 {
            first.get_or_insert(scan_index);
            eob = scan_index + 1;
        }
    }
    (levels, first.map(|first| (first, eob)))
}

fn tx4x8_coefficient_levels_and_bounds(
    coefficients: &[i32; TX4X8_SAMPLES],
) -> ([u32; TX4X8_SAMPLES], Option<(usize, usize)>) {
    let mut levels = [0u32; TX4X8_SAMPLES];
    let mut first = None;
    let mut eob = 0usize;
    for (scan_index, &index) in TX4X8_SCAN.iter().enumerate() {
        let coefficient = coefficients[index];
        debug_assert_eq!(
            coefficient % 8,
            0,
            "AV2 quantized DCT coefficient must be scaled by UNIT_QUANT_FACTOR"
        );
        let level = coefficient.unsigned_abs() / 8;
        levels[index] = level;
        if level != 0 {
            first.get_or_insert(scan_index);
            eob = scan_index + 1;
        }
    }
    (levels, first.map(|first| (first, eob)))
}

fn write_eob_y(writer: &mut Av2EntropyWriter, eob: usize) {
    let (eob_pt, eob_extra) = eob_pos_token(eob);
    let mut cdf = DEFAULT_EOB_MULTI16_Y_CTX0_CDF;
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.y.eob_pt_tx4x4",
        AV2_STATIC_CDF_EOB_Y,
        eob_pt - 1,
        &mut cdf,
        5,
        false,
    );

    let eob_offset_bits = eob_offset_bits(eob_pt);
    if eob_offset_bits > 0 {
        let eob_shift = eob_offset_bits - 1;
        let bit = (eob_extra & (1 << eob_shift)) != 0;
        let mut extra_cdf = DEFAULT_EOB_EXTRA_CDF;
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.eob_extra_bit",
            AV2_STATIC_CDF_EOB_EXTRA,
            usize::from(bit),
            &mut extra_cdf,
            2,
            false,
        );
        let low_bits = eob_extra & ((1 << eob_shift) - 1);
        writer.write_literal("tile.coeff.y.eob_extra", low_bits as u32, eob_shift as u8);
    }
}

fn write_eob_y_inter(writer: &mut Av2EntropyWriter, eob: usize) {
    let (eob_pt, eob_extra) = eob_pos_token(eob);
    let mut cdf = DEFAULT_EOB_MULTI16_Y_INTER_CTX0_CDF;
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.y.inter_eob_pt_tx4x4",
        AV2_STATIC_CDF_EOB_Y_INTER,
        eob_pt - 1,
        &mut cdf,
        5,
        false,
    );

    let eob_offset_bits = eob_offset_bits(eob_pt);
    if eob_offset_bits > 0 {
        let eob_shift = eob_offset_bits - 1;
        let bit = (eob_extra & (1 << eob_shift)) != 0;
        let mut extra_cdf = DEFAULT_EOB_EXTRA_CDF;
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.eob_extra_bit",
            AV2_STATIC_CDF_EOB_EXTRA,
            usize::from(bit),
            &mut extra_cdf,
            2,
            false,
        );
        let low_bits = eob_extra & ((1 << eob_shift) - 1);
        writer.write_literal("tile.coeff.y.inter_eob_extra", low_bits as u32, eob_shift as u8);
    }
}

fn regular_inter_tx_type_eob_ctx_4x4(eob: usize) -> usize {
    debug_assert!((1..=TX4X4_SAMPLES).contains(&eob));
    // AVM get_lp2tx_ctx() derives the transform-type context from eob - 1 as
    // a raster last-position value, not from the coefficient scan position.
    let last = eob - 1;
    let diag = last % TX4X4_SIZE + last / TX4X4_SIZE;
    if diag < 2 {
        1
    } else if diag > 4 {
        2
    } else {
        0
    }
}

fn write_regular_inter_dct_dct_tx_type(writer: &mut Av2EntropyWriter, eob: usize) {
    let eob_ctx = regular_inter_tx_type_eob_ctx_4x4(eob);
    let mut cdf = DEFAULT_INTER_EXT_TX_DCT_IDTX_4X4_CDF;
    // With reduced_tx_set_used=2, inter 4x4 uses EXT_TX_SET_DCT_IDTX:
    // symbol 1 maps to DCT_DCT and symbol 0 maps to IDTX.
    writer.write_symbol_with_static_cdf_key(
        "tile.tx_type.inter_reduced_dct_dct",
        AV2_STATIC_CDF_INTER_EXT_TX_DCT_IDTX_4X4_BASE + eob_ctx,
        1,
        &mut cdf,
        2,
        false,
    );
}

fn write_eob_uv(writer: &mut Av2EntropyWriter, eob: usize) {
    let (eob_pt, eob_extra) = eob_pos_token(eob);
    let mut cdf = DEFAULT_EOB_MULTI16_UV_CTX2_CDF;
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.uv.eob_pt_tx4x4",
        AV2_STATIC_CDF_EOB_UV,
        eob_pt - 1,
        &mut cdf,
        5,
        false,
    );

    let eob_offset_bits = eob_offset_bits(eob_pt);
    if eob_offset_bits > 0 {
        let eob_shift = eob_offset_bits - 1;
        let bit = (eob_extra & (1 << eob_shift)) != 0;
        let mut extra_cdf = DEFAULT_EOB_EXTRA_CDF;
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.eob_extra_bit",
            AV2_STATIC_CDF_EOB_EXTRA,
            usize::from(bit),
            &mut extra_cdf,
            2,
            false,
        );
        let low_bits = eob_extra & ((1 << eob_shift) - 1);
        writer.write_literal("tile.coeff.uv.eob_extra", low_bits as u32, eob_shift as u8);
    }
}

fn write_eob_uv_tx8x8(writer: &mut Av2EntropyWriter, eob: usize) {
    let (eob_pt, eob_extra) = eob_pos_token(eob);
    let mut cdf = DEFAULT_EOB_MULTI64_UV_CTX2_CDF;
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.uv.eob_pt_tx8x8",
        AV2_STATIC_CDF_EOB_UV_TX8X8,
        eob_pt - 1,
        &mut cdf,
        7,
        false,
    );

    let eob_offset_bits = eob_offset_bits(eob_pt);
    if eob_offset_bits > 0 {
        let eob_shift = eob_offset_bits - 1;
        let bit = (eob_extra & (1 << eob_shift)) != 0;
        let mut extra_cdf = DEFAULT_EOB_EXTRA_CDF;
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.eob_extra_bit",
            AV2_STATIC_CDF_EOB_EXTRA,
            usize::from(bit),
            &mut extra_cdf,
            2,
            false,
        );
        let low_bits = eob_extra & ((1 << eob_shift) - 1);
        writer.write_literal("tile.coeff.uv.eob_extra_tx8x8", low_bits as u32, eob_shift as u8);
    }
}

fn write_eob_uv_tx4x8(writer: &mut Av2EntropyWriter, eob: usize) {
    let (eob_pt, eob_extra) = eob_pos_token(eob);
    let mut cdf = DEFAULT_EOB_MULTI32_UV_CTX2_CDF;
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.uv.eob_pt_tx4x8",
        AV2_STATIC_CDF_EOB_UV_TX4X8,
        eob_pt - 1,
        &mut cdf,
        6,
        false,
    );

    let eob_offset_bits = eob_offset_bits(eob_pt);
    if eob_offset_bits > 0 {
        let eob_shift = eob_offset_bits - 1;
        let bit = (eob_extra & (1 << eob_shift)) != 0;
        let mut extra_cdf = DEFAULT_EOB_EXTRA_CDF;
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.eob_extra_bit",
            AV2_STATIC_CDF_EOB_EXTRA,
            usize::from(bit),
            &mut extra_cdf,
            2,
            false,
        );
        let low_bits = eob_extra & ((1 << eob_shift) - 1);
        writer.write_literal("tile.coeff.uv.eob_extra_tx4x8", low_bits as u32, eob_shift as u8);
    }
}

fn eob_pos_token(eob: usize) -> (usize, usize) {
    const EOB_TO_POS_SMALL: [usize; 33] = [
        0, 1, 2, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
        6, 6, 6,
    ];
    const EOB_GROUP_START: [usize; 12] = [0, 1, 2, 3, 5, 9, 17, 33, 65, 129, 257, 513];
    assert!((1..=TX8X8_SAMPLES).contains(&eob));
    let token = if eob < EOB_TO_POS_SMALL.len() {
        EOB_TO_POS_SMALL[eob]
    } else {
        7
    };
    (token, eob - EOB_GROUP_START[token])
}

fn eob_offset_bits(eob_pt: usize) -> usize {
    const EOB_OFFSET_BITS: [usize; 12] = [0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    EOB_OFFSET_BITS[eob_pt]
}

fn write_chroma_coefficient_level(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    is_eob_coefficient: bool,
    coeff_ctx: usize,
    level: u32,
) {
    let limits = chroma_lf_limits(pos);
    if is_eob_coefficient {
        assert!(level > 0, "AV2 EOB coefficient must be non-zero");
        if limits {
            let mut cdf = DEFAULT_COEFF_BASE_LF_EOB_UV_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.uv.base_lf_eob",
                AV2_STATIC_CDF_COEFF_UV_BASE_LF_EOB_BASE + coeff_ctx,
                level.min(5) as usize - 1,
                &mut cdf,
                5,
                false,
            );
        } else {
            let mut cdf = DEFAULT_COEFF_BASE_EOB_UV_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.uv.base_eob",
                AV2_STATIC_CDF_COEFF_UV_BASE_EOB_BASE + coeff_ctx,
                level.min(3) as usize - 1,
                &mut cdf,
                3,
                false,
            );
            if level > 2 {
                write_chroma_low_range(writer, levels, pos, level - 3);
            }
        }
    } else if limits {
        let mut cdf = DEFAULT_COEFF_BASE_LF_UV_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.uv.base_lf",
            AV2_STATIC_CDF_COEFF_UV_BASE_LF_BASE + coeff_ctx,
            level.min(5) as usize,
            &mut cdf,
            6,
            false,
        );
    } else {
        let mut cdf = DEFAULT_COEFF_BASE_UV_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.uv.base",
            AV2_STATIC_CDF_COEFF_UV_BASE_BASE + coeff_ctx,
            level.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
        if level > 2 {
            write_chroma_low_range(writer, levels, pos, level - 3);
        }
    }
}

fn write_chroma_tx8x8_coefficient_level(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX8X8_SAMPLES],
    pos: usize,
    is_eob_coefficient: bool,
    coeff_ctx: usize,
    level: u32,
) {
    let limits = chroma_tx8x8_lf_limits(pos);
    if is_eob_coefficient {
        assert!(level > 0, "AV2 EOB coefficient must be non-zero");
        if limits {
            let mut cdf = DEFAULT_COEFF_BASE_LF_EOB_UV_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.uv.base_lf_eob_tx8x8",
                AV2_STATIC_CDF_COEFF_UV_BASE_LF_EOB_BASE + coeff_ctx,
                level.min(5) as usize - 1,
                &mut cdf,
                5,
                false,
            );
        } else {
            let mut cdf = DEFAULT_COEFF_BASE_EOB_UV_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.uv.base_eob_tx8x8",
                AV2_STATIC_CDF_COEFF_UV_BASE_EOB_BASE + coeff_ctx,
                level.min(3) as usize - 1,
                &mut cdf,
                3,
                false,
            );
            if level > 2 {
                write_chroma_tx8x8_low_range(writer, levels, pos, level - 3);
            }
        }
    } else if limits {
        let mut cdf = DEFAULT_COEFF_BASE_LF_UV_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.uv.base_lf_tx8x8",
            AV2_STATIC_CDF_COEFF_UV_BASE_LF_BASE + coeff_ctx,
            level.min(5) as usize,
            &mut cdf,
            6,
            false,
        );
    } else {
        let mut cdf = DEFAULT_COEFF_BASE_UV_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.uv.base_tx8x8",
            AV2_STATIC_CDF_COEFF_UV_BASE_BASE + coeff_ctx,
            level.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
        if level > 2 {
            write_chroma_tx8x8_low_range(writer, levels, pos, level - 3);
        }
    }
}

fn write_chroma_tx4x8_coefficient_level(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X8_SAMPLES],
    pos: usize,
    is_eob_coefficient: bool,
    coeff_ctx: usize,
    level: u32,
) {
    let limits = chroma_tx4x8_lf_limits(pos);
    if is_eob_coefficient {
        assert!(level > 0, "AV2 EOB coefficient must be non-zero");
        if limits {
            let mut cdf = DEFAULT_COEFF_BASE_LF_EOB_UV_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.uv.base_lf_eob_tx4x8",
                AV2_STATIC_CDF_COEFF_UV_BASE_LF_EOB_BASE + coeff_ctx,
                level.min(5) as usize - 1,
                &mut cdf,
                5,
                false,
            );
        } else {
            let mut cdf = DEFAULT_COEFF_BASE_EOB_UV_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.uv.base_eob_tx4x8",
                AV2_STATIC_CDF_COEFF_UV_BASE_EOB_BASE + coeff_ctx,
                level.min(3) as usize - 1,
                &mut cdf,
                3,
                false,
            );
            if level > 2 {
                write_chroma_tx4x8_low_range(writer, levels, pos, level - 3);
            }
        }
    } else if limits {
        let mut cdf = DEFAULT_COEFF_BASE_LF_UV_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.uv.base_lf_tx4x8",
            AV2_STATIC_CDF_COEFF_UV_BASE_LF_BASE + coeff_ctx,
            level.min(5) as usize,
            &mut cdf,
            6,
            false,
        );
    } else {
        let mut cdf = DEFAULT_COEFF_BASE_UV_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.uv.base_tx4x8",
            AV2_STATIC_CDF_COEFF_UV_BASE_BASE + coeff_ctx,
            level.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
        if level > 2 {
            write_chroma_tx4x8_low_range(writer, levels, pos, level - 3);
        }
    }
}

fn write_luma_coefficient_level(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    is_eob_coefficient: bool,
    coeff_ctx: usize,
    level: u32,
) {
    let limits = luma_lf_limits(pos);
    if is_eob_coefficient {
        assert!(level > 0, "AV2 EOB coefficient must be non-zero");
        if limits {
            let mut cdf = DEFAULT_COEFF_BASE_LF_EOB_Y_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.y.base_lf_eob",
                AV2_STATIC_CDF_COEFF_Y_BASE_LF_EOB_BASE + coeff_ctx,
                level.min(5) as usize - 1,
                &mut cdf,
                5,
                false,
            );
            if level > 4 {
                write_luma_low_range(writer, levels, pos, true, level - 5);
            }
        } else {
            let mut cdf = DEFAULT_COEFF_BASE_EOB_Y_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.y.base_eob",
                AV2_STATIC_CDF_COEFF_Y_BASE_EOB_BASE + coeff_ctx,
                level.min(3) as usize - 1,
                &mut cdf,
                3,
                false,
            );
            if level > 2 {
                write_luma_low_range(writer, levels, pos, false, level - 3);
            }
        }
    } else if limits {
        let mut cdf = DEFAULT_COEFF_BASE_LF_Y_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.base_lf",
            AV2_STATIC_CDF_COEFF_Y_BASE_LF_BASE + coeff_ctx,
            level.min(5) as usize,
            &mut cdf,
            6,
            false,
        );
        if level > 4 {
            write_luma_low_range(writer, levels, pos, true, level - 5);
        }
    } else {
        let mut cdf = DEFAULT_COEFF_BASE_Y_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.base",
            AV2_STATIC_CDF_COEFF_Y_BASE_BASE + coeff_ctx,
            level.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
        if level > 2 {
            write_luma_low_range(writer, levels, pos, false, level - 3);
        }
    }
}

fn write_luma_low_range(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    lf: bool,
    base_range: u32,
) {
    if lf {
        let br_ctx = luma_br_lf_context(levels, pos);
        let mut cdf = DEFAULT_COEFF_BR_LF_Y_CDFS[br_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.low_range_lf",
            AV2_STATIC_CDF_COEFF_Y_BR_LF_BASE + br_ctx,
            base_range.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
    } else {
        let br_ctx = luma_br_context(levels, pos);
        let mut cdf = DEFAULT_COEFF_BR_Y_CDFS[br_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.low_range",
            AV2_STATIC_CDF_COEFF_Y_BR_BASE + br_ctx,
            base_range.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
    }
}

fn write_chroma_low_range(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    base_range: u32,
) {
    let br_ctx = chroma_br_context(levels, pos);
    let mut cdf = DEFAULT_COEFF_BR_UV_CDFS[br_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.uv.low_range",
        AV2_STATIC_CDF_COEFF_UV_BR_BASE + br_ctx,
        base_range.min(3) as usize,
        &mut cdf,
        4,
        false,
    );
}

fn write_chroma_tx8x8_low_range(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX8X8_SAMPLES],
    pos: usize,
    base_range: u32,
) {
    let br_ctx = chroma_tx8x8_br_context(levels, pos);
    let mut cdf = DEFAULT_COEFF_BR_UV_CDFS[br_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.uv.low_range_tx8x8",
        AV2_STATIC_CDF_COEFF_UV_BR_BASE + br_ctx,
        base_range.min(3) as usize,
        &mut cdf,
        4,
        false,
    );
}

fn write_chroma_tx4x8_low_range(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X8_SAMPLES],
    pos: usize,
    base_range: u32,
) {
    let br_ctx = chroma_tx4x8_br_context(levels, pos);
    let mut cdf = DEFAULT_COEFF_BR_UV_CDFS[br_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.uv.low_range_tx4x8",
        AV2_STATIC_CDF_COEFF_UV_BR_BASE + br_ctx,
        base_range.min(3) as usize,
        &mut cdf,
        4,
        false,
    );
}

fn write_idtx_low_range(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    level: u32,
) {
    let br_ctx = idtx_br_context(levels, pos);
    let mut cdf = DEFAULT_COEFF_BR_IDTX_CDFS[br_ctx];
    writer.write_symbol(
        "tile.coeff.y.idtx_low_range",
        (level - 3).min(3) as usize,
        &mut cdf,
        4,
        false,
    );
}

fn write_luma_high_range(
    writer: &mut Av2EntropyWriter,
    pos: usize,
    level: u32,
    hr_level_avg: &mut u32,
) {
    let limits = luma_lf_limits(pos);
    let threshold = if limits { 7 } else { 5 };
    if level <= threshold {
        return;
    }
    let decoded_base = threshold + 1;
    let high_range = level.saturating_sub(decoded_base);
    write_adaptive_high_range_with_context(
        writer,
        "tile.coeff.y.high_range",
        high_range,
        *hr_level_avg,
    );
    *hr_level_avg = (*hr_level_avg + high_range) >> 1;
}

fn write_idtx_high_range(writer: &mut Av2EntropyWriter, level: u32, hr_level_avg: &mut u32) {
    if level <= 5 {
        return;
    }
    let high_range = level - 6;
    write_adaptive_high_range_with_context(
        writer,
        "tile.coeff.y.idtx_high_range",
        high_range,
        *hr_level_avg,
    );
    *hr_level_avg = (*hr_level_avg + high_range) >> 1;
}

fn write_chroma_high_range(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    pos: usize,
    level: u32,
    hr_level_avg: &mut u32,
) {
    let limits = chroma_lf_limits(pos);
    let threshold = if limits { 4 } else { 5 };
    if level <= threshold {
        return;
    }
    let decoded_base = if limits { 5 } else { 6 };
    let high_range = level.saturating_sub(decoded_base);
    let name = match plane {
        Av2ChromaPlane::U => "tile.coeff.u.high_range",
        Av2ChromaPlane::V => "tile.coeff.v.high_range",
    };
    write_adaptive_high_range_with_context(writer, name, high_range, *hr_level_avg);
    *hr_level_avg = (*hr_level_avg + high_range) >> 1;
}

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
    if chroma_tx8x8_lf_limits(pos) {
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
    if chroma_tx4x8_lf_limits(pos) {
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

fn idtx_bob_context(scan_index: usize) -> usize {
    if scan_index <= TX4X4_SAMPLES / 8 {
        0
    } else if scan_index <= TX4X4_SAMPLES / 4 {
        1
    } else {
        2
    }
}

fn idtx_upper_levels_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    let mag = idtx_left_level(levels, pos).min(3) + idtx_above_level(levels, pos).min(3);
    mag.min(6) as usize
}

fn idtx_br_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    let mag = idtx_left_level(levels, pos).min(5) + idtx_above_level(levels, pos).min(5);
    mag.min(6) as usize
}

fn idtx_sign_context(
    levels: &[u32; TX4X4_SAMPLES],
    coefficients: &[i32; TX4X4_SAMPLES],
    pos: usize,
) -> usize {
    let mut sign_sum = 0i32;
    if let Some(left) = idtx_left_pos(pos).filter(|&left| levels[left] != 0) {
        sign_sum += idtx_sign_value(coefficients[left]);
    }
    if let Some(above) = idtx_above_pos(pos).filter(|&above| levels[above] != 0) {
        sign_sum += idtx_sign_value(coefficients[above]);
    }
    if let Some(above_left) = idtx_above_left_pos(pos).filter(|&above_left| levels[above_left] != 0)
    {
        sign_sum += idtx_sign_value(coefficients[above_left]);
    }
    let mut ctx = if sign_sum > 2 {
        5
    } else if sign_sum < -2 {
        6
    } else if sign_sum > 0 {
        1
    } else if sign_sum < 0 {
        2
    } else {
        0
    };
    if levels[pos] > 3 && ctx != 0 {
        ctx += 2;
    }
    ctx
}

fn idtx_sign_value(coefficient: i32) -> i32 {
    if coefficient < 0 {
        -1
    } else {
        1
    }
}

fn idtx_left_level(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> u32 {
    idtx_left_pos(pos).map_or(0, |left| levels[left].min(127))
}

fn idtx_above_level(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> u32 {
    idtx_above_pos(pos).map_or(0, |above| levels[above].min(127))
}

fn idtx_left_pos(pos: usize) -> Option<usize> {
    if pos % TX4X4_SIZE != 0 {
        Some(pos - 1)
    } else {
        None
    }
}

fn idtx_above_pos(pos: usize) -> Option<usize> {
    if pos >= TX4X4_SIZE {
        Some(pos - TX4X4_SIZE)
    } else {
        None
    }
}

fn idtx_above_left_pos(pos: usize) -> Option<usize> {
    if pos % TX4X4_SIZE != 0 && pos >= TX4X4_SIZE {
        Some(pos - TX4X4_SIZE - 1)
    } else {
        None
    }
}

fn tx4x4_level_at(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    row_delta: usize,
    col_delta: usize,
) -> u32 {
    let row = pos / TX4X4_SIZE + row_delta;
    let col = pos % TX4X4_SIZE + col_delta;
    if row < TX4X4_SIZE && col < TX4X4_SIZE {
        levels[row * TX4X4_SIZE + col].min(127)
    } else {
        0
    }
}

fn tx8x8_level_at(
    levels: &[u32; TX8X8_SAMPLES],
    pos: usize,
    row_delta: usize,
    col_delta: usize,
) -> u32 {
    let row = pos / TX8X8_SIZE + row_delta;
    let col = pos % TX8X8_SIZE + col_delta;
    if row < TX8X8_SIZE && col < TX8X8_SIZE {
        levels[row * TX8X8_SIZE + col].min(127)
    } else {
        0
    }
}

fn tx4x8_level_at(
    levels: &[u32; TX4X8_SAMPLES],
    pos: usize,
    row_delta: usize,
    col_delta: usize,
) -> u32 {
    let row = pos / TX4X8_WIDTH + row_delta;
    let col = pos % TX4X8_WIDTH + col_delta;
    if row < TX4X8_HEIGHT && col < TX4X8_WIDTH {
        levels[row * TX4X8_WIDTH + col].min(127)
    } else {
        0
    }
}

fn chroma_lf_limits(pos: usize) -> bool {
    let row = pos / TX4X4_SIZE;
    let col = pos % TX4X4_SIZE;
    row + col < 1
}

fn chroma_tx8x8_lf_limits(pos: usize) -> bool {
    let row = pos / TX8X8_SIZE;
    let col = pos % TX8X8_SIZE;
    row + col < 1
}

fn chroma_tx4x8_lf_limits(pos: usize) -> bool {
    let row = pos / TX4X8_WIDTH;
    let col = pos % TX4X8_WIDTH;
    row + col < 1
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

fn write_y_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    let skip_ctx = normalize_av2_context(skip_ctx, 1, 5, 5, "AV2 luma TXB skip");
    let (name, mut cdf) = match skip_ctx {
        1 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx1",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx2",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx3",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx4",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX5_CDF,
        ),
        _ => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX5_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        y_txb_skip_static_cdf_key(skip_ctx),
        1,
        &mut cdf,
        2,
        false,
    );
}

fn write_y_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    let skip_ctx = normalize_av2_context(skip_ctx, 1, 5, 5, "AV2 luma TXB skip");
    let (name, mut cdf) = match skip_ctx {
        1 => (
            "tile.coeff.y.txb_nonzero_tx4x4_ctx1",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.txb_nonzero_tx4x4_ctx2",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.y.txb_nonzero_tx4x4_ctx3",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.y.txb_nonzero_tx4x4_ctx4",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.y.txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX5_CDF,
        ),
        _ => (
            "tile.coeff.y.txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX5_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        y_txb_skip_static_cdf_key(skip_ctx),
        0,
        &mut cdf,
        2,
        false,
    );
}

fn write_y_inter_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    let skip_ctx = normalize_av2_context(skip_ctx, 1, 5, 5, "AV2 inter luma TXB skip");
    let (name, mut cdf) = match skip_ctx {
        1 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx1",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx2",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx3",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx4",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX5_CDF,
        ),
        _ => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX5_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        y_inter_txb_skip_static_cdf_key(skip_ctx),
        1,
        &mut cdf,
        2,
        false,
    );
}

fn write_y_inter_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    let skip_ctx = normalize_av2_context(skip_ctx, 1, 5, 5, "AV2 inter luma TXB skip");
    let (name, mut cdf) = match skip_ctx {
        1 => (
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx1",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx2",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx3",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx4",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX5_CDF,
        ),
        _ => (
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX5_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        y_inter_txb_skip_static_cdf_key(skip_ctx),
        0,
        &mut cdf,
        2,
        false,
    );
}

fn write_y_fsc_txb_all_zero(writer: &mut Av2EntropyWriter) {
    let mut cdf = DEFAULT_TXB_SKIP_Y_FSC_TX4X4_CTX9_CDF;
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.y.txb_all_zero_fsc_tx4x4_ctx9",
        y_fsc_txb_skip_static_cdf_key(9),
        1,
        &mut cdf,
        2,
        false,
    );
}

fn write_y_fsc_txb_nonzero(writer: &mut Av2EntropyWriter) {
    let mut cdf = DEFAULT_TXB_SKIP_Y_FSC_TX4X4_CTX9_CDF;
    writer.write_symbol_with_static_cdf_key(
        "tile.coeff.y.txb_nonzero_fsc_tx4x4_ctx9",
        y_fsc_txb_skip_static_cdf_key(9),
        0,
        &mut cdf,
        2,
        false,
    );
}

fn write_u_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 6, 8, 8, "AV2 U TXB skip");
    let (name, mut cdf) = match skip_ctx {
        6 if use_fsc => (
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx6",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX6_CDF,
        ),
        6 => (
            "tile.coeff.u.txb_nonzero_tx4x4_ctx6",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX6_CDF,
        ),
        7 if use_fsc => (
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx7",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX7_CDF,
        ),
        7 => (
            "tile.coeff.u.txb_nonzero_tx4x4_ctx7",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX7_CDF,
        ),
        8 if use_fsc => (
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX8_CDF,
        ),
        8 => (
            "tile.coeff.u.txb_nonzero_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX8_CDF,
        ),
        _ if use_fsc => (
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX8_CDF,
        ),
        _ => (
            "tile.coeff.u.txb_nonzero_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX8_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        u_txb_skip_static_cdf_key(skip_ctx, use_fsc),
        0,
        &mut cdf,
        2,
        false,
    );
}

fn write_v_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    let skip_ctx = normalize_av2_context(skip_ctx, 0, 11, 11, "AV2 V TXB skip");
    let (name, mut cdf) = match skip_ctx {
        0 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx0",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX0_CDF,
        ),
        1 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx1",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx2",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx3",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx4",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx5",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX5_CDF,
        ),
        6 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx6",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX6_CDF,
        ),
        7 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx7",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX7_CDF,
        ),
        8 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx8",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX8_CDF,
        ),
        9 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx9",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX9_CDF,
        ),
        10 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx10",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX10_CDF,
        ),
        11 => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx11",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX11_CDF,
        ),
        _ => (
            "tile.coeff.v.txb_nonzero_tx4x4_ctx11",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX11_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        v_txb_skip_static_cdf_key(skip_ctx),
        0,
        &mut cdf,
        2,
        false,
    );
}

fn write_u_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 6, 8, 8, "AV2 U TXB skip");
    let (name, mut cdf) = match skip_ctx {
        6 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx6",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX6_CDF,
        ),
        6 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx6",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX6_CDF,
        ),
        7 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx7",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX7_CDF,
        ),
        7 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx7",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX7_CDF,
        ),
        8 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX8_CDF,
        ),
        8 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX8_CDF,
        ),
        _ if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX8_CDF,
        ),
        _ => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX8_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        u_txb_skip_static_cdf_key(skip_ctx, use_fsc),
        1,
        &mut cdf,
        2,
        false,
    );
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
    let (name, mut cdf) = match (use_inter_contexts, skip_ctx) {
        (false, 6) => (
            if all_zero {
                "tile.coeff.u.txb_all_zero_tx8x8_ctx6"
            } else {
                "tile.coeff.u.txb_nonzero_tx8x8_ctx6"
            },
            DEFAULT_TXB_SKIP_U_TX8X8_CTX6_CDF,
        ),
        (false, 7) => (
            if all_zero {
                "tile.coeff.u.txb_all_zero_tx8x8_ctx7"
            } else {
                "tile.coeff.u.txb_nonzero_tx8x8_ctx7"
            },
            DEFAULT_TXB_SKIP_U_TX8X8_CTX7_CDF,
        ),
        (false, 8) => (
            if all_zero {
                "tile.coeff.u.txb_all_zero_tx8x8_ctx8"
            } else {
                "tile.coeff.u.txb_nonzero_tx8x8_ctx8"
            },
            DEFAULT_TXB_SKIP_U_TX8X8_CTX8_CDF,
        ),
        (true, 6) => (
            if all_zero {
                "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx6"
            } else {
                "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx6"
            },
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX6_CDF,
        ),
        (true, 7) => (
            if all_zero {
                "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx7"
            } else {
                "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx7"
            },
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX7_CDF,
        ),
        (true, 8) => (
            if all_zero {
                "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx8"
            } else {
                "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx8"
            },
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX8_CDF,
        ),
        _ => unreachable!("normalized AV2 U TXB skip 8x8 context is in range"),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        u_txb_skip_tx8x8_static_cdf_key(skip_ctx, use_inter_contexts),
        usize::from(all_zero),
        &mut cdf,
        2,
        false,
    );
}

fn write_v_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    let skip_ctx = normalize_av2_context(skip_ctx, 0, 11, 11, "AV2 V TXB skip");
    let (name, mut cdf) = match skip_ctx {
        0 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx0",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX0_CDF,
        ),
        1 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx1",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx2",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx3",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx4",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx5",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX5_CDF,
        ),
        6 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx6",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX6_CDF,
        ),
        7 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx7",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX7_CDF,
        ),
        8 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx8",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX8_CDF,
        ),
        9 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx9",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX9_CDF,
        ),
        10 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx10",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX10_CDF,
        ),
        11 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx11",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX11_CDF,
        ),
        _ => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx11",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX11_CDF,
        ),
    };
    writer.write_symbol_with_static_cdf_key(
        name,
        v_txb_skip_static_cdf_key(skip_ctx),
        1,
        &mut cdf,
        2,
        false,
    );
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

#[cfg(feature = "bench-internals")]
pub(crate) fn bench_transform_quant_roundtrip_checksum(
    residuals: &[[i32; TX4X4_SAMPLES]],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> u64 {
    let mut checksum = 0xcbf2_9ce4_8422_2325u64;
    for residual in residuals {
        let fwht = tx4x4_coefficients_from_residual(residual, false);
        let idtx = tx4x4_coefficients_from_residual(residual, true);
        let dct = av2_fdct4x4(residual);
        let (qcoeff, dqcoeff) = av2_regular_quantize_dct4x4(&dct, qindex, bit_depth);
        let recon = av2_idct4x4(&dqcoeff, bit_depth);
        for value in fwht
            .iter()
            .chain(idtx.iter())
            .chain(dct.iter())
            .chain(qcoeff.iter())
            .chain(dqcoeff.iter())
            .chain(recon.iter())
        {
            checksum = checksum.rotate_left(5) ^ (*value as i64 as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    checksum
}
