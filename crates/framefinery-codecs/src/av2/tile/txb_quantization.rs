const AV2_QUANT_TABLE_BITS: u8 = 3;
const AV2_QUANT_FP_BITS: u8 = 4;
const AV2_QLOOKUP_QTX: [i32; 25] = [
    64, 40, 41, 43, 44, 45, 47, 48, 49, 51, 52, 54, 55, 57, 59, 60, 62, 64, 66, 68, 70, 72, 74, 76,
    78,
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
    av2_regular_quantize_with_params(coefficients, params)
}

fn av2_regular_quantize_with_params<const SAMPLES: usize>(
    coefficients: &[i32; SAMPLES],
    params: Av2RegularQuantParams,
) -> ([i32; SAMPLES], [i32; SAMPLES]) {
    let mut qcoeff = [0i32; SAMPLES];
    for pos in 0..SAMPLES {
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
    let abs_qcoeff = ((abs_coeff + params.round_fp[rc01]) * params.quant_fp[rc01]) >> shift;
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

fn av2_regular_quantized_level_coefficients<const SAMPLES: usize>(
    qcoeff: &[i32; SAMPLES],
) -> [i32; SAMPLES] {
    let mut coefficients = [0i32; SAMPLES];
    for (dst, &level) in coefficients.iter_mut().zip(qcoeff.iter()) {
        *dst = level * 8;
    }
    coefficients
}

#[cfg(test)]
mod regular_quantization_tests {
    use super::*;

    fn expected_regular_quantization<const SAMPLES: usize>(
        coefficients: &[i32; SAMPLES],
        params: Av2RegularQuantParams,
    ) -> ([i32; SAMPLES], [i32; SAMPLES]) {
        let mut qcoeff = [0; SAMPLES];
        for pos in 0..SAMPLES {
            qcoeff[pos] = av2_regular_quantize_coefficient(coefficients[pos], params, pos != 0);
        }
        let dqcoeff = av2_regular_dequantize(&qcoeff, params.dequant);
        (qcoeff, dqcoeff)
    }

    fn expected_quantized_levels<const SAMPLES: usize>(qcoeff: &[i32; SAMPLES]) -> [i32; SAMPLES] {
        let mut coefficients = [0; SAMPLES];
        for (dst, &level) in coefficients.iter_mut().zip(qcoeff) {
            *dst = level * 8;
        }
        coefficients
    }

    #[test]
    fn regular_quantization_contract_matches_all_transform_sizes() {
        let bit_depth = SampleBitDepth::new(10).unwrap();
        let params = Av2RegularQuantParams::new(91, bit_depth);

        let coefficients_4x4: [i32; TX4X4_SAMPLES] =
            std::array::from_fn(|pos| pos as i32 * 97 - 511);
        let expected_4x4 = expected_regular_quantization(&coefficients_4x4, params);
        assert_eq!(
            av2_regular_quantize_with_params(&coefficients_4x4, params),
            expected_4x4
        );
        assert_eq!(
            av2_regular_quantized_level_coefficients(&expected_4x4.0),
            expected_quantized_levels(&expected_4x4.0)
        );

        let coefficients_8x8: [i32; TX8X8_SAMPLES] =
            std::array::from_fn(|pos| pos as i32 * 43 - 997);
        let expected_8x8 = expected_regular_quantization(&coefficients_8x8, params);
        assert_eq!(
            av2_regular_quantize_with_params(&coefficients_8x8, params),
            expected_8x8
        );
        assert_eq!(
            av2_regular_quantized_level_coefficients(&expected_8x8.0),
            expected_quantized_levels(&expected_8x8.0)
        );

        let coefficients_4x8: [i32; TX4X8_SAMPLES] =
            std::array::from_fn(|pos| pos as i32 * 61 - 733);
        let expected_4x8 = expected_regular_quantization(&coefficients_4x8, params);
        assert_eq!(
            av2_regular_quantize_with_params(&coefficients_4x8, params),
            expected_4x8
        );
        assert_eq!(
            av2_regular_quantized_level_coefficients(&expected_4x8.0),
            expected_quantized_levels(&expected_4x8.0)
        );
    }
}
