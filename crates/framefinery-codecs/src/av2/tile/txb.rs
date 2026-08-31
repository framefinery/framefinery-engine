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
include!("txb_quantization.rs");
include!("txb_forward_transform.rs");
include!("txb_inverse_transform.rs");

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
            checksum =
                checksum.rotate_left(5) ^ (*value as i64 as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    checksum
}
