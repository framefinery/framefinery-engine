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
        fdct4x4_pass(
            &in_high,
            &mut intermediate[col * TX4X4_SIZE..][..TX4X4_SIZE],
        );
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
        i64::from(x2) * i64::from(AV2_COSPI_24_64) + i64::from(x3) * i64::from(AV2_COSPI_8_64),
    );
    output[4] = fdct_round_shift(i64::from(x0 - x1) * i64::from(AV2_COSPI_16_64));
    output[6] = fdct_round_shift(
        -i64::from(x2) * i64::from(AV2_COSPI_8_64) + i64::from(x3) * i64::from(AV2_COSPI_24_64),
    );

    let t0 = fdct_round_shift(i64::from(s6 - s5) * i64::from(AV2_COSPI_16_64));
    let t1 = fdct_round_shift(i64::from(s6 + s5) * i64::from(AV2_COSPI_16_64));
    let x0 = s4 + t0;
    let x1 = s4 - t0;
    let x2 = s7 - t1;
    let x3 = s7 + t1;
    output[1] = fdct_round_shift(
        i64::from(x0) * i64::from(AV2_COSPI_28_64) + i64::from(x3) * i64::from(AV2_COSPI_4_64),
    );
    output[3] = fdct_round_shift(
        i64::from(x2) * i64::from(AV2_COSPI_12_64) - i64::from(x1) * i64::from(AV2_COSPI_20_64),
    );
    output[5] = fdct_round_shift(
        i64::from(x1) * i64::from(AV2_COSPI_12_64) + i64::from(x2) * i64::from(AV2_COSPI_20_64),
    );
    output[7] = fdct_round_shift(
        i64::from(x3) * i64::from(AV2_COSPI_28_64) - i64::from(x0) * i64::from(AV2_COSPI_4_64),
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
