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
fn av2_idct4x4_dc_only(
    input: &[i32; TX4X4_SAMPLES],
    bit_depth: SampleBitDepth,
) -> [i32; TX4X4_SAMPLES] {
    debug_assert!(input[1..].iter().all(|&coefficient| coefficient == 0));
    let intermediate_bitdepth = i32::from(bit_depth.bits()) + 8;
    let rng_min = -(1 << (intermediate_bitdepth - 1));
    let rng_max = (1 << (intermediate_bitdepth - 1)) - 1;
    let col_rng_min = -(1 << bit_depth.bits());
    let col_rng_max = (1 << bit_depth.bits()) - 1;
    let first_stage = ((AV2_DCT4_KERNEL[0][0] * input[0] + (1 << 6)) >> 7).clamp(rng_min, rng_max);
    let sample =
        ((AV2_DCT4_KERNEL[0][0] * first_stage + (1 << 9)) >> 10).clamp(col_rng_min, col_rng_max);
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
fn av2_idct8x8_dc_only(
    input: &[i32; TX8X8_SAMPLES],
    bit_depth: SampleBitDepth,
) -> [i32; TX8X8_SAMPLES] {
    debug_assert!(input[1..].iter().all(|&coefficient| coefficient == 0));
    let intermediate_bitdepth = i32::from(bit_depth.bits()) + 8;
    let rng_min = -(1 << (intermediate_bitdepth - 1));
    let rng_max = (1 << (intermediate_bitdepth - 1)) - 1;
    let col_rng_min = -(1 << bit_depth.bits());
    let col_rng_max = (1 << bit_depth.bits()) - 1;
    let first_stage = ((AV2_DCT8_KERNEL[0][0] * input[0] + (1 << 6)) >> 7).clamp(rng_min, rng_max);
    let sample =
        ((AV2_DCT8_KERNEL[0][0] * first_stage + (1 << 10)) >> 11).clamp(col_rng_min, col_rng_max);
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

fn inv_dct4_shifted(input: &[i32; TX4X4_SIZE], shift: u8, min: i32, max: i32) -> [i32; TX4X4_SIZE] {
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

fn inv_dct8_shifted(input: &[i32; TX8X8_SIZE], shift: u8, min: i32, max: i32) -> [i32; TX8X8_SIZE] {
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
            output[(k + 4) * line + j] = ((a[3 - k] - b[3 - k] + add) >> shift).clamp(min, max);
        }
    }
    output
}

fn inv_dct4_pass(
    input: &[i32; TX4X4_SAMPLES],
    shift: u8,
    min: i32,
    max: i32,
) -> [i32; TX4X4_SAMPLES] {
    let mut output = [0i32; TX4X4_SAMPLES];
    let add = 1 << (shift - 1);
    for j in 0..TX4X4_SIZE {
        let src = j * TX4X4_SIZE;
        let b0 = AV2_DCT4_KERNEL[1][0] * input[src + 1] + AV2_DCT4_KERNEL[3][0] * input[src + 3];
        let b1 = AV2_DCT4_KERNEL[1][1] * input[src + 1] + AV2_DCT4_KERNEL[3][1] * input[src + 3];
        let a0 = AV2_DCT4_KERNEL[0][0] * input[src] + AV2_DCT4_KERNEL[2][0] * input[src + 2];
        let a1 = AV2_DCT4_KERNEL[0][1] * input[src] + AV2_DCT4_KERNEL[2][1] * input[src + 2];
        output[j] = ((a0 + b0 + add) >> shift).clamp(min, max);
        output[TX4X4_SIZE + j] = ((a1 + b1 + add) >> shift).clamp(min, max);
        output[2 * TX4X4_SIZE + j] = ((a1 - b1 + add) >> shift).clamp(min, max);
        output[3 * TX4X4_SIZE + j] = ((a0 - b0 + add) >> shift).clamp(min, max);
    }
    output
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
