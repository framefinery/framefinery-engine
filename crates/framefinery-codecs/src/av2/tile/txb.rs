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

#[derive(Clone, Copy)]
struct Av2RegularQuantParams {
    dequant: [i32; 2],
    quant_fp: [i64; 2],
    round_fp: [i64; 2],
}

impl Av2RegularQuantParams {
    fn new(qindex: u16, bit_depth: SampleBitDepth) -> Self {
        let dequant = av2_regular_dequant_qtx(qindex, bit_depth);
        let mut quant_fp = [0; 2];
        let mut round_fp = [0; 2];
        for (index, &dequant) in dequant.iter().enumerate() {
            quant_fp[index] =
                (1i64 << (16 + AV2_QUANT_FP_BITS + AV2_QUANT_TABLE_BITS)) / i64::from(dequant);
            round_fp[index] = (64 * i64::from(dequant)) >> (7 + AV2_QUANT_TABLE_BITS);
        }
        Self {
            dequant,
            quant_fp,
            round_fp,
        }
    }
}

#[cfg(any(test, feature = "bench-internals"))]
fn av2_regular_quantize_dct4x4(
    coefficients: &[i32; TX4X4_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> ([i32; TX4X4_SAMPLES], [i32; TX4X4_SAMPLES]) {
    let params = Av2RegularQuantParams::new(qindex, bit_depth);
    av2_regular_quantize_dct4x4_with_params(coefficients, params)
}

fn av2_regular_quantize_dct4x4_with_params(
    coefficients: &[i32; TX4X4_SAMPLES],
    params: Av2RegularQuantParams,
) -> ([i32; TX4X4_SAMPLES], [i32; TX4X4_SAMPLES]) {
    let mut qcoeff = [0i32; TX4X4_SAMPLES];
    for pos in 0..TX4X4_SAMPLES {
        qcoeff[pos] = av2_regular_quantize_coefficient(coefficients[pos], params, pos != 0);
    }
    let dqcoeff = av2_regular_dequantize(&qcoeff, params.dequant);
    (qcoeff, dqcoeff)
}

#[cfg(any(test, feature = "bench-internals"))]
fn av2_regular_quantize_dct8x8(
    coefficients: &[i32; TX8X8_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> ([i32; TX8X8_SAMPLES], [i32; TX8X8_SAMPLES]) {
    let params = Av2RegularQuantParams::new(qindex, bit_depth);
    av2_regular_quantize_dct8x8_with_params(coefficients, params)
}

fn av2_regular_quantize_dct8x8_with_params(
    coefficients: &[i32; TX8X8_SAMPLES],
    params: Av2RegularQuantParams,
) -> ([i32; TX8X8_SAMPLES], [i32; TX8X8_SAMPLES]) {
    let mut qcoeff = [0i32; TX8X8_SAMPLES];
    for pos in 0..TX8X8_SAMPLES {
        qcoeff[pos] = av2_regular_quantize_coefficient(coefficients[pos], params, pos != 0);
    }
    let dqcoeff = av2_regular_dequantize(&qcoeff, params.dequant);
    (qcoeff, dqcoeff)
}

#[cfg(any(test, feature = "bench-internals"))]
fn av2_regular_quantize_dct4x8(
    coefficients: &[i32; TX4X8_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> ([i32; TX4X8_SAMPLES], [i32; TX4X8_SAMPLES]) {
    let params = Av2RegularQuantParams::new(qindex, bit_depth);
    av2_regular_quantize_dct4x8_with_params(coefficients, params)
}

fn av2_regular_quantize_dct4x8_with_params(
    coefficients: &[i32; TX4X8_SAMPLES],
    params: Av2RegularQuantParams,
) -> ([i32; TX4X8_SAMPLES], [i32; TX4X8_SAMPLES]) {
    let mut qcoeff = [0i32; TX4X8_SAMPLES];
    for pos in 0..TX4X8_SAMPLES {
        qcoeff[pos] = av2_regular_quantize_coefficient(coefficients[pos], params, pos != 0);
    }
    let dqcoeff = av2_regular_dequantize(&qcoeff, params.dequant);
    (qcoeff, dqcoeff)
}

fn av2_regular_quantize_coefficient(
    coefficient: i32,
    params: Av2RegularQuantParams,
    ac_coefficient: bool,
) -> i32 {
    let rc01 = usize::from(ac_coefficient);
    let shift = 16 + AV2_QUANT_FP_BITS;
    let sign = coefficient.signum();
    let abs_coeff = i64::from(coefficient.abs());
    if (abs_coeff << (1 + AV2_QUANT_TABLE_BITS)) < i64::from(params.dequant[rc01]) {
        return 0;
    }
    let abs_qcoeff =
        ((abs_coeff + params.round_fp[rc01]) * params.quant_fp[rc01]) >> shift;
    (abs_qcoeff as i32) * sign
}

fn av2_regular_dequantize<const SAMPLES: usize>(
    qcoeff: &[i32; SAMPLES],
    dequant: [i32; 2],
) -> [i32; SAMPLES] {
    let mut dqcoeff = [0i32; SAMPLES];
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

// A DC-only transform produces a constant block.  Keep the two rounded and
// clipped stages identical to av2_idct4x4(), but avoid running the full
// separable transform when the regular-DCT candidate has no AC coefficients.
fn av2_idct4x4_dc_only(input: &[i32; TX4X4_SAMPLES], bit_depth: SampleBitDepth) -> [i32; TX4X4_SAMPLES] {
    debug_assert!(input[1..].iter().all(|&coefficient| coefficient == 0));
    let intermediate_bitdepth = i32::from(bit_depth.bits()) + 8;
    let rng_min = -(1 << (intermediate_bitdepth - 1));
    let rng_max = (1 << (intermediate_bitdepth - 1)) - 1;
    let col_rng_min = -(1 << bit_depth.bits());
    let col_rng_max = (1 << bit_depth.bits()) - 1;
    let first_stage = ((AV2_DCT4_KERNEL[0][0] * input[0] + (1 << 6)) >> 7)
        .clamp(rng_min, rng_max);
    let sample = ((AV2_DCT4_KERNEL[0][0] * first_stage + (1 << 9)) >> 10)
        .clamp(col_rng_min, col_rng_max);
    [sample; TX4X4_SAMPLES]
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

// The 8x8 DC-only inverse has the same separable structure as the full
// inverse, but only the first horizontal and vertical outputs can be nonzero.
fn av2_idct8x8_dc_only(input: &[i32; TX8X8_SAMPLES], bit_depth: SampleBitDepth) -> [i32; TX8X8_SAMPLES] {
    debug_assert!(input[1..].iter().all(|&coefficient| coefficient == 0));
    let intermediate_bitdepth = i32::from(bit_depth.bits()) + 8;
    let rng_min = -(1 << (intermediate_bitdepth - 1));
    let rng_max = (1 << (intermediate_bitdepth - 1)) - 1;
    let col_rng_min = -(1 << bit_depth.bits());
    let col_rng_max = (1 << bit_depth.bits()) - 1;
    let first_stage = ((AV2_DCT8_KERNEL[0][0] * input[0] + (1 << 6)) >> 7)
        .clamp(rng_min, rng_max);
    let sample = ((AV2_DCT8_KERNEL[0][0] * first_stage + (1 << 10)) >> 11)
        .clamp(col_rng_min, col_rng_max);
    [sample; TX8X8_SAMPLES]
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

#[cfg(test)]
mod dc_only_tests {
    use super::*;

    #[test]
    fn dc_only_inverse_matches_full_inverse() {
        for bit_depth in [
            SampleBitDepth::new(8).unwrap(),
            SampleBitDepth::new(10).unwrap(),
            SampleBitDepth::new(12).unwrap(),
        ] {
            for dc in [-32768, -1024, -1, 0, 1, 1024, 32767] {
                let mut coefficients = [0; TX4X4_SAMPLES];
                coefficients[0] = dc;
                assert_eq!(
                    av2_idct4x4_dc_only(&coefficients, bit_depth),
                    av2_idct4x4(&coefficients, bit_depth),
                    "DC-only inverse mismatch for {bit_depth:?}, dc={dc}"
                );
            }
        }
    }

    #[test]
    fn dc_only_inverse_8x8_matches_full_inverse() {
        for bit_depth in [
            SampleBitDepth::new(8).unwrap(),
            SampleBitDepth::new(10).unwrap(),
            SampleBitDepth::new(12).unwrap(),
        ] {
            for dc in [-32768, -1024, -1, 0, 1, 1024, 32767] {
                let mut coefficients = [0; TX8X8_SAMPLES];
                coefficients[0] = dc;
                assert_eq!(
                    av2_idct8x8_dc_only(&coefficients, bit_depth),
                    av2_idct8x8(&coefficients, bit_depth),
                    "8x8 DC-only inverse mismatch for {bit_depth:?}, dc={dc}"
                );
            }
        }
    }
}
